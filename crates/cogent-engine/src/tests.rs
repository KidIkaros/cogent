//! Tests for cogent-engine helpers.

use crate::MockToolRunner;
use cogent_common::CheckResult;

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
    let result = crate::checks::check_secrets_with_excludes(".", false, 5, &[], &runner);
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
    let result = crate::checks::check_secrets_with_excludes(".", false, 5, &[], &runner);
    assert!(!result.passed);
    assert_eq!(result.score, Some(7.0));
}

// ═══════════════════════════════════════════
// Integration tests: secrets_exclude_paths pipeline
// ═══════════════════════════════════════════

/// Verify that `check_secrets_with_excludes` passes `--exclude` to the secrets
/// binary when exclude paths are provided, and omits it when empty.
#[test]
fn test_check_secrets_excludes_passes_arg_to_binary() {
    // When exclude_paths is non-empty, the args should include --exclude
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:crates/engine,tests/fixtures",
        serde_json::json!({
            "summary": { "findings_count": 2 }
        }),
    );
    let excludes = vec!["crates/engine".to_string(), "tests/fixtures".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed, "2 findings <= 10");
    assert_eq!(result.score, Some(2.0));
}

/// When exclude_paths is empty, no --exclude flag should appear in args.
#[test]
fn test_check_secrets_no_excludes_omits_flag() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &[], &runner,
    );
    assert!(result.passed, "0 findings <= 10");
    assert_eq!(result.score, Some(0.0));
}

/// Verify that with excludes + recursive, both flags are present in args.
#[test]
fn test_check_secrets_excludes_with_recursive() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--recursive:--exclude:vendor",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let excludes = vec!["vendor".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", true, 10, &excludes, &runner,
    );
    assert!(result.passed, "0 findings <= 10");
}

/// Verify the contract that the registry relies on: check_secrets_with_excludes
/// correctly forwards CheckThresholds.secrets_exclude_paths as --exclude args.
#[test]
fn test_check_secrets_excludes_forwards_thresholds_exclude_paths() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:crates/engine",
        serde_json::json!({
            "summary": { "findings_count": 3 }
        }),
    );
    // Simulate what the registry does: extract secrets_exclude_paths from thresholds
    let mut thresholds = crate::CheckThresholds::default();
    thresholds.max_secrets = 5;
    thresholds.secrets_exclude_paths = vec!["crates/engine".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, thresholds.max_secrets, &thresholds.secrets_exclude_paths, &runner,
    );
    assert!(result.passed, "3 findings <= 5");
    assert_eq!(result.score, Some(3.0));
}

/// Regression guard: different exclude_paths produce different finding counts,
/// confirming the mock contract matches the arg-construction logic.
#[test]
fn test_exclude_paths_changes_finding_count() {
    // Without excludes, the binary would report 10 findings
    let runner_no_exclude = MockToolRunner::new().with_response(
        "secrets:.:--format:json",
        serde_json::json!({
            "summary": { "findings_count": 10 }
        }),
    );
    let result_no_exclude = crate::checks::check_secrets_with_excludes(
        ".", false, 20, &[], &runner_no_exclude,
    );
    assert_eq!(result_no_exclude.score, Some(10.0));

    // With excludes, the binary reports only 3 findings (excluded path suppressed)
    let runner_with_exclude = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:crates/engine",
        serde_json::json!({
            "summary": { "findings_count": 3 }
        }),
    );
    let excludes = vec!["crates/engine".to_string()];
    let result_with_exclude = crate::checks::check_secrets_with_excludes(
        ".", false, 20, &excludes, &runner_with_exclude,
    );
    assert_eq!(result_with_exclude.score, Some(3.0));
    assert!(result_with_exclude.passed, "3 findings <= 20");

    // The excluded path reduced findings from 10 to 3
    assert!(result_with_exclude.score.unwrap() < result_no_exclude.score.unwrap());
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

// ═══════════════════════════════════════════
// CheckThresholds default values
// ═══════════════════════════════════════════

#[test]
fn test_default_thresholds_secrets_exclude_empty() {
    let t = crate::CheckThresholds::default();
    assert!(t.secrets_exclude_paths.is_empty(), "default secrets_exclude_paths should be empty");
    assert_eq!(t.max_crap, 30.0);
    assert_eq!(t.max_secrets, 0);
    assert_eq!(t.max_vuln_critical, 0);
}

#[test]
fn test_default_thresholds_debuggability_nonzero() {
    let t = crate::CheckThresholds::default();
    assert!(t.max_debuggability > 0, "default max_debuggability should be > 0");
}

#[test]
fn test_default_thresholds_coverage_path_is_none() {
    let t = crate::CheckThresholds::default();
    assert!(t.coverage_path.is_none(), "default coverage_path should be None");
}

