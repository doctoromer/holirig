use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

pub enum ServerMessage {
    Response(Response),
    Notification(Notification),
}

pub enum ParsedNotification {
    StatusUpdate {
        rig_id: usize,
        updates: HashMap<String, Value>,
    },
    ConnectionUpdate {
        rig_id: usize,
        connected: bool,
    },
    Unknown,
}

impl Notification {
    pub fn parse(self) -> ParsedNotification {
        let rig_id = self
            .params
            .get("rig_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        match (self.method.as_str(), rig_id) {
            ("status_update", Some(rig_id)) => {
                if let Some(obj) = self.params.get("updates").and_then(|v| v.as_object()) {
                    let updates = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    return ParsedNotification::StatusUpdate { rig_id, updates };
                }
                ParsedNotification::Unknown
            }
            ("connection_update", Some(rig_id)) => {
                if let Some(connected) = self.params.get("connected").and_then(|v| v.as_bool()) {
                    return ParsedNotification::ConnectionUpdate { rig_id, connected };
                }
                ParsedNotification::Unknown
            }
            _ => ParsedNotification::Unknown,
        }
    }
}

pub fn parse_server_message(data: &[u8]) -> anyhow::Result<ServerMessage> {
    let value: Value = serde_json::from_slice(data)?;
    if value.get("id").is_some() {
        Ok(ServerMessage::Response(serde_json::from_value(value)?))
    } else {
        Ok(ServerMessage::Notification(serde_json::from_value(value)?))
    }
}
