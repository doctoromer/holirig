use anyhow::Result;
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::hash::Hash;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info};

use super::Notification;
use crate::interfaces::jsonrpc::{Request, Response, RigRpcHandler, RpcError};
use crate::resources::Resources;
use crate::rig_settings::{RigId, RigSettings};
use crate::serial::manager::{ConnectionStatus, ManagerCommand, ManagerMessage, StatusCache};

fn encode_message(message: &impl Serialize) -> Result<Vec<u8>> {
    let mut data = serde_json::to_vec(message)?;
    data.push(b'\n');
    Ok(data)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ConnectionId(u64);

impl ConnectionId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

struct ClientSession {
    id: ConnectionId,
    subscriptions: RwLock<HashMap<RigId, Vec<String>>>,
}

impl ClientSession {
    fn new() -> Self {
        Self {
            id: ConnectionId::new(),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }

    fn subscribe(&self, rig_id: RigId, fields: Vec<String>) {
        self.subscriptions.write().insert(rig_id, fields);
    }

    fn unsubscribe(&self, rig_id: RigId) {
        self.subscriptions.write().remove(&rig_id);
    }

    fn get_subscribed_fields(&self, rig_id: RigId) -> Option<Vec<String>> {
        self.subscriptions.read().get(&rig_id).cloned()
    }
}

struct ServerState {
    handlers: Arc<HashMap<String, RigRpcHandler>>,
    rigs_state: Arc<RwLock<HashMap<RigId, (String, bool)>>>,
    clients: Arc<RwLock<HashMap<ConnectionId, Arc<ClientSession>>>>,
    status_cache: StatusCache,
    manager_rx: broadcast::Receiver<ManagerMessage>,
    notification_tx: broadcast::Sender<Notification>,
}

impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            handlers: self.handlers.clone(),
            rigs_state: self.rigs_state.clone(),
            clients: self.clients.clone(),
            status_cache: self.status_cache.clone(),
            manager_rx: self.manager_rx.resubscribe(),
            notification_tx: self.notification_tx.clone(),
        }
    }
}

impl ServerState {
    fn handle_manager_message(&self, message: ManagerMessage) {
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
    }

    fn register_client(&self, session: Arc<ClientSession>) {
        self.clients.write().insert(session.id, session);
    }

    fn unregister_client(&self, id: ConnectionId) {
        self.clients.write().remove(&id);
    }
}

pub struct JsonRpcServer {
    listener: TcpListener,
    state: ServerState,
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

        let state = ServerState {
            handlers: Arc::new(handlers),
            rigs_state: Arc::new(RwLock::new(rigs_state)),
            clients: Arc::new(RwLock::new(HashMap::new())),
            status_cache,
            manager_rx,
            notification_tx,
        };

        Ok(Self { listener, state })
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
                            let state = self.state.clone();
                            tokio::spawn(async move {
                                if let Err(err) = handle_client(stream, addr, state).await {
                                    error!(%addr, %err, "Client connection error");
                                }
                            });
                        }
                        Err(err) => {
                            error!(listen_addr = ?self.listener.local_addr(), %err, "Failed to accept connection");
                        }
                    }
                }
                message = self.state.manager_rx.recv() => {
                    match message {
                        Ok(msg) => {
                            self.state.handle_manager_message(msg);
                        }
                        Err(_) => return Ok(()),
                    }
                }
            }
        }
    }
}

async fn handle_client(stream: TcpStream, addr: SocketAddr, server: ServerState) -> Result<()> {
    info!(%addr, "New client connection");

    let session = Arc::new(ClientSession::new());
    let session_id = session.id;

    server.register_client(session.clone());

    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();
    let mut notification_rx = server.notification_tx.subscribe();

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
                                let response = handle_request(&server, &session, request).await;
                                let msg = encode_message(&response)?;
                                if writer.write_all(&msg).await.is_err() {
                                    error!(%addr, "Failed to send response");
                                    break;
                                }
                            }
                            Err(err) => {
                                error!(%err, "Invalid JSON");
                                let response = Response::build_error(RpcError::parse_error(&err));
                                let msg = encode_message(&response)?;
                                if writer.write_all(&msg).await.is_err() {
                                    error!(%addr, "Failed to send error response");
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
                    Ok(n) => {
                        if !send_notification_if_relevant(&mut writer, &session, &n).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    server.unregister_client(session_id);
    info!(%addr, "Client connection closed");
    Ok(())
}

async fn send_notification_if_relevant(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    session: &Arc<ClientSession>,
    notification: &Notification,
) -> bool {
    let Some(rig_id) = notification
        .params
        .get("rig_id")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<usize>().ok().map(RigId)))
    else {
        return true;
    };

    if let Some(fields) = session.get_subscribed_fields(rig_id) {
        // Filter updates to only requested fields
        if let Some(updates) = notification
            .params
            .get("updates")
            .and_then(|v| v.as_object())
        {
            let filtered: serde_json::Map<_, _> = updates
                .iter()
                .filter(|(key, _)| fields.contains(key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();

            if filtered.is_empty() {
                return true;
            }

            let filtered_notification = Notification {
                jsonrpc: notification.jsonrpc.clone(),
                method: notification.method.clone(),
                params: json!({
                    "rig_id": rig_id,
                    "updates": filtered,
                }),
            };

            match encode_message(&filtered_notification) {
                Ok(msg) => {
                    if writer.write_all(&msg).await.is_err() {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    } else if notification.method == "connection_update" {
        // Send connection updates to all clients
        match encode_message(notification) {
            Ok(msg) => {
                if writer.write_all(&msg).await.is_err() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }

    true
}

async fn handle_request(
    server: &ServerState,
    session: &Arc<ClientSession>,
    request: Request,
) -> Response {
    let method = request.method.as_str();

    if method == "list_rigs" {
        let rigs = serde_json::Value::Object(
            server
                .rigs_state
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
    }

    let Some(id) = request.get_rig_id() else {
        return Response::build_error(RpcError::missing_rig_id().with_id(&request.id));
    };

    match method {
        "get_status" => {
            let cached = server.status_cache.read();
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
                    let rigs_state_lock = server.rigs_state.read();
                    if let Some((rig_model, _)) = rigs_state_lock.get(&id)
                        && let Some(handler) = server.handlers.get(rig_model)
                    {
                        match handler.check_fields(&fields) {
                            Ok(_) => {
                                session.subscribe(id, fields);
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
        "unsubscribe_status" => {
            session.unsubscribe(id);
            Response::build_success(request.id)
        }
        _ => {
            let rig_model = {
                let rigs_state_lock = server.rigs_state.read();
                rigs_state_lock.get(&id).map(|(m, _)| m.clone())
            };

            if let Some(rig_model) = rig_model
                && let Some(handler) = server.handlers.get(&rig_model)
            {
                match handler.handle_request(&request, id).await {
                    Ok(request) => request,
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
