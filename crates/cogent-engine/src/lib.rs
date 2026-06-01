//! cogent-engine — audit orchestration: runs tool binaries, parses output,
//! and produces structured `CheckResult`s.

#![deny(clippy::all)]

pub mod checks;
pub mod registry;
pub mod runner;

#[cfg(test)]
mod tests;

pub use runner::{DefaultToolRunner, MockToolRunner, ToolRunner};

use cogent_common::{CheckResult, Finding, FileSummary, ToolResult};
use std::time::Instant;
use tracing::{info, warn};

// ═══════════════════════════════════════════
// RUN TOOL
// ═══════════════════════════════════════════

/// Run a Cogent tool binary (or `cargo run` fallback) and return a `ToolResult`.
///
/// This is the backward-compatible free-function wrapper around
/// [`DefaultToolRunner::run`]. For testability, prefer using the [`ToolRunner`]
/// trait directly so a [`MockToolRunner`] can be substituted.
pub fn run_tool(crate_name: &str, bin_name: &str, args: &[&str], tool_start: Instant) -> ToolResult {
    info!(tool = bin_name, crate = crate_name, "running tool via DefaultToolRunner");
    let runner = DefaultToolRunner;
    match runner.run(crate_name, bin_name, args, tool_start) {
        Ok(result) => result,
        Err(e) => {
            warn!(tool = bin_name, error = %e, "tool execution failed");
            ToolResult {
                tool: bin_name.to_string(),
                success: false,
                duration_ms: tool_start.elapsed().as_millis() as u64,
                data: serde_json::Value::Null,
                error: Some(e.to_string()),
                suggested_fix: None,
                auto_fix_available: None,
            }
        }
    }
}

// ═══════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════

/// Run a tool via any [`ToolRunner`] and return a [`ToolResult`], mirroring the
/// behavior of the legacy [`run_tool`] free function.
///
/// This helper is intended for use in `check_*` functions so they can be tested
/// with [`MockToolRunner`].
pub fn run_tool_with_runner(
    runner: &dyn ToolRunner,
    crate_name: &str,
    bin_name: &str,
    args: &[&str],
    tool_start: Instant,
) -> ToolResult {
    match runner.run(crate_name, bin_name, args, tool_start) {
        Ok(result) => result,
        Err(e) => {
            warn!(tool = bin_name, error = %e, "tool execution failed via runner");
            ToolResult {
                tool: bin_name.to_string(),
                success: false,
                duration_ms: tool_start.elapsed().as_millis() as u64,
                data: serde_json::Value::Null,
                error: Some(e.to_string()),
                suggested_fix: None,
                auto_fix_available: None,
            }
        }
    }
}

/// Build a `CheckResult` for a tool that could not be run (missing binary, etc.).
pub fn skipped_tool_check(
    name: &str,
    rule_id: &str,
    threshold: usize,
    error: Option<String>,
) -> CheckResult {
    info!(tool = name, rule_id, "returning skipped check result");
    CheckResult {
        name: name.into(),
        passed: true,
        score: None,
        threshold: Some(threshold as f64),
        message: match error {
            Some(error) if !error.is_empty() => format!("Skipped: {}", error),
            _ => "Skipped: tool unavailable or produced no JSON output".into(),
        },
        details: serde_json::Value::Null,
        severity: Some("info".into()),
        help: Some(
            "Install the check tool or run from the Cogent workspace to enable this check.".into(),
        ),
        findings: Vec::new(),
        rule_id: Some(rule_id.into()),
    }
}

/// Generic extraction of `Finding` structs from typical tool JSON output.
pub fn extract_findings_from_details(
    details: &serde_json::Value,
    default_rule_id: &str,
    default_severity: &str,
) -> Vec<Finding> {
    info!(default_rule_id, default_severity, "extracting findings from tool output");
    let mut findings = Vec::new();
    let arrays = [
        "findings",
        "violations",
        "items",
        "functions",
        "files",
        "results",
        "matches",
    ];
    for key in &arrays {
        if let Some(arr) = details.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                let file = item
                    .get("file")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("path").and_then(|v| v.as_str()))
                    .or_else(|| item.get("filename").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let line = item.get("line").and_then(|v| v.as_u64());
                let column = item.get("column").and_then(|v| v.as_u64());
                let severity = item
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_severity)
                    .to_string();
                let message = item
                    .get("message")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("description").and_then(|v| v.as_str()))
                    .or_else(|| item.get("name").and_then(|v| v.as_str()))
                    .or_else(|| item.get("type").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let rule_id = item
                    .get("rule_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("rule").and_then(|v| v.as_str()))
                    .unwrap_or(default_rule_id)
                    .to_string();
                let fix_hint = item
                    .get("fix_hint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // Skip empty entries that are just summary metadata
                if file.is_empty() && message.is_empty() {
                    continue;
                }
                findings.push(Finding {
                    file,
                    line,
                    column,
                    severity,
                    message,
                    rule_id,
                    fix_hint,
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                });
            }
        }
    }
    findings
}

/// Compute per-file aggregation from all check results.
pub fn aggregate_file_summary(checks: &[CheckResult]) -> Vec<FileSummary> {
    info!(check_count = checks.len(), "aggregating file summaries");
    let mut map: std::collections::HashMap<
        String,
        (usize, usize, std::collections::HashMap<String, usize>),
    > = std::collections::HashMap::new();
    for check in checks {
        for finding in &check.findings {
            let sev_score = match finding.severity.as_str() {
                "critical" => 4,
                "high" | "error" => 3,
                "medium" | "warning" => 2,
                "low" => 1,
                _ => 0,
            };
            let entry = map
                .entry(finding.file.clone())
                .or_insert_with(|| (0, 0, std::collections::HashMap::new()));
            entry.0 += 1;
            entry.1 += sev_score;
            *entry.2.entry(finding.severity.clone()).or_insert(0) += 1;
        }
    }
    let mut summaries: Vec<FileSummary> = map
        .into_iter()
        .map(
            |(file, (issue_count, severity_score, findings_by_severity))| FileSummary {
                file,
                issue_count,
                severity_score,
                findings_by_severity,
            },
        )
        .collect();
    summaries.sort_by_key(|s| std::cmp::Reverse(s.severity_score));
    summaries
}
