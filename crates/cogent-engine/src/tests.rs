//! Tests for cogent-engine helpers.

use crate::{MockToolRunner, ToolRunner};
use cogent_common::CheckResult;
use std::time::Instant;

#[test]
fn test_extract_findings_from_details_empty() {
    let details = serde_json::json!({});
    let findings = crate::extract_findings_from_details(&details, "rule", "medium");
    assert!(findings.is_empty());
}

#[test]
fn test_extract_findings_from_details_items() {
    let details = serde_json::json!({
        "items": [
            {"file": "a.rs", "line": 5, "severity": "high", "message": "bad", "rule_id": "r1"},
            {"file": "b.rs", "line": 10, "message": "warn", "rule_id": "r2"},
        ]
    });
    let findings = crate::extract_findings_from_details(&details, "default", "medium");
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].file, "a.rs");
    assert_eq!(findings[0].severity, "high");
    assert_eq!(findings[1].file, "b.rs");
    assert_eq!(findings[1].severity, "medium");
}

#[test]
fn test_aggregate_file_summary() {
    let checks = vec![
        CheckResult {
            name: "debt".into(), passed: false, score: None, threshold: None,
            message: "".into(), details: serde_json::Value::Null,
            severity: Some("medium".into()), help: None, rule_id: None,
            findings: vec![
                cogent_common::Finding {
                    file: "src/main.rs".into(), line: Some(10), column: None,
                    severity: "high".into(), message: "todo".into(),
                    rule_id: "debt-todo".into(), fix_hint: "".into(),
                    evidence: None, suggested_fix: None, controls: None,
                },
            ],
        },
        CheckResult {
            name: "secrets".into(), passed: false, score: None, threshold: None,
            message: "".into(), details: serde_json::Value::Null,
            severity: Some("high".into()), help: None, rule_id: None,
            findings: vec![
                cogent_common::Finding {
                    file: "src/main.rs".into(), line: Some(20), column: None,
                    severity: "critical".into(), message: "key".into(),
                    rule_id: "secret-key".into(), fix_hint: "".into(),
                    evidence: None, suggested_fix: None, controls: None,
                },
                cogent_common::Finding {
                    file: "src/lib.rs".into(), line: Some(5), column: None,
                    severity: "medium".into(), message: "token".into(),
                    rule_id: "secret-token".into(), fix_hint: "".into(),
                    evidence: None, suggested_fix: None, controls: None,
                },
            ],
        },
    ];
    let summary = crate::aggregate_file_summary(&checks);
    assert_eq!(summary.len(), 2);
    let main_summary = summary.iter().find(|s| s.file == "src/main.rs").unwrap();
    assert_eq!(main_summary.issue_count, 2);
    assert_eq!(main_summary.severity_score, 7); // high=3 + critical=4
    let lib_summary = summary.iter().find(|s| s.file == "src/lib.rs").unwrap();
    assert_eq!(lib_summary.issue_count, 1);
    assert_eq!(lib_summary.severity_score, 2); // medium=2
}

// ═══════════════════════════════════════════
// Mock-based tests for ToolRunner-wired check functions
// ═══════════════════════════════════════════

#[test]
fn test_check_access_control_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "access-control:.:--format:json:--max-violations:5",
        serde_json::json!({
            "summary": { "total_findings": 2, "critical": 0 }
        }),
    );
    let result = crate::checks::check_access_control_with_runner(".", false, 5, &runner);
    assert!(result.passed);
    assert_eq!(result.score, Some(2.0));
}

#[test]
fn test_check_access_control_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "access-control:.:--format:json:--max-violations:3",
        serde_json::json!({
            "summary": { "total_findings": 5, "critical": 1 }
        }),
    );
    let result = crate::checks::check_access_control_with_runner(".", false, 3, &runner);
    assert!(!result.passed);
    assert_eq!(result.score, Some(5.0));
}

#[test]
fn test_check_supply_chain_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "supply-chain:.:--format:json:--max-risks:10",
        serde_json::json!({
            "summary": { "total_risks": 3, "critical": 0 }
        }),
    );
    let result = crate::checks::check_supply_chain_with_runner(".", 10, &runner);
    assert!(result.passed);
    assert_eq!(result.score, Some(3.0));
}

#[test]
fn test_check_supply_chain_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "supply-chain:.:--format:json:--max-risks:2",
        serde_json::json!({
            "summary": { "total_risks": 5, "critical": 2 }
        }),
    );
    let result = crate::checks::check_supply_chain_with_runner(".", 2, &runner);
    assert!(!result.passed);
    assert_eq!(result.score, Some(5.0));
}

#[test]
fn test_check_secrets_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let result = crate::checks::check_secrets_with_runner(".", false, 5, &runner);
    assert!(result.passed);
    assert_eq!(result.score, Some(0.0));
}

#[test]
fn test_check_secrets_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json",
        serde_json::json!({
            "summary": { "findings_count": 7 }
        }),
    );
    let result = crate::checks::check_secrets_with_runner(".", false, 5, &runner);
    assert!(!result.passed);
    assert_eq!(result.score, Some(7.0));
}
