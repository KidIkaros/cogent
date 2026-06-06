//! Core data types for Cogent — check results, findings, evidence, SARIF output.
//!
//! This module contains **only** data definitions (structs, enums, derives).
//! No logic, no formatting, no I/O.  All construction helpers and formatters
//! live in other crates (`cogent-engine` for check logic, `cogent-report` for
//! formatting).

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════
// HEADLESS API TYPES
// ═══════════════════════════════════════════

/// Request to run a quality tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolRequest {
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// Response from a quality tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub tool: String,
    pub version: String,
    pub success: bool,
    pub duration_ms: u64,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Suggested fix for the issues found (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    /// Whether an auto-fix is available for the issues found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_fix_available: Option<bool>,
}

/// Result from one tool run within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: String,
    pub success: bool,
    pub duration_ms: u64,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Suggested fix for the issues found (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    /// Whether an auto-fix is available for the issues found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_fix_available: Option<bool>,
}

/// Progress event streamed during long-running tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub tool: String,
    pub stage: String,
    pub progress_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ═══════════════════════════════════════════
// CHECK RESULT TYPES
// ═══════════════════════════════════════════

/// A single finding (file + line + severity + message).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Finding {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
    pub severity: String,
    pub message: String,
    pub rule_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fix_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<SuggestedFix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controls: Option<Vec<String>>,
}

/// Supporting evidence for a finding (snippet, hash, context).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Evidence {
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A suggested fix for a finding.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SuggestedFix {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub confidence: String,
}

/// Per-file summary of findings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileSummary {
    pub file: String,
    pub issue_count: usize,
    pub severity_score: usize,
    pub findings_by_severity: std::collections::HashMap<String, usize>,
}

/// Result of a single quality check.
#[derive(Serialize, Deserialize, Clone)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub score: Option<f64>,
    pub threshold: Option<f64>,
    pub message: String,
    pub details: serde_json::Value,
    pub severity: Option<String>,
    pub help: Option<String>,
    pub rule_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub findings: Vec<Finding>,
}

/// Full report for a path (all checks + summary).
#[derive(Serialize, Deserialize)]
pub struct CheckReport {
    pub passed: bool,
    pub path: String,
    pub checks: Vec<CheckResult>,
    pub summary: CheckSummary,
    /// Weighted health score 0–100 (security ×3, compliance ×2, quality ×1).
    pub health_score: u32,
    /// Letter grade: A (90+), B (80–89), C (65–79), D (50–64), F (<50).
    pub grade: String,
    /// Full audit opinion with gate killers, category scores, and margin risks.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub audit: Option<crate::AuditResult>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub file_summary: Vec<FileSummary>,
}

/// Aggregate summary across all checks.
#[derive(Serialize, Deserialize)]
pub struct CheckSummary {
    pub total_checks: usize,
    pub passed_checks: usize,
    pub failed_checks: usize,
    pub functions_analyzed: usize,
    pub avg_complexity: f64,
    pub avg_crap: f64,
}
