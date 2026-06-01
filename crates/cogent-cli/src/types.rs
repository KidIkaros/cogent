//! Shared result types for cogent-cli.

#![deny(clippy::all)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Finding {
    pub(crate) file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) column: Option<u64>,
    pub(crate) severity: String,
    pub(crate) message: String,
    pub(crate) rule_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) fix_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) evidence: Option<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) suggested_fix: Option<SuggestedFix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) controls: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Evidence {
    pub(crate) snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SuggestedFix {
    pub(crate) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diff: Option<String>,
    pub(crate) confidence: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct FileSummary {
    pub(crate) file: String,
    pub(crate) issue_count: usize,
    pub(crate) severity_score: usize,
    pub(crate) findings_by_severity: std::collections::HashMap<String, usize>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CheckResult {
    pub(crate) name: String,
    pub(crate) passed: bool,
    pub(crate) score: Option<f64>,
    pub(crate) threshold: Option<f64>,
    pub(crate) message: String,
    pub(crate) details: serde_json::Value,
    pub(crate) severity: Option<String>,
    pub(crate) help: Option<String>,
    pub(crate) rule_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) findings: Vec<Finding>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CheckReport {
    pub(crate) passed: bool,
    pub(crate) path: String,
    pub(crate) checks: Vec<CheckResult>,
    pub(crate) summary: CheckSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) file_summary: Vec<FileSummary>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CheckSummary {
    pub(crate) total_checks: usize,
    pub(crate) passed_checks: usize,
    pub(crate) failed_checks: usize,
    pub(crate) functions_analyzed: usize,
    pub(crate) avg_complexity: f64,
    pub(crate) avg_crap: f64,
}

#[derive(Serialize)]
pub(crate) struct ToolInfo {
    pub(crate) name: String,
    pub(crate) binary: String,
    pub(crate) description: String,
    pub(crate) supported_formats: Vec<String>,
    pub(crate) output_fields: Vec<String>,
    pub(crate) rule_ids: Vec<String>,
}

pub(crate) fn extract_findings_from_details(
    details: &serde_json::Value,
    default_rule_id: &str,
    default_severity: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let arrays = ["findings", "violations", "items", "functions", "files", "results", "matches"];
    for key in &arrays {
        if let Some(arr) = details.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                let file = item.get("file").and_then(|v| v.as_str())
                    .or_else(|| item.get("path").and_then(|v| v.as_str()))
                    .or_else(|| item.get("filename").and_then(|v| v.as_str()))
                    .unwrap_or("").to_string();
                let line = item.get("line").and_then(|v| v.as_u64());
                let column = item.get("column").and_then(|v| v.as_u64());
                let severity = item.get("severity").and_then(|v| v.as_str())
                    .unwrap_or(default_severity).to_string();
                let message = item.get("message").and_then(|v| v.as_str())
                    .or_else(|| item.get("description").and_then(|v| v.as_str()))
                    .or_else(|| item.get("name").and_then(|v| v.as_str()))
                    .or_else(|| item.get("type").and_then(|v| v.as_str()))
                    .unwrap_or("").to_string();
                let rule_id = item.get("rule_id").and_then(|v| v.as_str())
                    .or_else(|| item.get("rule").and_then(|v| v.as_str()))
                    .unwrap_or(default_rule_id).to_string();
                let fix_hint = item.get("fix_hint").and_then(|v| v.as_str())
                    .unwrap_or("").to_string();
                if file.is_empty() && message.is_empty() { continue; }
                findings.push(Finding {
                    file, line, column, severity, message, rule_id, fix_hint,
                    evidence: None, suggested_fix: None, controls: None,
                });
            }
        }
    }
    findings
}

pub(crate) fn aggregate_file_summary(checks: &[CheckResult]) -> Vec<FileSummary> {
    let mut map: std::collections::HashMap<String, (usize, usize, std::collections::HashMap<String, usize>)> =
        std::collections::HashMap::new();
    for check in checks {
        for finding in &check.findings {
            let sev_score = match finding.severity.as_str() {
                "critical" => 4,
                "high" | "error" => 3,
                "medium" | "warning" => 2,
                "low" => 1,
                _ => 0,
            };
            let entry = map.entry(finding.file.clone()).or_insert_with(|| (0, 0, std::collections::HashMap::new()));
            entry.0 += 1;
            entry.1 += sev_score;
            *entry.2.entry(finding.severity.clone()).or_insert(0) += 1;
        }
    }
    let mut summaries: Vec<FileSummary> = map.into_iter()
        .map(|(file, (issue_count, severity_score, findings_by_severity))| FileSummary {
            file, issue_count, severity_score, findings_by_severity,
        })
        .collect();
    summaries.sort_by(|a, b| {
        b.severity_score.cmp(&a.severity_score).then_with(|| b.issue_count.cmp(&a.issue_count))
    });
    summaries.truncate(20);
    summaries
}
