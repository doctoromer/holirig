use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::RpcError;
use crate::rig_settings::RigId;

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

impl Request {
    pub fn get_rig_id(&self) -> Option<RigId> {
        if let Some(Value::Object(params)) = &self.params
            && let Some(Value::Number(id)) = params.get("rig_id")
        {
            Some(serde_json::from_value(Value::Number(id.clone())).ok()?)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: Id,
}

impl Response {
    pub fn build_result(id: Id, result: Value) -> Response {
        Response {
            jsonrpc: super::VERSION.into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn build_success(id: Id) -> Self {
        Response {
            jsonrpc: super::VERSION.into(),
            result: Some(json!({"result": "success"})),
            error: None,
            id,
        }
    }

    pub fn build_error(mut error: RpcError) -> Self {
        let id = std::mem::take(&mut error.id);
        Response {
            jsonrpc: super::VERSION.into(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}
