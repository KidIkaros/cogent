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

#[cfg(test)]
mod tests {
    use super::{extract_findings_from_details, aggregate_file_summary, CheckResult, Finding};
    use serde_json::json;

    fn make_finding(file: &str, severity: &str) -> Finding {
        Finding {
            file: file.to_string(),
            line: None,
            column: None,
            severity: severity.to_string(),
            message: "test finding".to_string(),
            rule_id: "test-rule".to_string(),
            fix_hint: String::new(),
            evidence: None,
            suggested_fix: None,
            controls: None,
        }
    }

    fn make_check(findings: Vec<Finding>) -> CheckResult {
        CheckResult {
            name: "test-check".to_string(),
            passed: true,
            score: None,
            threshold: None,
            message: "ok".to_string(),
            details: json!({}),
            severity: None,
            help: None,
            rule_id: None,
            findings,
        }
    }

    // ── Empty / edge cases ─────────────────────────────────────────

    #[test]
    fn test_extract_empty_details() {
        let findings = extract_findings_from_details(&json!({}), "rule-x", "medium");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_extract_no_matching_array() {
        let findings =
            extract_findings_from_details(&json!({"other": []}), "rule-x", "medium");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_extract_empty_array() {
        let findings =
            extract_findings_from_details(&json!({"findings": []}), "rule-x", "medium");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_extract_skip_empty_file_and_message() {
        // Both file and message empty → skip
        let details = json!({"findings": [{"severity": "high"}]});
        let findings = extract_findings_from_details(&details, "rule-x", "medium");
        assert!(
            findings.is_empty(),
            "item with empty file+message should be skipped"
        );
    }

    // ── Per-array type (7 array keys) ──────────────────────────────

    #[test]
    fn test_extract_findings_array() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "err", "severity": "high"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "a.rs");
        assert_eq!(findings[0].message, "err");
    }

    #[test]
    fn test_extract_violations_array() {
        let details = json!({
            "violations": [{"file": "b.rs", "message": "violation", "severity": "critical"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "b.rs");
        assert_eq!(findings[0].severity, "critical");
    }

    #[test]
    fn test_extract_items_array() {
        let details = json!({
            "items": [{"file": "c.rs", "description": "item desc", "severity": "low"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        // `description` fallback for message
        assert_eq!(findings[0].message, "item desc");
    }

    #[test]
    fn test_extract_functions_array() {
        let details = json!({
            "functions": [{"file": "d.rs", "name": "do_stuff", "severity": "medium"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        // `name` fallback for message
        assert_eq!(findings[0].message, "do_stuff");
    }

    #[test]
    fn test_extract_files_array() {
        let details = json!({
            "files": [{"file": "e.rs", "message": "file issue", "severity": "high"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "e.rs");
    }

    #[test]
    fn test_extract_results_array() {
        let details = json!({
            "results": [{"path": "f.rs", "message": "result msg", "severity": "low"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        // `path` fallback for file
        assert_eq!(findings[0].file, "f.rs");
    }

    #[test]
    fn test_extract_matches_array() {
        let details = json!({
            "matches": [{"filename": "g.rs", "type": "match found", "severity": "info"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "g.rs");   // `filename` fallback for file
        assert_eq!(findings[0].message, "match found"); // `type` fallback for message
    }

    // ── File field fallbacks ───────────────────────────────────────

    #[test]
    fn test_extract_file_field_precedence() {
        // file > path > filename
        let details = json!({
            "findings": [{
                "file": "a.rs", "path": "b.rs", "filename": "c.rs", "message": "x"
            }]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].file, "a.rs");
    }

    #[test]
    fn test_extract_file_path_fallback() {
        let details = json!({
            "findings": [{"path": "b.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].file, "b.rs");
    }

    #[test]
    fn test_extract_file_filename_fallback() {
        let details = json!({
            "findings": [{"filename": "c.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].file, "c.rs");
    }

    #[test]
    fn test_extract_file_missing_defaults_empty() {
        let details = json!({
            "findings": [{"message": "only msg", "severity": "high"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].file, "");
    }

    // ── Message field fallbacks ────────────────────────────────────

    #[test]
    fn test_extract_message_precedence() {
        let details = json!({
            "findings": [{
                "file": "a.rs", "message": "m1",
                "description": "d1", "name": "n1", "type": "t1"
            }]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].message, "m1");
    }

    #[test]
    fn test_extract_message_description_fallback() {
        let details = json!({
            "findings": [{"file": "a.rs", "description": "fallback desc"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].message, "fallback desc");
    }

    #[test]
    fn test_extract_message_name_fallback() {
        let details = json!({
            "findings": [{"file": "a.rs", "name": "fn_name"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].message, "fn_name");
    }

    #[test]
    fn test_extract_message_type_fallback() {
        let details = json!({
            "findings": [{"file": "a.rs", "type": "bug_type"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].message, "bug_type");
    }

    #[test]
    fn test_extract_message_missing_defaults_empty() {
        let details = json!({
            "findings": [{"file": "a.rs"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].message, "");
    }

    // ── Rule ID field fallbacks ────────────────────────────────────

    #[test]
    fn test_extract_rule_id_present() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x", "rule_id": "MY-001"}]
        });
        let findings = extract_findings_from_details(&details, "fallback-rule", "medium");
        assert_eq!(findings[0].rule_id, "MY-001");
    }

    #[test]
    fn test_extract_rule_id_rule_fallback() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x", "rule": "ALT-002"}]
        });
        let findings = extract_findings_from_details(&details, "fallback-rule", "medium");
        assert_eq!(findings[0].rule_id, "ALT-002");
    }

    #[test]
    fn test_extract_rule_id_default() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "default-rule", "medium");
        assert_eq!(findings[0].rule_id, "default-rule");
    }

    // ── Severity ───────────────────────────────────────────────────

    #[test]
    fn test_extract_severity_present() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x", "severity": "critical"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].severity, "critical");
    }

    #[test]
    fn test_extract_severity_default() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "high");
        assert_eq!(findings[0].severity, "high");
    }

    // ── Line and column ────────────────────────────────────────────

    #[test]
    fn test_extract_line_column() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x", "line": 42, "column": 7}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].line, Some(42));
        assert_eq!(findings[0].column, Some(7));
    }

    #[test]
    fn test_extract_line_column_missing() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].line, None);
        assert_eq!(findings[0].column, None);
    }

    // ── Fix hint ───────────────────────────────────────────────────

    #[test]
    fn test_extract_fix_hint() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x", "fix_hint": "add docs"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].fix_hint, "add docs");
    }

    #[test]
    fn test_extract_fix_hint_missing() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "x"}]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings[0].fix_hint, "");
    }

    // ── Multiple items / multiple arrays ───────────────────────────

    #[test]
    fn test_extract_multiple_items() {
        let details = json!({
            "findings": [
                {"file": "a.rs", "message": "err1"},
                {"file": "b.rs", "message": "err2"},
            ]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].file, "a.rs");
        assert_eq!(findings[1].file, "b.rs");
    }

    #[test]
    fn test_extract_multiple_arrays_accumulate() {
        let details = json!({
            "findings": [{"file": "a.rs", "message": "finding"}],
            "items": [{"file": "b.rs", "message": "item"}],
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].file, "a.rs");
        assert_eq!(findings[1].file, "b.rs");
    }

    #[test]
    fn test_extract_skip_blank_item() {
        // file="" and message="" → skip
        let details = json!({
            "findings": [
                {"file": "", "message": ""},
                {"file": "valid.rs", "message": "real issue"},
            ]
        });
        let findings = extract_findings_from_details(&details, "r1", "medium");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "valid.rs");
    }

    // ═══════════════════════════════════════════════════════════════
    // aggregate_file_summary
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_aggregate_empty_checks() {
        let result = aggregate_file_summary(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_aggregate_no_findings() {
        let checks = vec![make_check(vec![])];
        let result = aggregate_file_summary(&checks);
        assert!(result.is_empty());
    }

    #[test]
    fn test_aggregate_single_file_single_finding() {
        let checks = vec![make_check(vec![make_finding("main.rs", "high")])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file, "main.rs");
        assert_eq!(result[0].issue_count, 1);
        assert_eq!(result[0].severity_score, 3); // high = 3
        let mut expected = std::collections::HashMap::new();
        expected.insert("high".to_string(), 1);
        assert_eq!(result[0].findings_by_severity, expected);
    }

    #[test]
    fn test_aggregate_single_file_multiple_findings() {
        let checks = vec![make_check(vec![
            make_finding("main.rs", "critical"),
            make_finding("main.rs", "high"),
            make_finding("main.rs", "medium"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file, "main.rs");
        assert_eq!(result[0].issue_count, 3);
        assert_eq!(result[0].severity_score, 4 + 3 + 2); // critical+high+medium = 9
    }

    #[test]
    fn test_aggregate_multiple_files() {
        let checks = vec![make_check(vec![
            make_finding("a.rs", "high"),
            make_finding("b.rs", "low"),
            make_finding("c.rs", "critical"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 3);
        // Collect filenames
        let files: Vec<&str> = result.iter().map(|s| s.file.as_str()).collect();
        assert!(files.contains(&"a.rs"));
        assert!(files.contains(&"b.rs"));
        assert!(files.contains(&"c.rs"));
    }

    #[test]
    fn test_aggregate_severity_scoring() {
        let checks = vec![make_check(vec![
            make_finding("f.rs", "critical"), // 4
            make_finding("f.rs", "high"),      // 3
            make_finding("f.rs", "error"),      // 3 (alias for high)
            make_finding("f.rs", "medium"),    // 2
            make_finding("f.rs", "warning"),    // 2 (alias for medium)
            make_finding("f.rs", "low"),       // 1
            make_finding("f.rs", "info"),       // 0 (unknown)
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity_score, 4 + 3 + 3 + 2 + 2 + 1 + 0);
    }

    #[test]
    fn test_aggregate_findings_by_severity_map() {
        let checks = vec![make_check(vec![
            make_finding("f.rs", "critical"),
            make_finding("f.rs", "critical"),
            make_finding("f.rs", "low"),
            make_finding("f.rs", "medium"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 1);
        let map = &result[0].findings_by_severity;
        assert_eq!(map.get("critical"), Some(&2));
        assert_eq!(map.get("low"), Some(&1));
        assert_eq!(map.get("medium"), Some(&1));
        assert_eq!(map.get("high"), None);
    }

    #[test]
    fn test_aggregate_sort_order() {
        // Create files with different severity scores
        // c.rs (critical+critical = 8), a.rs (high+high = 6), b.rs (medium = 2)
        let checks = vec![make_check(vec![
            make_finding("c.rs", "critical"),
            make_finding("c.rs", "critical"),
            make_finding("a.rs", "high"),
            make_finding("a.rs", "high"),
            make_finding("b.rs", "medium"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 3);
        // Sorted by severity_score descending: c.rs(8) → a.rs(6) → b.rs(2)
        assert_eq!(result[0].file, "c.rs");
        assert_eq!(result[1].file, "a.rs");
        assert_eq!(result[2].file, "b.rs");
    }

    #[test]
    fn test_aggregate_sort_tiebreaker() {
        // Different severity_score → sorted by score desc
        // a.rs: high(3) + medium(2) + low(1) = 6, issues = 3
        // b.rs: high(3) + medium(2) + high(3) = 8, issues = 3
        // Sort: b.rs(8) > a.rs(6)
        let checks = vec![make_check(vec![
            make_finding("a.rs", "high"),
            make_finding("a.rs", "medium"),
            make_finding("a.rs", "low"),
            make_finding("b.rs", "high"),
            make_finding("b.rs", "medium"),
            make_finding("b.rs", "high"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result[0].file, "b.rs");
        assert_eq!(result[1].file, "a.rs");
    }

    #[test]
    fn test_aggregate_sort_tiebreaker_same_score() {
        // Same severity_score AND same issue_count → stable ordering
        // a.rs: high(3) + high(3) + low(1) = 7, issues = 3
        // b.rs: critical(4) + medium(2) + low(1) = 7, issues = 3
        // c.rs: high(3) + medium(2) + medium(2) = 7, issues = 3
        // All same score and count → tie is stable (HashMap iteration order, but the sort is stable)
        let checks = vec![make_check(vec![
            make_finding("a.rs", "high"),
            make_finding("a.rs", "high"),
            make_finding("a.rs", "low"),
            make_finding("b.rs", "critical"),
            make_finding("b.rs", "medium"),
            make_finding("b.rs", "low"),
            make_finding("c.rs", "high"),
            make_finding("c.rs", "medium"),
            make_finding("c.rs", "medium"),
        ])];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].severity_score, 7);
        assert_eq!(result[1].severity_score, 7);
        assert_eq!(result[2].severity_score, 7);
        assert_eq!(result[0].issue_count, 3);
        assert_eq!(result[1].issue_count, 3);
        assert_eq!(result[2].issue_count, 3);
    }

    #[test]
    fn test_aggregate_cross_check_accumulation() {
        // Findings from different checks for the same file should accumulate
        let check1 = make_check(vec![make_finding("shared.rs", "high")]);
        let check2 = make_check(vec![make_finding("shared.rs", "critical")]);
        let checks = vec![check1, check2];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file, "shared.rs");
        assert_eq!(result[0].issue_count, 2);
        assert_eq!(result[0].severity_score, 3 + 4); // high + critical
    }

    #[test]
    fn test_aggregate_truncate() {
        // Create more than 20 unique files → result should be truncated to 20
        let mut findings = Vec::new();
        for i in 0..25 {
            let file = format!("f{:02}.rs", i);
            findings.push(make_finding(&file, "low"));
        }
        let checks = vec![make_check(findings)];
        let result = aggregate_file_summary(&checks);
        assert_eq!(result.len(), 20, "should truncate to 20 files");
    }
}
