use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn next_id() -> Id {
    Id::Number(NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum Id {
    #[default]
    Null,
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    pub id: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub result: Option<Value>,
    pub error: Option<Value>,
    pub id: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

pub fn list_rigs_request() -> Request {
    Request {
        jsonrpc: "2.0".into(),
        method: "list_rigs".into(),
        params: None,
        id: next_id(),
    }
}

pub fn get_capabilities_request(rig_id: usize) -> Request {
    Request {
        jsonrpc: "2.0".into(),
        method: "get_capabilities".into(),
        params: Some(json!({"rig_id": rig_id})),
        id: next_id(),
    }
}

pub fn subscribe_status_request(rig_id: usize, fields: Vec<String>) -> Request {
    Request {
        jsonrpc: "2.0".into(),
        method: "subscribe_status".into(),
        params: Some(json!({"rig_id": rig_id, "fields": fields})),
        id: next_id(),
    }
}

pub fn get_status_request(rig_id: usize) -> Request {
    Request {
        jsonrpc: "2.0".into(),
        method: "get_status".into(),
        params: Some(json!({"rig_id": rig_id})),
        id: next_id(),
    }
}

pub fn execute_command_request(
    rig_id: usize,
    command: String,
    parameters: HashMap<String, Value>,
) -> Request {
    Request {
        jsonrpc: "2.0".into(),
        method: "execute_command".into(),
        params: Some(json!({
            "rig_id": rig_id,
            "command": command,
            "parameters": parameters,
        })),
        id: next_id(),
    }
}

pub enum ServerMessage {
    Response(Response),
    Notification(Notification),
}

pub fn parse_server_message(data: &[u8]) -> anyhow::Result<ServerMessage> {
    let value: Value = serde_json::from_slice(data)?;
    if value.get("id").is_some() {
        Ok(ServerMessage::Response(serde_json::from_value(value)?))
    } else {
        Ok(ServerMessage::Notification(serde_json::from_value(value)?))
    }
}
