use std::collections::HashMap;
use std::net::SocketAddr;

use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::capabilities::{Capabilities, parse_capabilities};
use crate::net::{TcpClient, TcpSender};
use crate::protocol::{Id, Request, Response, ServerMessage};

pub struct RigInfo {
    pub id: usize,
    pub connected: bool,
    pub capabilities: Capabilities,
    pub status: HashMap<String, Value>,
}

pub struct HolyrigClient {
    sender: TcpSender,
    next_id: i64,
    pub rigs: HashMap<usize, RigInfo>,
}

impl HolyrigClient {
    /// Connects to the server, runs the full init sequence, spawns the receiver
    /// loop (forwarding all incoming messages to `notif_tx`), and returns the
    /// ready-to-use client.
    pub async fn connect(addr: SocketAddr, notif_tx: mpsc::Sender<ServerMessage>) -> Result<Self> {
        let mut tcp = TcpClient::connect(addr).await?;
        let mut next_id = 1i64;

        let rigs = init(&mut tcp, &mut next_id).await?;

        let (sender, receiver) = tcp.into_split();
        tokio::spawn(async move {
            let _ = receiver.run(notif_tx).await;
        });

        Ok(Self {
            sender,
            next_id,
            rigs,
        })
    }

    pub async fn execute_command(
        &mut self,
        rig_id: usize,
        command: String,
        parameters: HashMap<String, Value>,
    ) -> Result<()> {
        let req = self.build_request(
            "execute_command",
            Some(json!({ "rig_id": rig_id, "command": command, "parameters": parameters })),
        );
        self.sender.send_request(&req).await
    }

    pub fn list_rigs_request(&mut self) -> Request {
        self.build_request("list_rigs", None)
    }

    pub fn execute_command_request(
        &mut self,
        rig_id: usize,
        command: String,
        parameters: HashMap<String, Value>,
    ) -> Request {
        self.build_request(
            "execute_command",
            Some(json!({ "rig_id": rig_id, "command": command, "parameters": parameters })),
        )
    }

    pub async fn send_request(&mut self, req: &Request) -> Result<()> {
        self.sender.send_request(req).await
    }

    fn build_request(&mut self, method: &str, params: Option<Value>) -> Request {
        let id = self.next_id;
        self.next_id += 1;
        Request {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id: Id::Number(id),
        }
    }
}

async fn init(tcp: &mut TcpClient, next_id: &mut i64) -> Result<HashMap<usize, RigInfo>> {
    let rigs_value = match send_and_wait(tcp, next_id, "list_rigs", None).await {
        Ok(resp) => resp.result.unwrap_or(Value::Null),
        Err(e) => bail!("list_rigs failed: {e}"),
    };

    let rig_entries: Vec<(usize, bool)> = match &rigs_value {
        Value::Object(map) => map
            .iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_bool().unwrap_or(false))))
            .collect(),
        _ => bail!("Unexpected response from list_rigs"),
    };

    if rig_entries.is_empty() {
        bail!("Server has no rigs configured");
    }

    let mut rigs = HashMap::new();

    for (rig_id, connected) in rig_entries {
        let caps = match send_and_wait(
            tcp,
            next_id,
            "get_capabilities",
            Some(json!({"rig_id": rig_id})),
        )
        .await
        {
            Ok(resp) => resp
                .result
                .map(|v| parse_capabilities(&v))
                .unwrap_or_default(),
            Err(e) => bail!("get_capabilities failed for rig {rig_id}: {e}"),
        };

        let fields: Vec<String> = caps.status_fields.keys().cloned().collect();
        if !fields.is_empty() {
            let _ = send_and_wait(
                tcp,
                next_id,
                "subscribe_status",
                Some(json!({"rig_id": rig_id, "fields": fields})),
            )
            .await;
        }

        let status: HashMap<String, Value> =
            send_and_wait(tcp, next_id, "get_status", Some(json!({"rig_id": rig_id})))
                .await
                .ok()
                .and_then(|r| r.result)
                .and_then(|v| {
                    if let Value::Object(m) = v {
                        Some(m.into_iter().collect())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

        rigs.insert(
            rig_id,
            RigInfo {
                id: rig_id,
                connected,
                capabilities: caps,
                status,
            },
        );
    }

    Ok(rigs)
}

async fn send_and_wait(
    tcp: &mut TcpClient,
    next_id: &mut i64,
    method: &str,
    params: Option<Value>,
) -> Result<Response> {
    let id = *next_id;
    *next_id += 1;
    let req = Request {
        jsonrpc: "2.0".into(),
        method: method.into(),
        params,
        id: Id::Number(id),
    };
    tcp.send_and_wait(&req).await
}
