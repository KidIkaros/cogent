//! JSON-RPC method definitions for the Cogent Protocol

use crate::types::*;
use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<super::error::JsonRpcError>,
}

/// JSON-RPC 2.0 Notification (no id, no response expected)
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Method names as constants
pub mod method_names {
    pub const INITIALIZE: &str = "initialize";
    pub const INITIALIZED: &str = "initialized";
    pub const CHECK_RUN: &str = "check.run";
    pub const FINDINGS_STREAM: &str = "findings.stream";
    pub const BASELINE_GET: &str = "baseline.get";
    pub const BASELINE_SET: &str = "baseline.set";
    pub const BASELINE_DIFF: &str = "baseline.diff";
    pub const RULE_PACK_INSTALL: &str = "rule.pack.install";
    pub const RULE_PACK_LIST: &str = "rule.pack.list";
    pub const RULE_PACK_REMOVE: &str = "rule.pack.remove";
    pub const RULE_PACK_RESOLVE: &str = "rule.pack.resolve";
    pub const REMEDIATION_APPLY: &str = "remediation.apply";
    pub const CAPABILITIES: &str = "capabilities";
    pub const PROGRESS: &str = "check.progress";
    pub const FINDINGS_FINDING: &str = "findings.finding";
    pub const FINDINGS_END: &str = "findings.end";
    pub const CHECK_RULE_COMPLETE: &str = "check.rule_complete";
}

/// Initialize request (client -> server)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub client_info: ClientInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
}

/// Client info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Client capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<bool>,
}

/// Initialize response (server -> client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub server_info: ServerInfo,
    pub capabilities: Capabilities,
}

/// Server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Check run request params
pub type CheckRunParams = crate::types::CheckRunParams;

/// Check run response (for batched mode)
pub type CheckRunResponse = crate::types::CheckRunResponse;

/// Findings stream params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingsStreamParams {
    pub check_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<StreamFilters>,
}

/// Stream filters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamFilters {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub severity: Vec<Severity>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub rules: Vec<String>,
}

/// Baseline get params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineGetParams {
    pub baseline_id: String,
}

/// Baseline set params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSetParams {
    pub baseline_id: String,
    pub entries: Vec<BaselineEntry>,
}

/// Baseline diff params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiffParams {
    pub baseline_id: String,
    pub current_findings: Vec<Finding>,
}

/// Baseline diff result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiffResult {
    pub new_findings: Vec<Finding>,
    pub resolved_findings: Vec<String>, // finding_ids
    pub unchanged_findings: Vec<String>, // finding_ids
}

/// Rule pack install params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackInstallParams {
    pub pack: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Rule pack list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackListResponse {
    pub packs: Vec<RulePackInfo>,
}

/// Rule pack info (lightweight)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
}

/// Rule pack resolve params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackResolveParams {
    pub pack: String,
}

/// Rule pack resolve response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackResolveResponse {
    pub rules: Vec<PackRule>,
}

/// Remediation apply params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationApplyParams {
    pub finding_id: String,
    pub action: String,
}

/// Remediation apply response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationApplyResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Capabilities request (no params)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesParams {}

/// Capabilities response
pub type CapabilitiesResponse = Capabilities;

/// Progress notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressParams {
    pub check_id: String,
    pub rule: String,
    pub stage: String,
    pub files_processed: usize,
    pub total_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Rule complete notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCompleteParams {
    pub check_id: String,
    pub rule: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    pub duration_ms: u64,
}

/// Finding notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingNotificationParams {
    pub check_id: String,
    pub finding: Finding,
}

/// Findings end notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingsEndParams {
    pub check_id: String,
    pub total_findings: usize,
}

/// Batch request (multiple requests in one HTTP call)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest(pub Vec<JsonRpcRequest>);

/// Batch response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse(pub Vec<JsonRpcResponse>);

impl JsonRpcRequest {
    /// Create a new request
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::Value::String(id)),
            method: method.into(),
            params,
        }
    }

    /// Create a notification (no id)
    pub fn notification(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: method.into(),
            params,
        }
    }
}

impl JsonRpcResponse {
    /// Create a success response
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: Option<serde_json::Value>, error: super::error::JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = JsonRpcRequest::new("check.run", Some(serde_json::json!({"workspace": "/tmp"})));
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("check.run"));
        assert!(json.contains("2.0"));
    }

    #[test]
    fn test_response_serialization() {
        let resp = JsonRpcResponse::success(
            serde_json::Value::String("123".into()),
            serde_json::json!({"status": "ok"}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("result"));
        assert!(json.contains("status"));
    }

    #[test]
    fn test_notification_serialization() {
        let notif = JsonRpcRequest::notification("findings.finding", Some(serde_json::json!({})));
        assert!(notif.id.is_none());
    }
}