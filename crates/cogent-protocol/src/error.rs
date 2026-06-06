//! Error types for the Cogent Protocol

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// JSON-RPC 2.0 standard error codes
pub mod codes {
    // Standard JSON-RPC 2.0
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Cogent-specific application errors (-32000 to -32099)
    pub const WORKSPACE_NOT_FOUND: i32 = -32000;
    pub const RULE_NOT_SUPPORTED: i32 = -32001;
    pub const RULE_PACK_NOT_FOUND: i32 = -32002;
    pub const BASELINE_NOT_FOUND: i32 = -32003;
    pub const INCREMENTAL_STATE_CORRUPT: i32 = -32004;
    pub const REMEDIATION_NOT_SUPPORTED: i32 = -32005;
    pub const AUTHENTICATION_REQUIRED: i32 = -32006;
    pub const QUOTA_EXCEEDED: i32 = -32007;
}

/// JSON-RPC 2.0 Error Object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Short error message
    pub message: String,
    /// Additional data (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Create a new JSON-RPC error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Create error with additional data
    pub fn with_data(code: i32, message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// Protocol-level error type
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// JSON-RPC parse error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Invalid JSON-RPC request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Method not found
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid method parameters
    #[error("Invalid params: {0}")]
    InvalidParams(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Workspace not found
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    /// Rule not supported by this provider
    #[error("Rule not supported: {0}")]
    RuleNotSupported(String),

    /// Rule pack not found
    #[error("Rule pack not found: {0}")]
    RulePackNotFound(String),

    /// Baseline not found
    #[error("Baseline not found: {0}")]
    BaselineNotFound(String),

    /// Incremental state corrupt
    #[error("Incremental state corrupt: {0}")]
    IncrementalStateCorrupt(String),

    /// Remediation not supported for this finding
    #[error("Remediation not supported: {0}")]
    RemediationNotSupported(String),

    /// Authentication required
    #[error("Authentication required: {0}")]
    AuthenticationRequired(String),

    /// Quota exceeded
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Generic error with code
    #[error("Protocol error {code}: {message}")]
    WithCode { code: i32, message: String },
}

impl ProtocolError {
    /// Convert to JSON-RPC error object
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        match self {
            ProtocolError::ParseError(msg) => JsonRpcError::new(codes::PARSE_ERROR, msg),
            ProtocolError::InvalidRequest(msg) => JsonRpcError::new(codes::INVALID_REQUEST, msg),
            ProtocolError::MethodNotFound(msg) => JsonRpcError::new(codes::METHOD_NOT_FOUND, msg),
            ProtocolError::InvalidParams(msg) => JsonRpcError::new(codes::INVALID_PARAMS, msg),
            ProtocolError::Internal(msg) => JsonRpcError::new(codes::INTERNAL_ERROR, msg),
            ProtocolError::WorkspaceNotFound(msg) => JsonRpcError::new(codes::WORKSPACE_NOT_FOUND, msg),
            ProtocolError::RuleNotSupported(msg) => JsonRpcError::new(codes::RULE_NOT_SUPPORTED, msg),
            ProtocolError::RulePackNotFound(msg) => JsonRpcError::new(codes::RULE_PACK_NOT_FOUND, msg),
            ProtocolError::BaselineNotFound(msg) => JsonRpcError::new(codes::BASELINE_NOT_FOUND, msg),
            ProtocolError::IncrementalStateCorrupt(msg) => JsonRpcError::new(codes::INCREMENTAL_STATE_CORRUPT, msg),
            ProtocolError::RemediationNotSupported(msg) => JsonRpcError::new(codes::REMEDIATION_NOT_SUPPORTED, msg),
            ProtocolError::AuthenticationRequired(msg) => JsonRpcError::new(codes::AUTHENTICATION_REQUIRED, msg),
            ProtocolError::QuotaExceeded(msg) => JsonRpcError::new(codes::QUOTA_EXCEEDED, msg),
            ProtocolError::WithCode { code, message } => JsonRpcError::new(*code, message),
        }
    }
}

/// Result type for protocol operations
pub type ProtocolResult<T> = Result<T, ProtocolError>;

impl From<ProtocolError> for JsonRpcError {
    fn from(err: ProtocolError) -> Self {
        err.to_jsonrpc_error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_error_serialization() {
        let err = JsonRpcError::new(codes::METHOD_NOT_FOUND, "check.run");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("METHOD_NOT_FOUND"));
    }

    #[test]
    fn test_protocol_error_codes() {
        let err = ProtocolError::WorkspaceNotFound("test".into());
        let json_rpc = err.to_jsonrpc_error();
        assert_eq!(json_rpc.code, codes::WORKSPACE_NOT_FOUND);
    }
}