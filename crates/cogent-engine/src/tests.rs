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

// ═══════════════════════════════════════════
// summary_u64
// ═══════════════════════════════════════════

#[test]
fn test_summary_u64_present() {
    let data = serde_json::json!({"summary": {"findings": 42}});
    assert_eq!(crate::checks::summary_u64(&data, "findings"), 42);
}

#[test]
fn test_summary_u64_missing_field() {
    let data = serde_json::json!({"summary": {"other": 10}});
    assert_eq!(crate::checks::summary_u64(&data, "findings"), 0);
}

#[test]
fn test_summary_u64_missing_summary() {
    let data = serde_json::json!({"not_summary": {}});
    assert_eq!(crate::checks::summary_u64(&data, "findings"), 0);
}

#[test]
fn test_summary_u64_null_summary() {
    let data = serde_json::json!({"summary": null});
    assert_eq!(crate::checks::summary_u64(&data, "findings"), 0);
}

#[test]
fn test_summary_u64_zero_value() {
    let data = serde_json::json!({"summary": {"findings": 0}});
    assert_eq!(crate::checks::summary_u64(&data, "findings"), 0);
}

// ═══════════════════════════════════════════
// summary_f64
// ═══════════════════════════════════════════

#[test]
fn test_summary_f64_present() {
    let data = serde_json::json!({"summary": {"score": 3.5}});
    assert_eq!(crate::checks::summary_f64(&data, "score"), 3.5);
}

#[test]
fn test_summary_f64_missing_field() {
    let data = serde_json::json!({"summary": {"other": 1.0}});
    assert_eq!(crate::checks::summary_f64(&data, "score"), 0.0);
}

#[test]
fn test_summary_f64_missing_summary() {
    let data = serde_json::json!({});
    assert_eq!(crate::checks::summary_f64(&data, "score"), 0.0);
}

#[test]
fn test_summary_f64_integer_value() {
    let data = serde_json::json!({"summary": {"score": 5}});
    assert_eq!(crate::checks::summary_f64(&data, "score"), 5.0);
}

// ═══════════════════════════════════════════
// Mock-based tests for remaining check_*_with_runner functions
// ═══════════════════════════════════════════

// ── taint ──

#[test]
fn test_check_taint_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "taint:.:--format:json",
        serde_json::json!({"summary": {"violations_count": 2}}),
    );
    let result = crate::checks::check_taint_with_runner(".", false, 5, &runner);
    assert!(result.passed, "2 violations <= 5");
    assert_eq!(result.score, Some(2.0));
}

#[test]
fn test_check_taint_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "taint:.:--format:json",
        serde_json::json!({"summary": {"violations_count": 10}}),
    );
    let result = crate::checks::check_taint_with_runner(".", false, 5, &runner);
    assert!(!result.passed, "10 violations > 5");
}

// ── duplication ──

#[test]
fn test_check_dupfind_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "dupfind:.:--format:json",
        serde_json::json!({"summary": {"total_groups": 2.0}}),
    );
    let result = crate::checks::check_dupfind_with_runner(".", false, 5.0, &runner);
    assert!(result.passed, "2 groups <= 5");
    assert_eq!(result.score, Some(2.0));
}

#[test]
fn test_check_dupfind_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "dupfind:.:--format:json",
        serde_json::json!({"summary": {"total_groups": 10.0}}),
    );
    let result = crate::checks::check_dupfind_with_runner(".", false, 5.0, &runner);
    assert!(!result.passed, "10 groups > 5");
}

// ── riskmap ──

#[test]
fn test_check_riskmap_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "riskmap:.:--format:json",
        serde_json::json!({"files": [{"risk_score": 10.0}, {"risk_score": 30.0}]}),
    );
    let result = crate::checks::check_riskmap_with_runner(".", false, 50.0, &runner);
    assert!(result.passed, "max risk 30 <= 50");
}