#[test]
fn test_default_thresholds_known_values() {
    let t = crate::CheckThresholds::default();
    assert_eq!(t.max_crap, 30.0);
    assert_eq!(t.min_doc, 50.0);
    assert_eq!(t.max_debt, 1000);
    assert_eq!(t.max_secrets, 0);
    assert_eq!(t.max_vuln_critical, 0);
    assert_eq!(t.max_vuln_high, 0);
    assert_eq!(t.max_sast, 0);
    assert_eq!(t.max_crypto, 0);
    assert_eq!(t.max_license_violations, 0);
    assert_eq!(t.max_access_control, 0);
    assert_eq!(t.max_supply_chain, 0);
}

// ═══════════════════════════════════════════
// load_from_config negative / edge-case tests
// ═══════════════════════════════════════════

/// Missing file should return defaults (no panic).
#[test]
fn test_load_from_config_missing_file() {
    let t = crate::CheckThresholds::load_from_config("/nonexistent/path/to/config.toml");
    assert_eq!(t.max_crap, 30.0, "missing file should return default max_crap");
    assert!(t.secrets_exclude_paths.is_empty(), "missing file should return empty excludes");
    assert!(t.coverage_path.is_none(), "missing file should return None coverage_path");
}

/// Empty file should return all defaults.
#[test]
fn test_load_from_config_empty_file() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_empty_config.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        // write nothing
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 30.0);
    assert!(t.secrets_exclude_paths.is_empty());
    let _ = std::fs::remove_file(&path);
}

/// Comments-only file should return all defaults.
#[test]
fn test_load_from_config_comments_only() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_comments_only.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# This is a comment").unwrap();
        writeln!(f, "# Another comment").unwrap();
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 30.0);
    assert_eq!(t.max_secrets, 0);
    assert!(t.secrets_exclude_paths.is_empty());
    let _ = std::fs::remove_file(&path);
}

/// Mixed numeric and string keys — both parsed correctly.
#[test]
fn test_load_from_config_mixed_keys() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_mixed_keys.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_avg = 42.0").unwrap();
        writeln!(f, "max_secrets = 7").unwrap();
        writeln!(f, "secrets_exclude = [\"a\", \"b\"]").unwrap();
        writeln!(f, "min_pct = 80.0").unwrap();
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 42.0);
    assert_eq!(t.max_secrets, 7);
    assert_eq!(t.secrets_exclude_paths, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(t.min_doc, 80.0);
    let _ = std::fs::remove_file(&path);
}

/// Numeric key with no space after `=` should still parse.
#[test]
fn test_load_from_config_no_space_after_eq() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_no_space.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_avg=99.0").unwrap();
        writeln!(f, "max_secrets=3").unwrap();
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 99.0);
    assert_eq!(t.max_secrets, 3);
    let _ = std::fs::remove_file(&path);
}

/// Duplicate keys — last value wins (line-by-line parser behavior, not TOML spec).
#[test]
fn test_load_from_config_duplicate_keys_last_wins() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_dup_keys.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_avg = 10.0").unwrap();
        writeln!(f, "max_avg = 50.0").unwrap();
        writeln!(f, "secrets_exclude = [\"first\"]").unwrap();
        writeln!(f, "secrets_exclude = [\"second\", \"third\"]").unwrap();
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 50.0, "last value should win");
    assert_eq!(t.secrets_exclude_paths, vec!["second".to_string(), "third".to_string()]);
    let _ = std::fs::remove_file(&path);
}

/// Config with only `secrets_exclude` and no numeric keys — all defaults preserved.
#[test]
fn test_load_from_config_only_exclude_preserves_defaults() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_only_exclude.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "secrets_exclude = [\"vendor\", \"test\"]").unwrap();
    }
    let t = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(t.max_crap, 30.0, "numeric defaults should be preserved");
    assert_eq!(t.min_doc, 50.0);
    assert_eq!(t.max_debt, 1000);
    assert_eq!(t.max_secrets, 0);
    assert_eq!(t.secrets_exclude_paths, vec!["vendor".to_string(), "test".to_string()]);
    let _ = std::fs::remove_file(&path);
}

// ═══════════════════════════════════════════
// load_from_config integration test for secrets_exclude_paths
// ═══════════════════════════════════════════

#[test]
fn test_load_from_config_populates_secrets_exclude_paths() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_exclude_paths.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_avg = 25.0").unwrap();
        writeln!(f, "max_secrets = 5").unwrap();
        writeln!(f, "secrets_exclude = [\"vendor\", \"tests\", \"docs\"]").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(thresholds.max_crap, 25.0);
    assert_eq!(thresholds.max_secrets, 5);
    assert_eq!(
        thresholds.secrets_exclude_paths,
        vec!["vendor".to_string(), "tests".to_string(), "docs".to_string()]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_load_from_config_no_secrets_exclude() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_no_exclude.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_avg = 20.0").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(thresholds.max_crap, 20.0);
    assert!(thresholds.secrets_exclude_paths.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_load_from_config_bare_comma_exclude() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_bare_comma.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "secrets_exclude = vendor, test, docs").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(
        thresholds.secrets_exclude_paths,
        vec!["vendor".to_string(), "test".to_string(), "docs".to_string()]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_load_from_config_empty_array_exclude() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_empty_array.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "secrets_exclude = []").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert!(thresholds.secrets_exclude_paths.is_empty());
    let _ = std::fs::remove_file(&path);
}

