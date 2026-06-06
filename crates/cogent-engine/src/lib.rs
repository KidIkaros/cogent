//! cogent-engine — audit orchestration: runs tool binaries, parses output,
//! and produces structured `CheckResult`s.

#![deny(clippy::all)]

pub mod checks;
pub mod bridge;
pub mod registry;
pub mod runner;

#[cfg(test)]
mod tests;

pub use runner::{DefaultToolRunner, MockToolRunner, ToolRunner};
pub use registry::{AuditTool, ToolRegistry};

use cogent_common::{CheckResult, Finding, FileSummary, ToolResult};
pub use registry::registry;
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

// ═══════════════════════════════════════════
// CHECK THRESHOLDS — data-driven dispatch
// ═══════════════════════════════════════════

/// Thresholds for all delegated tool checks.
/// Loaded from `.quality.toml` and passed to [`ToolRegistry::run_check`]
/// for data-driven dispatch.
#[derive(Debug, Clone)]
pub struct CheckThresholds {
    pub max_crap: f64,
    pub min_doc: f64,
    pub max_debt: usize,
    pub max_complexity_violations: usize,
    pub min_complexity: u32,
    pub max_taint: usize,
    pub max_duplication: f64,
    pub max_risk: f64,
    pub max_coupling: usize,
    pub min_propcov: f64,
    pub max_fuzz_risk: usize,
    pub max_linelen: usize,
    pub max_halstead_bugs: f64,
    pub max_secrets: usize,
    pub max_deadcode: usize,
    pub max_cohesion: usize,
    pub min_comment_ratio: f64,
    pub max_errhandle: usize,
    pub min_typecov: f64,
    pub max_vuln_critical: usize,
    pub max_vuln_high: usize,
    pub max_sast: usize,
    pub max_crypto: usize,
    pub max_license_violations: usize,
    pub max_outdated: usize,
    pub max_access_control: usize,
    pub max_supply_chain: usize,
    /// Optional coverage path for CRAP metric calculation.
    pub coverage_path: Option<String>,
    pub max_observability: usize,
    pub max_test_quality: usize,
    pub max_debuggability: usize,
    /// Path substrings to exclude from the secrets scanner.
    pub secrets_exclude_paths: Vec<String>,
    /// Path substrings to exclude from the access-control scanner.
    pub access_control_exclude_paths: Vec<String>,
}

impl CheckThresholds {
    /// Load thresholds from a `.quality.toml` config file.
    ///
    /// Uses line-by-line TOML parsing (intentionally avoids a TOML dependency).
    /// Falls back to `Self::default()` values for any key not present in the file.
    pub fn load_from_config(config_path: &str) -> Self {
        let mut t = Self::default();
        let Ok(content) = std::fs::read_to_string(config_path) else {
            return t;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            macro_rules! parse_f64 {
                ($key:expr, $field:ident) => {
                    if let Some(val) = parse_config_f64(line, $key) {
                        t.$field = val;
                    }
                };
            }
            macro_rules! parse_usize {
                ($key:expr, $field:ident) => {
                    if let Some(val) = parse_config_usize(line, $key) {
                        t.$field = val;
                    }
                };
            }
            parse_f64!("max_avg", max_crap);
            parse_f64!("min_pct", min_doc);
            parse_usize!("max_markers", max_debt);
            parse_usize!("max_violations", max_complexity_violations);
            parse_f64!("max_duplicates", max_duplication);
            parse_usize!("max_taint", max_taint);
            parse_f64!("max_risk", max_risk);
            parse_usize!("max_coupling", max_coupling);
            parse_f64!("min_propcov", min_propcov);
            parse_usize!("max_fuzz_risk", max_fuzz_risk);
            parse_usize!("max_linelen", max_linelen);
            parse_f64!("max_halstead_bugs", max_halstead_bugs);
            parse_usize!("max_secrets", max_secrets);
            parse_usize!("max_deadcode", max_deadcode);
            parse_usize!("max_cohesion", max_cohesion);
            parse_f64!("min_comment_ratio", min_comment_ratio);
            parse_usize!("max_errhandle", max_errhandle);
            parse_f64!("min_typecov", min_typecov);
            parse_usize!("max_vuln_critical", max_vuln_critical);
            parse_usize!("max_vuln_high", max_vuln_high);
            parse_usize!("max_sast", max_sast);
            parse_usize!("max_crypto", max_crypto);
            parse_usize!("max_license_violations", max_license_violations);
            parse_usize!("max_outdated", max_outdated);
            parse_usize!("max_access_control", max_access_control);
            parse_usize!("max_supply_chain", max_supply_chain);
            parse_usize!("max_observability", max_observability);
            parse_usize!("max_test_quality", max_test_quality);
            parse_usize!("max_debuggability", max_debuggability);
            // secrets_exclude_paths is a comma-separated list
            if let Some(val) = cogent_common::parse_string_list(line, "secrets_exclude") {
                t.secrets_exclude_paths = val;
            }
            // access_control_exclude_paths is a comma-separated list
            if let Some(val) = cogent_common::parse_string_list(line, "access_control_exclude") {
                t.access_control_exclude_paths = val;
            }
        }
        t
    }
}

/// Parse a `key = value` line for f64 values.
fn parse_config_f64(line: &str, key: &str) -> Option<f64> {
    let prefix = format!("{} =", key);
    let prefix2 = format!("{}=", key);
    let rest = line
        .strip_prefix(&prefix)
        .or_else(|| line.strip_prefix(&prefix2))?;
    rest.split_whitespace().next()?.parse().ok()
}

/// Parse a `key = value` line for usize values.
fn parse_config_usize(line: &str, key: &str) -> Option<usize> {
    parse_config_f64(line, key).map(|v| v as usize)
}

/// Parse a `key = value` line for u32 values.
fn parse_config_u32(line: &str, key: &str) -> Option<u32> {
    parse_config_f64(line, key).map(|v| v as u32)
}



impl Default for CheckThresholds {
    fn default() -> Self {
        Self {
            max_crap: 30.0,
            min_doc: 50.0,
            max_debt: 1000,
            max_complexity_violations: 0,
            min_complexity: 10,
            max_taint: 0,
            max_duplication: 100.0,
            max_risk: 100.0,
            max_coupling: usize::MAX,
            min_propcov: 0.0,
            max_fuzz_risk: usize::MAX,
            max_linelen: usize::MAX,
            max_halstead_bugs: 100.0,
            max_secrets: 0,
            max_deadcode: usize::MAX,
            max_cohesion: usize::MAX,
            min_comment_ratio: 0.0,
            max_errhandle: usize::MAX,
            min_typecov: 0.0,
            max_vuln_critical: 0,
            max_vuln_high: 0,
            max_sast: 0,
            max_crypto: 0,
            max_license_violations: 0,
            max_outdated: usize::MAX,
            max_access_control: 0,
            max_supply_chain: 0,
            coverage_path: None,
            max_observability: 1000,
            max_test_quality: 60,
            max_debuggability: 1000,
            secrets_exclude_paths: Vec::new(),
            access_control_exclude_paths: Vec::new(),
        }
    }
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