#[test]
fn test_check_riskmap_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "riskmap:.:--format:json",
        serde_json::json!({"files": [{"risk_score": 80.0}]}),
    );
    let result = crate::checks::check_riskmap_with_runner(".", false, 50.0, &runner);
    assert!(!result.passed, "max risk 80 > 50");
}

#[test]
fn test_check_riskmap_no_files() {
    let runner = MockToolRunner::new().with_response(
        "riskmap:.:--format:json",
        serde_json::json!({"files": []}),
    );
    let result = crate::checks::check_riskmap_with_runner(".", false, 50.0, &runner);
    assert!(result.passed, "no files → no risk");
    assert_eq!(result.score, Some(0.0));
}

// ── coupling ──

#[test]
fn test_check_coupling_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "coupling:.:--format:json",
        serde_json::json!({"summary": {"avg_fan_out": 3.0}}),
    );
    let result = crate::checks::check_coupling_with_runner(".", 5, &runner);
    assert!(result.passed, "fan-out 3 <= 5");
}

#[test]
fn test_check_coupling_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "coupling:.:--format:json",
        serde_json::json!({"summary": {"avg_fan_out": 10.0}}),
    );
    let result = crate::checks::check_coupling_with_runner(".", 5, &runner);
    assert!(!result.passed, "fan-out 10 > 5");
}

// ── propcov ──

#[test]
fn test_check_propcov_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "propcov:.:--format:json",
        serde_json::json!({"summary": {"coverage_percentage": 85.0}}),
    );
    let result = crate::checks::check_propcov_with_runner(".", false, 50.0, &runner);
    assert!(result.passed, "85% >= 50%");
}

#[test]
fn test_check_propcov_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "propcov:.:--format:json",
        serde_json::json!({"summary": {"coverage_percentage": 20.0}}),
    );
    let result = crate::checks::check_propcov_with_runner(".", false, 50.0, &runner);
    assert!(!result.passed, "20% < 50%");
}

// ── fuzz ──

#[test]
fn test_check_fuzz_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "fuzz:.:--format:json",
        serde_json::json!({"summary": {"fuzzable_functions": 0}}),
    );
    let result = crate::checks::check_fuzz_with_runner(".", false, 5, &runner);
    assert!(result.passed, "0 fuzzable <= 5");
}

#[test]
fn test_check_fuzz_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "fuzz:.:--format:json",
        serde_json::json!({"summary": {"fuzzable_functions": 10}}),
    );
    let result = crate::checks::check_fuzz_with_runner(".", false, 5, &runner);
    assert!(!result.passed, "10 fuzzable > 5");
}

// ── linelen ──

#[test]
fn test_check_linelen_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "linelen:.:--format:json",
        serde_json::json!({"summary": {"fn_violations": 1, "file_violations": 0}}),
    );
    let result = crate::checks::check_linelen_with_runner(".", false, 5, &runner);
    assert!(result.passed, "1 violation <= 5");
}

#[test]
fn test_check_linelen_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "linelen:.:--format:json",
        serde_json::json!({"summary": {"fn_violations": 8, "file_violations": 2}}),
    );
    let result = crate::checks::check_linelen_with_runner(".", false, 5, &runner);
    assert!(!result.passed, "10 violations > 5");
}

// ── halstead ──

#[test]
fn test_check_halstead_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "halstead:.:--format:json:--max-bugs:2",
        serde_json::json!({"summary": {"files_exceeding_bugs_threshold": 0, "total_bugs_estimated": 1.5}}),
    );
    let result = crate::checks::check_halstead_with_runner(".", false, 2.0, &runner);
    assert!(result.passed, "0 files exceed");
}

#[test]
fn test_check_halstead_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "halstead:.:--format:json:--max-bugs:2",
        serde_json::json!({"summary": {"files_exceeding_bugs_threshold": 3, "total_bugs_estimated": 8.0}}),
    );
    let result = crate::checks::check_halstead_with_runner(".", false, 2.0, &runner);
    assert!(!result.passed, "3 files exceed");
}

// ── deadcode ──