// ═══════════════════════════════════════════
// Edge case tests: malformed exclude paths
// ═══════════════════════════════════════════

/// Empty strings in exclude_paths are filtered out before joining.
#[test]
fn test_check_secrets_excludes_with_empty_string_in_list() {
    // After Phase 5 filter, empty string is removed → only valid-path in --exclude
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:valid-path",
        serde_json::json!({
            "summary": { "findings_count": 1 }
        }),
    );
    let excludes = vec!["valid-path".to_string(), String::new()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed, "1 finding <= 10");
    assert_eq!(result.score, Some(1.0));
}

/// Exclude path containing a comma — the join will split it, but the mock
/// verifies the arg is constructed correctly.
#[test]
fn test_check_secrets_excludes_with_comma_in_path() {
    let runner = MockToolRunner::new().with_response(
        r"secrets:.:--format:json:--exclude:path\with\,comma",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    // A single path that contains a comma — this is an edge case
    let excludes = vec![r"path\with\,comma".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed);
    assert_eq!(result.score, Some(0.0));
}

/// Exclude path containing a colon — should still work as a substring match.
#[test]
fn test_check_secrets_excludes_with_colon_in_path() {
    let runner = MockToolRunner::new().with_response(
        r"secrets:.:--format:json:--exclude:C:\Users\test",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let excludes = vec![r"C:\Users\test".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed);
}

/// Very long exclude string — should not cause issues.
#[test]
fn test_check_secrets_excludes_with_long_path() {
    let long_path = "a".repeat(500);
    let mock_key = format!("secrets:.:--format:json:--exclude:{}", long_path);
    let runner = MockToolRunner::new().with_response(
        &mock_key,
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let excludes = vec![long_path];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed);
}

/// Unicode in exclude path — should pass through correctly.
#[test]
fn test_check_secrets_excludes_with_unicode_path() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:src/日本語テスト",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let excludes = vec!["src/日本語テスト".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed);
}

/// Single exclude path — verify no trailing comma.
#[test]
fn test_check_secrets_excludes_single_path_no_trailing_comma() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:vendor",
        serde_json::json!({
            "summary": { "findings_count": 2 }
        }),
    );
    let excludes = vec!["vendor".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed, "2 findings <= 10");
}

/// Three or more exclude paths — verify correct comma joining.
#[test]
fn test_check_secrets_excludes_multiple_paths_joined() {
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:vendor,test,docs",
        serde_json::json!({
            "summary": { "findings_count": 0 }
        }),
    );
    let excludes = vec!["vendor".to_string(), "test".to_string(), "docs".to_string()];
    let result = crate::checks::check_secrets_with_excludes(
        ".", false, 10, &excludes, &runner,
    );
    assert!(result.passed);
}

// ═══════════════════════════════════════════
// Full pipeline: config → thresholds → check_secrets_with_excludes
// ═══════════════════════════════════════════

/// Verify end-to-end that a `.quality.toml` file with `secrets_exclude`
/// produces a `CheckThresholds` that forwards excludes to the secrets runner.
#[test]
fn test_config_to_check_secrets_pipeline() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_pipeline.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_secrets = 3").unwrap();
        writeln!(f, "secrets_exclude = [\"vendor\", \"tests/fixtures\"]").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert_eq!(thresholds.max_secrets, 3);
    assert_eq!(
        thresholds.secrets_exclude_paths,
        vec!["vendor".to_string(), "tests/fixtures".to_string()]
    );

    // Simulate the registry dispatching to check_secrets_with_excludes
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json:--exclude:vendor,tests/fixtures",
        serde_json::json!({"summary": {"findings_count": 1}}),
    );
    let result = crate::checks::check_secrets_with_excludes(
        ".", false,
        thresholds.max_secrets,
        &thresholds.secrets_exclude_paths,
        &runner,
    );
    assert!(result.passed, "1 finding <= 3");
    assert_eq!(result.score, Some(1.0));
    let _ = std::fs::remove_file(&path);
}

/// Config with empty `secrets_exclude` should produce no --exclude arg.
#[test]
fn test_config_empty_exclude_to_check_pipeline() {
    use std::io::Write;
    let path = std::env::temp_dir().join("cogent_engine_test_pipeline_empty.toml");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "max_secrets = 5").unwrap();
        writeln!(f, "secrets_exclude = []").unwrap();
    }
    let thresholds = crate::CheckThresholds::load_from_config(path.to_str().unwrap());
    assert!(thresholds.secrets_exclude_paths.is_empty());

    // No --exclude should appear in args
    let runner = MockToolRunner::new().with_response(
        "secrets:.:--format:json",
        serde_json::json!({"summary": {"findings_count": 0}}),
    );
    let result = crate::checks::check_secrets_with_excludes(
        ".", false,
        thresholds.max_secrets,
        &thresholds.secrets_exclude_paths,
        &runner,
    );
    assert!(result.passed);
    assert_eq!(result.score, Some(0.0));
    let _ = std::fs::remove_file(&path);
}
