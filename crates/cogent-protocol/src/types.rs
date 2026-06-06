//! Core data types for the Cogent Protocol

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity levels for findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Get numeric score for severity (higher = worse)
    pub fn score(&self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::High => 3,
            Severity::Critical => 4,
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Info
    }
}

/// Finding category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Security,
    Quality,
    Compliance,
    Style,
}

impl Default for Category {
    fn default() -> Self {
        Category::Quality
    }
}

/// Confidence level for suggested fixes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence::Medium
    }
}

/// A single finding (issue) detected during a check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Unique identifier: rule:file:line
    pub finding_id: String,
    /// Rule that produced this finding
    pub rule_id: String,
    /// Rule pack that owns this rule (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_pack: Option<String>,
    /// Severity level
    #[serde(default)]
    pub severity: Severity,
    /// Category of the finding
    #[serde(default)]
    pub category: Category,
    /// Source file path (relative to workspace root)
    pub file: String,
    /// Line number (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    /// Column number (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
    /// End line for multi-line findings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
    /// End column for multi-line findings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u64>,
    /// Human-readable message
    pub message: String,
    /// Code snippet (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_snippet: Option<String>,
    /// Suggested fix (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<SuggestedFix>,
    /// Compliance control mappings (e.g., SOC2 CC7.1)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub compliance_controls: Vec<String>,
    /// Tags for filtering/searching
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Supporting evidence for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A suggested fix for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedFix {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default)]
    pub confidence: Confidence,
    /// Whether this fix can be applied automatically
    #[serde(default)]
    pub auto_applicable: bool,
}

/// Per-file summary of findings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSummary {
    pub file: String,
    pub issue_count: usize,
    pub severity_score: usize,
    pub findings_by_severity: HashMap<String, usize>,
}

/// Result of a single quality check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub findings: Vec<Finding>,
}

/// Aggregate summary across all checks in a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub check_id: String,
    pub total_findings: usize,
    pub by_severity: HashMap<String, usize>,
    pub by_category: HashMap<String, usize>,
    pub by_rule: HashMap<String, RuleSummary>,
    pub rules_run: Vec<String>,
    pub skipped_rules: Vec<String>,
    pub incremental: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_id: Option<String>,
    pub new_findings: usize,
    pub suppressed_findings: usize,
}

/// Per-rule summary in check summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSummary {
    pub findings: usize,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// Complete check run response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub check_id: String,
    pub passed: bool,
    pub summary: CheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_id: Option<String>,
}

/// Rule configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleConfig {
    /// Rule-specific config (arbitrary JSON)
    #[serde(flatten)]
    pub config: HashMap<String, serde_json::Value>,
}

/// Rule pack definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePack {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub rules: Vec<PackRule>,
    /// Control mapping: control_id -> list of rule_ids
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub control_mapping: HashMap<String, Vec<String>>,
}

/// Rule within a pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRule {
    pub rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<RuleConfig>,
}

/// Baseline entry for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub finding_id: String,
    pub status: BaselineStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Baseline status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaselineStatus {
    Open,
    Suppressed,
    Fixed,
    WontFix,
}

/// Complete baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub baseline_id: String,
    pub created_at: String,
    pub entries: Vec<BaselineEntry>,
}

/// Provider capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol_version: String,
    pub rules: Vec<String>,
    pub rule_packs: Vec<String>,
    pub features: Vec<String>,
    pub max_workspace_size_mb: usize,
    pub languages: Vec<String>,
    pub transports: Vec<String>,
    pub auth_methods: Vec<String>,
}

/// Progress event for streaming updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub rule: String,
    pub stage: String,
    pub files_processed: usize,
    pub total_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Check run request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunParams {
    /// Workspace root (file:// URI or absolute path)
    pub workspace: String,
    /// Target paths to check (relative to workspace)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub targets: Vec<String>,
    /// Rules to run (empty = all available)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub rules: Vec<String>,
    /// Rule packs to include
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub rule_packs: Vec<String>,
    /// Per-rule configuration
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub config: HashMap<String, RuleConfig>,
    /// Baseline ID to compare against
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_id: Option<String>,
    /// Enable incremental scanning
    #[serde(default)]
    pub incremental: bool,
    /// Changed files for incremental mode
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub changed_files: Vec<String>,
    /// Output format
    #[serde(default = "default_output_format")]
    pub output_format: OutputFormat,
}

/// Output format for check results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Streaming,
    Batched,
}

fn default_output_format() -> OutputFormat {
    OutputFormat::Streaming
}

/// Check run response (for non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunResponse {
    pub check_id: String,
    pub passed: bool,
    pub summary: CheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finding_serialization() {
        let finding = Finding {
            finding_id: "secrets:src/main.rs:42".into(),
            rule_id: "secrets".into(),
            rule_pack: Some("soc2".into()),
            severity: Severity::Critical,
            category: Category::Security,
            file: "src/main.rs".into(),
            line: Some(42),
            column: Some(15),
            end_line: Some(42),
            end_column: Some(30),
            message: "Hardcoded AWS secret".into(),
            code_snippet: Some("aws_secret = \"AKIA...\"".into()),
            suggested_fix: Some(SuggestedFix {
                description: "Use env var".into(),
                diff: Some("- aws_secret = \"...\"\n+ aws_secret = env::var(\"AWS_SECRET\")".into()),
                confidence: Confidence::High,
                auto_applicable: true,
            }),
            compliance_controls: vec!["CC7.1".into()],
            tags: vec!["aws".into(), "secret".into()],
            metadata: None,
        };
        let json = serde_json::to_string(&finding).unwrap();
        assert!(json.contains("secrets:src/main.rs:42"));
        assert!(json.contains("CC7.1"));
    }

    #[test]
    fn test_severity_scores() {
        assert_eq!(Severity::Info.score(), 0);
        assert_eq!(Severity::Critical.score(), 4);
    }
}