#[test]
fn test_check_deadcode_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "deadcode:.:--format:json",
        serde_json::json!({"summary": {"total_findings": 3}}),
    );
    let result = crate::checks::check_deadcode_with_runner(".", false, 10, &runner);
    assert!(result.passed, "3 findings <= 10");
}

#[test]
fn test_check_deadcode_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "deadcode:.:--format:json",
        serde_json::json!({"summary": {"total_findings": 20}}),
    );
    let result = crate::checks::check_deadcode_with_runner(".", false, 10, &runner);
    assert!(!result.passed, "20 findings > 10");
}

// ── sast ──

#[test]
fn test_check_sast_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "sast:.:--format:json:--max-findings:10",
        serde_json::json!({"summary": {"total_findings": 3, "critical": 0, "high": 1}}),
    );
    let result = crate::checks::check_sast_with_runner(".", false, 10, &runner);
    assert!(result.passed, "3 findings <= 10");
}

#[test]
fn test_check_sast_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "sast:.:--format:json:--max-findings:10",
        serde_json::json!({"summary": {"total_findings": 15, "critical": 2, "high": 5}}),
    );
    let result = crate::checks::check_sast_with_runner(".", false, 10, &runner);
    assert!(!result.passed, "15 findings > 10");
}

#[test]
fn test_check_sast_skipped_with_null_data() {
    let runner = MockToolRunner::new().with_response(
        "sast:.:--format:json:--max-findings:10",
        serde_json::Value::Null,
    );
    let result = crate::checks::check_sast_with_runner(".", false, 10, &runner);
    assert!(result.passed, "skipped tools should report passed");
    assert!(result.message.contains("Skipped"), "message should indicate skipped");
}

// ── crypto ──

#[test]
fn test_check_crypto_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "cryptocheck:.:--format:json:--max-findings:10",
        serde_json::json!({"summary": {"total_findings": 2, "critical": 0}}),
    );
    let result = crate::checks::check_crypto_with_runner(".", false, 10, &runner);
    assert!(result.passed, "2 findings <= 10");
}

#[test]
fn test_check_crypto_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "cryptocheck:.:--format:json:--max-findings:10",
        serde_json::json!({"summary": {"total_findings": 20, "critical": 1}}),
    );
    let result = crate::checks::check_crypto_with_runner(".", false, 10, &runner);
    assert!(!result.passed, "20 findings > 10");
}

#[test]
fn test_check_crypto_skipped_with_null_data() {
    let runner = MockToolRunner::new().with_response(
        "cryptocheck:.:--format:json:--max-findings:10",
        serde_json::Value::Null,
    );
    let result = crate::checks::check_crypto_with_runner(".", false, 10, &runner);
    assert!(result.passed);
    assert!(result.message.contains("Skipped"));
}

// ── licenses ──

#[test]
fn test_check_licenses_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "licenses:.:--format:json:--max-violations:10",
        serde_json::json!({"summary": {"violations": 2, "packages_scanned": 50}}),
    );
    let result = crate::checks::check_licenses_with_runner(".", 10, &runner);
    assert!(result.passed, "2 violations <= 10");
}

#[test]
fn test_check_licenses_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "licenses:.:--format:json:--max-violations:10",
        serde_json::json!({"summary": {"violations": 15, "packages_scanned": 50}}),
    );
    let result = crate::checks::check_licenses_with_runner(".", 10, &runner);
    assert!(!result.passed, "15 violations > 10");
}

#[test]
fn test_check_licenses_skipped_with_null_data() {
    let runner = MockToolRunner::new().with_response(
        "licenses:.:--format:json:--max-violations:10",
        serde_json::Value::Null,
    );
    let result = crate::checks::check_licenses_with_runner(".", 10, &runner);
    assert!(result.passed);
    assert!(result.message.contains("Skipped"));
}

// ── typecov ──

