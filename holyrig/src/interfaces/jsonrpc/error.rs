use std::fmt::Display;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::interfaces::jsonrpc::types::Id;
use crate::rig_settings::RigId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip)]
    pub id: Id,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RpcError {
    // Standard JSON-RPC error codes
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Custom error codes for rig operations
    pub const RIG_COMMUNICATION_ERROR: i32 = -32000;
    pub const INVALID_COMMAND_PARAMS: i32 = -32001;
    pub const SUBSCRIPTION_ERROR: i32 = -32002;
    pub const MISSING_RIG_ID: i32 = -32003;
    pub const UNKNOWN_RIG_ID: i32 = -32004;
    pub const UNKNOWN_COMMAND: i32 = -32005;
    pub const UNKNOWN_FIELDS: i32 = -32006;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
            id: Default::default(),
        }
    }

    pub fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
            id: Default::default(),
        }
    }

    pub fn with_id(mut self, id: &Id) -> Self {
        self.id = id.clone();
        self
    }

    pub fn parse_error(message: &impl Display) -> Self {
        Self::new(Self::PARSE_ERROR, format!("Parse error: {message}"))
    }

    pub fn invalid_request() -> Self {
        Self::new(Self::INVALID_REQUEST, "Invalid Request")
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            Self::METHOD_NOT_FOUND,
            format!("Method not found: {method}"),
        )
    }

    pub fn invalid_params() -> Self {
        Self::new(Self::INVALID_PARAMS, "Invalid params")
    }

    pub fn internal_error() -> Self {
        Self::new(Self::INTERNAL_ERROR, "Internal error")
    }

    pub fn rig_communication_error(msg: impl Into<String>) -> Self {
        Self::new(Self::RIG_COMMUNICATION_ERROR, msg)
    }

    pub fn invalid_command_params(msg: impl Into<String>) -> Self {
        Self::new(Self::INVALID_COMMAND_PARAMS, msg)
    }

    pub fn subscription_error(msg: impl Into<String>) -> Self {
        Self::new(Self::SUBSCRIPTION_ERROR, msg)
    }

    pub fn missing_rig_id() -> Self {
        Self::new(Self::MISSING_RIG_ID, "Missing rig id")
    }

    pub fn unknown_rig_id(rig_id: RigId) -> Self {
        Self::new(Self::UNKNOWN_RIG_ID, format!("Unknown rig id: {rig_id}"))
    }

    pub fn unknown_command(command: &str) -> Self {
        Self::new(Self::UNKNOWN_COMMAND, format!("Unknown command: {command}"))
    }
    pub fn unknown_fields(fields: Vec<String>) -> Self {
        Self::new(
            Self::UNKNOWN_FIELDS,
            format!("Unknown fields: {}", fields.join(", ")),
        )
    }
}
