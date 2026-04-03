use anyhow::Result;
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info};

use super::{Notification, RigRpcHandler};
use crate::interfaces::jsonrpc::{Request, Response, RpcError};
use crate::resources::Resources;
use crate::rig_settings::{RigId, RigSettings};
use crate::serial::manager::{ConnectionStatus, ManagerCommand, ManagerMessage, StatusCache};

type Subscriptions = HashMap<(RigId, SocketAddr), Vec<String>>;

fn encode_message(message: &impl Serialize) -> Result<Vec<u8>> {
    let data = serde_json::to_vec(message)?;
    let mut frame = Vec::with_capacity(data.len() + 1);
    frame.extend_from_slice(&data);
    frame.push(b'\n');
    Ok(frame)
}

pub struct JsonRpcServer {
    listener: TcpListener,
    handlers: Arc<HashMap<String, RigRpcHandler>>,
    rigs_state: Arc<RwLock<HashMap<RigId, (String, bool)>>>,
    registered_status: Arc<RwLock<Subscriptions>>,
    status_cache: StatusCache,
    manager_rx: broadcast::Receiver<ManagerMessage>,
    notification_tx: broadcast::Sender<Notification>,
}

impl JsonRpcServer {
    pub async fn new(
        bind_address: &str,
        port: u16,
        resources: Arc<Resources>,
        command_tx: mpsc::Sender<ManagerCommand>,
        manager_rx: broadcast::Receiver<ManagerMessage>,
        status_cache: StatusCache,
        initial_rigs: &[RigSettings],
    ) -> Result<Self> {
        let handlers = resources
            .rigs
            .iter()
            .map(|(rig_name, interpreter)| {
                let rig_file = interpreter.rig_file();
                let schema = resources.schemas.get(&rig_file.impl_block.schema).unwrap();
                let handler = RigRpcHandler::new(rig_file, schema, command_tx.clone());
                (rig_name.clone(), handler)
            })
            .collect();

        let rigs_state: HashMap<RigId, (String, bool)> = initial_rigs
            .iter()
            .map(|rig| (rig.id, (rig.config.rig_type.clone(), false)))
            .collect();

        let addr = format!("{}:{}", bind_address, port);
        let listener = TcpListener::bind(&addr).await?;
        let (notification_tx, _) = broadcast::channel(64);

        Ok(Self {
            listener,
            handlers: Arc::new(handlers),
            rigs_state: Arc::new(RwLock::new(rigs_state)),
            registered_status: Arc::new(RwLock::new(HashMap::new())),
            status_cache,
            manager_rx,
            notification_tx,
        })
    }

    async fn spawn_client_task(&self, stream: TcpStream, addr: SocketAddr) {
        let handlers = self.handlers.clone();
        let rigs_state = self.rigs_state.clone();
        let registered_status = self.registered_status.clone();
        let status_cache = self.status_cache.clone();
        let notification_rx = self.notification_tx.subscribe();

        tokio::spawn(async move {
            if let Err(err) = handle_client(
                stream,
                addr,
                handlers,
                rigs_state,
                registered_status,
                status_cache,
                notification_rx,
            )
            .await
            {
                error!(%addr, %err, "Client connection error");
            }
        });
    }

