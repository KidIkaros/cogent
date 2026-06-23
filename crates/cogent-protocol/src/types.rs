//! Core data types for the Cogent Protocol

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity levels for findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — no action required.
    #[default]
    Info,
    /// Low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical severity — must be addressed.
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

/// Finding category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Security-related finding.
    Security,
    /// Code quality finding.
    #[default]
    Quality,
    /// Compliance-related finding.
    Compliance,
    /// Stylistic finding.
    Style,
}

/// Confidence level for suggested fixes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Low confidence — review carefully before applying.
    Low,
    /// Medium confidence.
    #[default]
    Medium,
    /// High confidence — safe to apply automatically.
    High,
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
    /// Code snippet that demonstrates the finding.
    pub snippet: String,
    /// Hash of the source file the snippet came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    /// Surrounding context for the snippet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A suggested fix for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedFix {
    /// Human-readable description of the fix.
    pub description: String,
    /// Unified diff that applies the fix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Confidence in the correctness of the fix.
    #[serde(default)]
    pub confidence: Confidence,
    /// Whether this fix can be applied automatically
    #[serde(default)]
    pub auto_applicable: bool,
}

/// Per-file summary of findings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSummary {
    /// Source file path (relative to workspace root).
    pub file: String,
    /// Number of findings in this file.
    pub issue_count: usize,
    /// Aggregate severity score for this file.
    pub severity_score: usize,
    /// Count of findings keyed by severity name.
    pub findings_by_severity: HashMap<String, usize>,
}

/// Result of a single quality check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Name of the check.
    pub name: String,
    /// Whether the check passed its threshold.
    pub passed: bool,
    /// Computed score for the check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Threshold the score was compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// Human-readable summary message.
    pub message: String,
    /// Tool-specific structured details.
    #[serde(default)]
    pub details: serde_json::Value,
    /// Overall severity of the check result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// Guidance on how to fix failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Identifier of the rule that produced this result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Individual findings produced by the check.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub findings: Vec<Finding>,
}

/// Aggregate summary across all checks in a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    /// Unique identifier for this check run.
    pub check_id: String,
    /// Total number of findings across all rules.
    pub total_findings: usize,
    /// Finding counts keyed by severity name.
    pub by_severity: HashMap<String, usize>,
    /// Finding counts keyed by category name.
    pub by_category: HashMap<String, usize>,
    /// Per-rule summaries keyed by rule id.
    pub by_rule: HashMap<String, RuleSummary>,
    /// Rules that were executed in this run.
    pub rules_run: Vec<String>,
    /// Rules that were skipped in this run.
    pub skipped_rules: Vec<String>,
    /// Whether the run used incremental scanning.
    pub incremental: bool,
    /// Baseline this run was compared against, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_id: Option<String>,
    /// Number of findings new relative to the baseline.
    pub new_findings: usize,
    /// Number of findings suppressed by the baseline.
    pub suppressed_findings: usize,
}

/// Per-rule summary in check summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSummary {
    /// Number of findings produced by the rule.
    pub findings: usize,
    /// Whether the rule passed its threshold.
    pub passed: bool,
    /// Computed score for the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Threshold the score was compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// Complete check run response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    /// Unique identifier for this check run.
    pub check_id: String,
    /// Whether the overall run passed.
    pub passed: bool,
    /// Aggregate summary of the run.
    pub summary: CheckSummary,
    /// Baseline this run was compared against, if any.
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
    /// Unique identifier of the rule pack.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Pack version string.
    pub version: String,
    /// Description of what the pack covers.
    pub description: String,
    /// Rules contained in the pack.
    pub rules: Vec<PackRule>,
    /// Control mapping: control_id -> list of rule_ids
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub control_mapping: HashMap<String, Vec<String>>,
}

/// Rule within a pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRule {
    /// Identifier of the rule.
    pub rule_id: String,
    /// Optional rule-specific configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<RuleConfig>,
}

/// Baseline entry for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Identifier of the finding this entry tracks.
    pub finding_id: String,
    /// Current status of the finding.
    pub status: BaselineStatus,
    /// Who suppressed the finding, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_by: Option<String>,
    /// When the finding was suppressed, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_at: Option<String>,
    /// Reason for the current status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Baseline status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaselineStatus {
    /// Finding is open and unresolved.
    Open,
    /// Finding has been suppressed.
    Suppressed,
    /// Finding has been fixed.
    Fixed,
    /// Finding is acknowledged but will not be fixed.
    WontFix,
}

/// Complete baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Unique identifier of the baseline.
    pub baseline_id: String,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Entries tracked by the baseline.
    pub entries: Vec<BaselineEntry>,
}

/// Provider capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Protocol version supported by the provider.
    pub protocol_version: String,
    /// Rule ids the provider supports.
    pub rules: Vec<String>,
    /// Rule pack ids the provider supports.
    pub rule_packs: Vec<String>,
    /// Optional feature flags supported by the provider.
    pub features: Vec<String>,
    /// Maximum workspace size (in megabytes) the provider accepts.
    pub max_workspace_size_mb: usize,
    /// Languages the provider can analyze.
    pub languages: Vec<String>,
    /// Transports the provider supports.
    pub transports: Vec<String>,
    /// Authentication methods the provider supports.
    pub auth_methods: Vec<String>,
}

/// Progress event for streaming updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
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
    /// Stream findings as they are produced.
    Streaming,
    /// Return all findings in a single batched response.
    Batched,
}

fn default_output_format() -> OutputFormat {
    OutputFormat::Streaming
}

/// Check run response (for non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunResponse {
    /// Unique identifier for this check run.
    pub check_id: String,
    /// Whether the overall run passed.
    pub passed: bool,
    /// Aggregate summary of the run.
    pub summary: CheckSummary,
    /// Baseline this run was compared against, if any.
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
                diff: Some(
                    "- aws_secret = \"...\"\n+ aws_secret = env::var(\"AWS_SECRET\")".into(),
                ),
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