#[test]
fn test_check_typecov_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "typecov:.:--format:json:--min-pct:50",
        serde_json::json!({"summary": {"overall_coverage_pct": 95.0, "files_below_threshold": 0}}),
    );
    let result = crate::checks::check_typecov_with_runner(".", false, 50.0, &runner);
    assert!(result.passed, "95% >= 50%");
}

#[test]
fn test_check_typecov_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "typecov:.:--format:json:--min-pct:50",
        serde_json::json!({"summary": {"overall_coverage_pct": 30.0, "files_below_threshold": 5}}),
    );
    let result = crate::checks::check_typecov_with_runner(".", false, 50.0, &runner);
    assert!(!result.passed, "5 files below threshold");
}

// ── vulnscan ──

#[test]
fn test_check_vulnscan_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "vulnscan:.:--format:json:--max-critical:1:--max-high:5",
        serde_json::json!({"summary": {"critical": 0, "high": 2, "total": 2}}),
    );
    let result = crate::checks::check_vulnscan_with_runner(".", 1, 5, &runner);
    assert!(result.passed, "0 critical <= 1, 2 high <= 5");
}

#[test]
fn test_check_vulnscan_fails_critical_exceeds() {
    let runner = MockToolRunner::new().with_response(
        "vulnscan:.:--format:json:--max-critical:1:--max-high:5",
        serde_json::json!({"summary": {"critical": 3, "high": 1, "total": 4}}),
    );
    let result = crate::checks::check_vulnscan_with_runner(".", 1, 5, &runner);
    assert!(!result.passed, "3 critical > 1");
}

#[test]
fn test_check_vulnscan_skipped_with_null_data() {
    let runner = MockToolRunner::new().with_response(
        "vulnscan:.:--format:json:--max-critical:1:--max-high:5",
        serde_json::Value::Null,
    );
    let result = crate::checks::check_vulnscan_with_runner(".", 1, 5, &runner);
    assert!(result.passed);
    assert!(result.message.contains("Skipped"));
}

// ── cohesion ──

#[test]
fn test_check_cohesion_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "cohesion:.:--format:json",
        serde_json::json!({"summary": {"violations": 2, "avg_lcom": 1.5}}),
    );
    let result = crate::checks::check_cohesion_with_runner(".", false, 5, &runner);
    assert!(result.passed, "2 violations <= 5");
}

#[test]
fn test_check_cohesion_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "cohesion:.:--format:json",
        serde_json::json!({"summary": {"violations": 8, "avg_lcom": 3.2}}),
    );
    let result = crate::checks::check_cohesion_with_runner(".", false, 5, &runner);
    assert!(!result.passed, "8 violations > 5");
}

// ── comments ──

#[test]
fn test_check_comments_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "comments:.:--format:json:--min-ratio:0.05",
        serde_json::json!({"summary": {"files_below_threshold": 0, "overall_comment_ratio": 0.15}}),
    );
    let result = crate::checks::check_comments_with_runner(".", false, 0.05, &runner);
    assert!(result.passed, "0 files below threshold");
}

#[test]
fn test_check_comments_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "comments:.:--format:json:--min-ratio:0.05",
        serde_json::json!({"summary": {"files_below_threshold": 3, "overall_comment_ratio": 0.02}}),
    );
    let result = crate::checks::check_comments_with_runner(".", false, 0.05, &runner);
    assert!(!result.passed, "3 files below threshold");
}

// ── errhandle ──

#[test]
fn test_check_errhandle_passes_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "errhandle:.:--format:json",
        serde_json::json!({"summary": {"total_findings": 10}}),
    );
    let result = crate::checks::check_errhandle_with_runner(".", false, 50, &runner);
    assert!(result.passed, "10 findings <= 50");
}

#[test]
fn test_check_errhandle_fails_with_mock() {
    let runner = MockToolRunner::new().with_response(
        "errhandle:.:--format:json",
        serde_json::json!({"summary": {"total_findings": 100}}),
    );
    let result = crate::checks::check_errhandle_with_runner(".", false, 50, &runner);
    assert!(!result.passed, "100 findings > 50");
}
