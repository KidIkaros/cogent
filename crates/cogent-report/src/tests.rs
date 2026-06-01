//! Tests for cogent-report formatters and HTML helpers.

use cogent_common::{CheckReport, CheckResult, CheckSummary, Finding};

#[test]
fn test_html_escape() {
    assert_eq!(crate::html_escape("<script>"), "&lt;script&gt;");
    assert_eq!(crate::html_escape("foo & bar"), "foo &amp; bar");
    assert_eq!(crate::html_escape(r#""quote""#), "&quot;quote&quot;");
}

#[test]
fn test_output_json_does_not_panic() {
    let report = CheckReport {
        passed: true,
        path: ".".into(),
        checks: vec![CheckResult {
            name: "crap".into(),
            passed: true,
            score: None,
            threshold: None,
            message: "ok".into(),
            details: serde_json::Value::Null,
            severity: None,
            help: None,
            rule_id: None,
            findings: vec![],
        }],
        summary: CheckSummary {
            total_checks: 1,
            passed_checks: 1,
            failed_checks: 0,
            functions_analyzed: 0,
            avg_complexity: 0.0,
            avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    // output_json writes to stdout; we just ensure it doesn't panic
    crate::formatters::output_json(&report);
}

#[test]
fn test_render_markdown_report_contains_headers() {
    let report = CheckReport {
        passed: false,
        path: ".".into(),
        checks: vec![
            CheckResult {
                name: "secrets".into(), passed: false, score: None, threshold: None,
                message: "found key".into(), details: serde_json::Value::Null,
                severity: Some("high".into()), help: None, rule_id: None,
                findings: vec![Finding {
                    file: "src/main.rs".into(), line: Some(10), column: None,
                    severity: "high".into(), message: "hardcoded key".into(),
                    rule_id: "secrets".into(), fix_hint: "use env var".into(),
                    evidence: None, suggested_fix: None, controls: None,
                }],
            },
        ],
        summary: CheckSummary {
            total_checks: 1, passed_checks: 0, failed_checks: 1,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let md = crate::html::render_markdown_report(
        &report, "myproject", "2024-01-01", &["secrets"], &["crap"], &["licenses"],
    );
    assert!(md.contains("Cogent Audit Report"));
    assert!(md.contains("❌ FAILED"));
    assert!(md.contains("secrets"));
    assert!(md.contains("hardcoded key"));
}

#[test]
fn test_render_html_report_contains_sections() {
    let report = CheckReport {
        passed: true,
        path: ".".into(),
        checks: vec![CheckResult {
            name: "crap".into(), passed: true, score: None, threshold: None,
            message: "ok".into(), details: serde_json::Value::Null,
            severity: None, help: None, rule_id: None, findings: vec![],
        }],
        summary: CheckSummary {
            total_checks: 1, passed_checks: 1, failed_checks: 0,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let html = crate::html::render_html_report(
        &report, "myproject", "2024-01-01", &["secrets"], &["crap"], &["licenses"],
    );
    assert!(html.contains("<html"));
    assert!(html.contains("PASSED"));
}
