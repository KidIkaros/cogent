//! JSON-RPC method definitions for the Cogent Protocol

use crate::types::*;
use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC protocol version (always "2.0").
    pub jsonrpc: String,
    /// Request id (absent for notifications).
    pub id: Option<serde_json::Value>,
    /// Method name being invoked.
    pub method: String,
    /// Method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC protocol version (always "2.0").
    pub jsonrpc: String,
    /// Id of the request this response corresponds to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// Successful result payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error object when the request failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<super::error::JsonRpcError>,
}

/// JSON-RPC 2.0 Notification (no id, no response expected)
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    /// JSON-RPC protocol version (always "2.0").
    pub jsonrpc: String,
    /// Notification method name.
    pub method: String,
    /// Notification parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Method names as constants
pub mod method_names {
    /// `initialize` — handshake request.
    pub const INITIALIZE: &str = "initialize";
    /// `initialized` — handshake acknowledgement.
    pub const INITIALIZED: &str = "initialized";
    /// `check.run` — run a check.
    pub const CHECK_RUN: &str = "check.run";
    /// `findings.stream` — stream findings for a check.
    pub const FINDINGS_STREAM: &str = "findings.stream";
    /// `baseline.get` — fetch a baseline.
    pub const BASELINE_GET: &str = "baseline.get";
    /// `baseline.set` — store a baseline.
    pub const BASELINE_SET: &str = "baseline.set";
    /// `baseline.diff` — diff findings against a baseline.
    pub const BASELINE_DIFF: &str = "baseline.diff";
    /// `rule.pack.install` — install a rule pack.
    pub const RULE_PACK_INSTALL: &str = "rule.pack.install";
    /// `rule.pack.list` — list installed rule packs.
    pub const RULE_PACK_LIST: &str = "rule.pack.list";
    /// `rule.pack.remove` — remove a rule pack.
    pub const RULE_PACK_REMOVE: &str = "rule.pack.remove";
    /// `rule.pack.resolve` — resolve a rule pack to its rules.
    pub const RULE_PACK_RESOLVE: &str = "rule.pack.resolve";
    /// `remediation.apply` — apply a remediation.
    pub const REMEDIATION_APPLY: &str = "remediation.apply";
    /// `capabilities` — query provider capabilities.
    pub const CAPABILITIES: &str = "capabilities";
    /// `check.progress` — progress notification.
    pub const PROGRESS: &str = "check.progress";
    /// `findings.finding` — single finding notification.
    pub const FINDINGS_FINDING: &str = "findings.finding";
    /// `findings.end` — end-of-findings notification.
    pub const FINDINGS_END: &str = "findings.end";
    /// `check.rule_complete` — rule completion notification.
    pub const CHECK_RULE_COMPLETE: &str = "check.rule_complete";
}

/// Initialize request (client -> server)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version requested by the client.
    pub protocol_version: String,
    /// Information about the client.
    pub client_info: ClientInfo,
    /// Optional client capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ClientCapabilities>,
}

/// Client info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client application name.
    pub name: String,
    /// Client application version.
    pub version: String,
}

/// Client capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientCapabilities {
    /// Whether the client supports streaming responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Whether the client supports incremental scans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<bool>,
}

/// Initialize response (server -> client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Information about the server.
    pub server_info: ServerInfo,
    /// Capabilities advertised by the server.
    pub capabilities: Capabilities,
}

/// Server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server application name.
    pub name: String,
    /// Server application version.
    pub version: String,
}

/// Findings stream params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingsStreamParams {
    /// Identifier of the check to stream findings for.
    pub check_id: String,
    /// Optional filters applied to the stream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<StreamFilters>,
}

/// Stream filters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamFilters {
    /// Only include findings with these severities.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub severity: Vec<Severity>,
    /// Only include findings from these rules.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub rules: Vec<String>,
}

/// Baseline get params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineGetParams {
    /// Identifier of the baseline to fetch.
    pub baseline_id: String,
}

/// Baseline set params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSetParams {
    /// Identifier of the baseline to store.
    pub baseline_id: String,
    /// Entries that make up the baseline.
    pub entries: Vec<BaselineEntry>,
}

/// Baseline diff params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiffParams {
    /// Identifier of the baseline to diff against.
    pub baseline_id: String,
    /// Current findings to compare with the baseline.
    pub current_findings: Vec<Finding>,
}

/// Baseline diff result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiffResult {
    /// Findings not present in the baseline.
    pub new_findings: Vec<Finding>,
    /// Ids of baseline findings that are now resolved.
    pub resolved_findings: Vec<String>, // finding_ids
    /// Ids of findings present in both baseline and current run.
    pub unchanged_findings: Vec<String>, // finding_ids
}

/// Rule pack install params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackInstallParams {
    /// Identifier of the rule pack to install.
    pub pack: String,
    /// Specific version to install, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Source to install the pack from, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Rule pack list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackListResponse {
    /// Installed rule packs.
    pub packs: Vec<RulePackInfo>,
}

/// Rule pack info (lightweight)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackInfo {
    /// Unique identifier of the rule pack.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Pack version string.
    pub version: String,
    /// Description of what the pack covers.
    pub description: String,
}

/// Rule pack resolve params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackResolveParams {
    /// Identifier of the rule pack to resolve.
    pub pack: String,
}

/// Rule pack resolve response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackResolveResponse {
    /// Rules contained in the resolved pack.
    pub rules: Vec<PackRule>,
}

/// Remediation apply params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationApplyParams {
    /// Identifier of the finding to remediate.
    pub finding_id: String,
    /// Remediation action to apply.
    pub action: String,
}

/// Remediation apply response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationApplyResponse {
    /// Whether the remediation was applied successfully.
    pub success: bool,
    /// Diff describing the applied change, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Error message when the remediation failed.
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
    /// Identifier of the check in progress.
    pub check_id: String,
    /// Rule currently being executed.
    pub rule: String,
    /// Current processing stage.
    pub stage: String,
    /// Number of files processed so far.
    pub files_processed: usize,
    /// Total number of files to process.
    pub total_files: usize,
    /// Optional human-readable progress message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Rule complete notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCompleteParams {
    /// Identifier of the check the rule belongs to.
    pub check_id: String,
    /// Rule that completed.
    pub rule: String,
    /// Whether the rule passed its threshold.
    pub passed: bool,
    /// Computed score for the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Threshold the score was compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// Wall-clock duration of the rule in milliseconds.
    pub duration_ms: u64,
}

/// Finding notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingNotificationParams {
    /// Identifier of the check the finding belongs to.
    pub check_id: String,
    /// The reported finding.
    pub finding: Finding,
}

/// Findings end notification params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingsEndParams {
    /// Identifier of the check that finished streaming.
    pub check_id: String,
    /// Total number of findings streamed.
    pub total_findings: usize,
}

/// Batch request (multiple requests in one HTTP call)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest(
    /// The requests included in the batch.
    pub Vec<JsonRpcRequest>,
);

/// Batch response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse(
    /// The responses included in the batch.
    pub Vec<JsonRpcResponse>,
);

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