    pub async fn run(mut self) -> Result<()> {
        info!(
            "JSON-RPC TCP server listening on {}",
            self.listener.local_addr()?
        );

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            self.spawn_client_task(stream, addr).await;
                        }
                        Err(err) => {
                            error!(listen_addr = ?self.listener.local_addr(), %err, "Failed to accept connection");
                        }
                    }
                }
                message = self.manager_rx.recv() => {
                    match message {
                        Ok(msg) => {
                            if let Err(err) = self.handle_manager_message(msg).await {
                                error!(%err, "Error handling manager message");
                            }
                        }
                        Err(_) => return Ok(()),
                    }
                }
            }
        }
    }

    async fn handle_manager_message(&self, message: ManagerMessage) -> Result<()> {
        match message {
            ManagerMessage::ConnectionStatusChanged { device_id, status } => {
                let connected = matches!(status, ConnectionStatus::Connected);
                self.rigs_state
                    .write()
                    .entry(device_id)
                    .and_modify(|(_, is_connected)| {
                        *is_connected = connected;
                    });

                let notification = Notification {
                    jsonrpc: super::VERSION.into(),
                    method: "connection_update".to_string(),
                    params: json!({
                        "rig_id": device_id,
                        "connected": connected,
                    }),
                };

                let _ = self.notification_tx.send(notification);
            }
            ManagerMessage::AvailablePorts(_) => {}
            ManagerMessage::StatusUpdate { device_id, values } => {
                let values: HashMap<_, _> = values
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect();

                let notification = Notification {
                    jsonrpc: super::VERSION.into(),
                    method: "status_update".to_string(),
                    params: json!({
                        "rig_id": device_id,
                        "updates": values,
                    }),
                };

                let _ = self.notification_tx.send(notification);
            }
        }
        Ok(())
    }
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    handlers: Arc<HashMap<String, RigRpcHandler>>,
    rigs_state: Arc<RwLock<HashMap<RigId, (String, bool)>>>,
    registered_status: Arc<RwLock<Subscriptions>>,
    status_cache: StatusCache,
    mut notification_rx: broadcast::Receiver<Notification>,
) -> Result<()> {
    info!(%addr, "New client connection");
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();

        tokio::select! {
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = line.trim_end();
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Request>(line) {
                            Ok(request) => {
                                debug!(method = %request.method, "Received request");
                                let response = handle_request(
                                    &handlers,
                                    &rigs_state,
                                    &registered_status,
                                    &status_cache,
                                    &addr,
                                    request,
                                )
                                .await;

                                let msg = encode_message(&response)?;
                                writer.write_all(&msg).await?;
                            }
                            Err(err) => {
                                error!(%err, "Invalid JSON");
                                let response = Response::build_error(RpcError::parse_error(&err));
                                let msg = encode_message(&response)?;
                                if writer.write_all(&msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        error!(%err, "Read error");
                        break;
                    }
                }
            }
            notification = notification_rx.recv() => {
                match notification {
                    Ok(notification) => {
                        // Extract rig_id from notification and check subscriptions
                        let Some(rig_id) = notification.params.get("rig_id").and_then(|v| {
                            v.as_str().and_then(|s| s.parse::<usize>().ok().map(RigId))
                        }) else {
                            continue;
                        };

                        let fields = {
                            let subscriptions = registered_status.read();
                            subscriptions.get(&(rig_id, addr)).cloned()
                        };

                        if let Some(fields) = fields {
                            // Filter updates to only requested fields
                            if let Some(updates) = notification.params.get("updates").and_then(|v| v.as_object()) {
                                let filtered: serde_json::Map<_, _> = updates
                                    .iter()
                                    .filter(|(key, _)| fields.contains(key))
                                    .map(|(key, value)| (key.clone(), value.clone()))
                                    .collect();

                                let filtered_notification = Notification {
                                    jsonrpc: notification.jsonrpc.clone(),
                                    method: notification.method.clone(),
                                    params: json!({
                                        "rig_id": rig_id,
                                        "updates": filtered,
                                    }),
                                };
                                let msg = encode_message(&filtered_notification)?;
                                if writer.write_all(&msg).await.is_err() {
                                    break;
                                }
                            }
                        } else if notification.method == "connection_update" {
                            let msg = encode_message(&notification)?;
                            if writer.write_all(&msg).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    info!(%addr, "Client connection closed");
    Ok(())
}

async fn handle_request(
    handlers: &Arc<HashMap<String, RigRpcHandler>>,
    rigs_state: &Arc<RwLock<HashMap<RigId, (String, bool)>>>,
    registered_status: &Arc<RwLock<Subscriptions>>,
    status_cache: &StatusCache,
    addr: &SocketAddr,
    request: Request,
) -> Response {
    let method = request.method.as_str();
    if method == "list_rigs" {
        let rigs = serde_json::Value::Object(
            rigs_state
                .read()
                .iter()
                .map(|(device_id, (_, is_connected))| {
                    (
                        device_id.to_string(),
                        serde_json::Value::Bool(*is_connected),
                    )
                })
                .collect(),
        );
        return Response::build_result(request.id, rigs);
    };

    let Some(id) = request.get_rig_id() else {
        return Response::build_error(RpcError::missing_rig_id().with_id(&request.id));
    };

    match request.method.as_str() {
        "get_status" => {
            let cached = status_cache.read();
            let values = cached.get(&id).cloned().unwrap_or_default();
            let json_values: serde_json::Map<String, serde_json::Value> =
                values.into_iter().collect();
            Response::build_result(request.id, serde_json::Value::Object(json_values))
        }
        "subscribe_status" => {
            let fields = request
                .params
                .as_ref()
                .and_then(|params| params.as_object())
                .and_then(|params| params.get("fields"))
                .and_then(|fields| fields.as_array())
                .and_then(|fields| {
                    fields
                        .iter()
                        .map(|field| field.as_str().map(|field| field.to_string()))
                        .collect::<Option<Vec<_>>>()
                });

            match fields {
                Some(fields) => {
                    let rigs_state_lock = rigs_state.read();
                    if let Some((rig_model, _)) = rigs_state_lock.get(&id)
                        && let Some(handler) = handlers.get(rig_model)
                    {
                        match handler.check_fields(&fields) {
                            Ok(_) => {
                                registered_status.write().insert((id, *addr), fields);
                                Response::build_success(request.id)
                            }
                            Err(bad_fields) => Response::build_error(
                                RpcError::unknown_fields(bad_fields).with_id(&request.id),
                            ),
                        }
                    } else {
                        Response::build_error(RpcError::unknown_rig_id(id).with_id(&request.id))
                    }
                }
                None => Response::build_error(RpcError::invalid_params().with_id(&request.id)),
            }
        }
        _ => {
            let rig_model = {
                let rigs_state_lock = rigs_state.read();
                rigs_state_lock.get(&id).map(|(m, _)| m.clone())
            };

            if let Some(rig_model) = rig_model
                && let Some(handler) = handlers.get(&rig_model)
            {
                match handler.handle_request(&request, id).await {
                    Ok(r) => r,
                    Err(err) => {
                        Response::build_error(RpcError::rig_communication_error(err.to_string()))
                    }
                }
            } else {
                Response::build_error(RpcError::unknown_rig_id(id).with_id(&request.id))
            }
        }
    }
}
