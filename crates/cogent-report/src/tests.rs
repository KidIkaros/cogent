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
    crate::formatters::output_json(&report);
}

// This test is intentionally minimal — output_json is a thin wrapper over format_json
// which is tested with structural assertions above.

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

// ═══════════════════════════════════════════
// format_elapsed
// ═══════════════════════════════════════════

#[test]
fn test_format_elapsed_under_1s() {
    let d = std::time::Duration::from_millis(500);
    let s = crate::formatters::format_elapsed(d);
    assert_eq!(s, "0.5s");
}

#[test]
fn test_format_elapsed_exactly_1s() {
    let d = std::time::Duration::from_secs(1);
    let s = crate::formatters::format_elapsed(d);
    assert_eq!(s, "1.0s");
}

#[test]
fn test_format_elapsed_over_1s() {
    let d = std::time::Duration::from_secs(3);
    let s = crate::formatters::format_elapsed(d);
    assert_eq!(s, "3.0s");
}

#[test]
fn test_format_elapsed_zero() {
    let d = std::time::Duration::from_millis(0);
    let s = crate::formatters::format_elapsed(d);
    assert_eq!(s, "0.0s");
}

// ═══════════════════════════════════════════
// format_ms
// ═══════════════════════════════════════════

#[test]
fn test_format_ms_under_1000() {
    assert_eq!(crate::formatters::format_ms(500), "500ms");
}

#[test]
fn test_format_ms_exactly_1000() {
    assert_eq!(crate::formatters::format_ms(1000), "1.0s");
}

#[test]
fn test_format_ms_over_1000() {
    assert_eq!(crate::formatters::format_ms(2500), "2.5s");
}

#[test]
fn test_format_ms_zero() {
    assert_eq!(crate::formatters::format_ms(0), "0ms");
}

// ═══════════════════════════════════════════
// format_duration
// ═══════════════════════════════════════════

#[test]
fn test_format_duration_zero() {
    let d = std::time::Duration::from_secs(0);
    assert_eq!(crate::formatters::format_duration(d), "00:00");
}

#[test]
fn test_format_duration_minutes_only() {
    let d = std::time::Duration::from_secs(120);
    assert_eq!(crate::formatters::format_duration(d), "02:00");
}

#[test]
fn test_format_duration_minutes_and_seconds() {
    let d = std::time::Duration::from_secs(185);
    assert_eq!(crate::formatters::format_duration(d), "03:05");
}

#[test]
fn test_format_duration_less_than_minute() {
    let d = std::time::Duration::from_secs(45);
    assert_eq!(crate::formatters::format_duration(d), "00:45");
}

// ═══════════════════════════════════════════
// format_ts
// ═══════════════════════════════════════════

#[test]
fn test_format_ts_epoch() {
    assert_eq!(crate::formatters::format_ts(0), "1970-01-01 00:00");
}

#[test]
fn test_format_ts_typical() {
    assert_eq!(crate::formatters::format_ts(1718461800), "2024-06-30 14:30");
}

#[test]
fn test_format_ts_midnight() {
    assert_eq!(crate::formatters::format_ts(1735689600), "2025-01-15 00:00");
}

#[test]
fn test_format_ts_one_second() {
    assert_eq!(crate::formatters::format_ts(1), "1970-01-01 00:00");
}

// ═══════════════════════════════════════════
// sarif_level
// ═══════════════════════════════════════════

#[test]
fn test_sarif_level_critical() {
    assert_eq!(crate::formatters::sarif_level("critical"), "error");
}

#[test]
fn test_sarif_level_high() {
    assert_eq!(crate::formatters::sarif_level("high"), "error");
}

#[test]
fn test_sarif_level_error() {
    assert_eq!(crate::formatters::sarif_level("error"), "error");
}

#[test]
fn test_sarif_level_medium() {
    assert_eq!(crate::formatters::sarif_level("medium"), "warning");
}

#[test]
fn test_sarif_level_warning() {
    assert_eq!(crate::formatters::sarif_level("warning"), "warning");
}

#[test]
fn test_sarif_level_low() {
    assert_eq!(crate::formatters::sarif_level("low"), "note");
}

#[test]
fn test_sarif_level_note() {
    assert_eq!(crate::formatters::sarif_level("note"), "note");
}

#[test]
fn test_sarif_level_unknown_defaults_to_warning() {
    assert_eq!(crate::formatters::sarif_level("unknown"), "warning");
}

#[test]
fn test_sarif_level_case_insensitive() {
    assert_eq!(crate::formatters::sarif_level("HIGH"), "error");
    assert_eq!(crate::formatters::sarif_level("Critical"), "error");
    assert_eq!(crate::formatters::sarif_level("Medium"), "warning");
}

#[test]
fn test_sarif_level_empty() {
    assert_eq!(crate::formatters::sarif_level(""), "warning");
}

// ═══════════════════════════════════════════
// visible_len / strip_ansi
// ═══════════════════════════════════════════

#[test]
fn test_visible_len_plain() {
    assert_eq!(crate::formatters::visible_len("hello"), 5);
}

#[test]
fn test_visible_len_with_ansi() {
    assert_eq!(crate::formatters::visible_len("\x1b[31mred\x1b[0m"), 3);
}

#[test]
fn test_visible_len_empty() {
    assert_eq!(crate::formatters::visible_len(""), 0);
}

#[test]
fn test_strip_ansi_no_ansi() {
    assert_eq!(crate::formatters::strip_ansi("hello world"), "hello world");
}

#[test]
fn test_strip_ansi_red_text() {
    assert_eq!(crate::formatters::strip_ansi("\x1b[31mred\x1b[0m"), "red");
}

#[test]
fn test_strip_ansi_multiple_codes() {
    assert_eq!(
        crate::formatters::strip_ansi("\x1b[1m\x1b[32mbold green\x1b[0m"),
        "bold green"
    );
}

#[test]
fn test_strip_ansi_empty() {
    assert_eq!(crate::formatters::strip_ansi(""), "");
}

#[test]
fn test_strip_ansi_incomplete_escape() {
    assert_eq!(crate::formatters::strip_ansi("a\x1bb"), "a\x1bb");
}

// ═══════════════════════════════════════════
// escape
// ═══════════════════════════════════════════

#[test]
fn test_escape_no_special() {
    assert_eq!(crate::formatters::escape("hello"), "hello");
}

#[test]
fn test_escape_ampersand() {
    assert_eq!(crate::formatters::escape("AT&T"), "AT&T");
}

#[test]
fn test_escape_angle_brackets() {
    assert_eq!(crate::formatters::escape("<div>"), "<div>");
}

#[test]
fn test_escape_quotes() {
    let result = crate::formatters::escape("\"hello\"");
    assert!(result.contains("hello"), "should preserve non-special chars");
    assert!(!result.contains("&amp;"), "escape() does NOT use &amp;");
}

#[test]
fn test_escape_single_quote() {
    assert_eq!(crate::formatters::escape("'single'"), "'single'");
}

#[test]
fn test_escape_empty() {
    assert_eq!(crate::formatters::escape(""), "");
}

// ═══════════════════════════════════════════
// Shared helper: build a report with findings
// ═══════════════════════════════════════════

fn make_report() -> CheckReport {
    CheckReport {
        passed: false,
        path: "test/path".into(),
        checks: vec![
            CheckResult {
                name: "secrets".into(),
                passed: false,
                score: None,
                threshold: None,
                message: "found secrets".into(),
                details: serde_json::json!({
                    "items": [
                        {"type": "api_key", "file": "src/main.rs", "line": 42}
                    ]
                }),
                severity: Some("high".into()),
                help: Some("use env vars".into()),
                rule_id: Some("SEC-001".into()),
                findings: vec![
                    Finding {
                        file: "src/main.rs".into(),
                        line: Some(42),
                        column: Some(10),
                        severity: "high".into(),
                        message: "hardcoded API key".into(),
                        rule_id: "SEC-001".into(),
                        fix_hint: "use environment variable".into(),
                        evidence: None,
                        suggested_fix: None,
                        controls: None,
                    },
                    Finding {
                        file: "src/config.rs".into(),
                        line: Some(15),
                        column: None,
                        severity: "medium".into(),
                        message: "hardcoded password".into(),
                        rule_id: "SEC-002".into(),
                        fix_hint: "use vault".into(),
                        evidence: None,
                        suggested_fix: None,
                        controls: None,
                    },
                ],
            },
            CheckResult {
                name: "style".into(),
                passed: true,
                score: None,
                threshold: None,
                message: "style check ok".into(),
                details: serde_json::Value::Null,
                severity: None,
                help: None,
                rule_id: None,
                findings: vec![],
            },
        ],
        summary: CheckSummary {
            total_checks: 2,
            passed_checks: 1,
            failed_checks: 1,
            functions_analyzed: 0,
            avg_complexity: 0.0,
            avg_crap: 0.0,
        },
        file_summary: vec![],
    }
}

// ═══════════════════════════════════════════
// format_json
// ═══════════════════════════════════════════

#[test]
fn test_format_json_with_findings() {
    let json = crate::formatters::format_json(&make_report());
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["passed"], serde_json::Value::Bool(false));
    assert_eq!(v["path"], "test/path");
    assert!(v["checks"].is_array());
    assert_eq!(v["checks"][0]["name"], "secrets");
    assert_eq!(v["summary"]["total_checks"], 2);
    assert_eq!(v["summary"]["failed_checks"], 1);
}

#[test]
fn test_format_json_empty_report() {
    let report = CheckReport {
        passed: true,
        path: "empty".into(),
        checks: vec![],
        summary: CheckSummary {
            total_checks: 0, passed_checks: 0, failed_checks: 0,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let json = crate::formatters::format_json(&report);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["path"], "empty");
    assert!(v["checks"].as_array().unwrap().is_empty());
}

// ═══════════════════════════════════════════
// format_ndjson
// ═══════════════════════════════════════════

#[test]
fn test_format_ndjson_with_findings() {
    let output = crate::formatters::format_ndjson(&make_report());
    assert!(!output.is_empty());
    let lines: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
    // make_report has 1 failing check with items, so should produce NDJSON lines per item
    assert!(!lines.is_empty(), "should produce at least one NDJSON line");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.get("tool").is_some(), "each NDJSON line must have 'tool'");
        assert!(v.get("severity").is_some());
        assert!(v.get("rule_id").is_some());
    }
}

#[test]
fn test_format_ndjson_passed_check_empty() {
    let report = CheckReport {
        passed: true,
        path: ".".into(),
        checks: vec![CheckResult {
            name: "ok".into(), passed: true, score: None, threshold: None,
            message: "all good".into(), details: serde_json::Value::Null,
            severity: None, help: None, rule_id: None, findings: vec![],
        }],
        summary: CheckSummary {
            total_checks: 1, passed_checks: 1, failed_checks: 0,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let output = crate::formatters::format_ndjson(&report);
    assert!(output.is_empty(), "passed checks should produce no NDJSON");
}

// ═══════════════════════════════════════════
// build_sarif_log / format_sarif_log
// ═══════════════════════════════════════════

#[test]
fn test_build_sarif_log_with_findings() {
    let log = crate::formatters::build_sarif_log(&make_report());
    assert_eq!(log.version, "2.1.0");
    assert!(!log.runs.is_empty());
    assert_eq!(log.runs[0].tool.driver.name, "cogent");
    // Two distinct rule_ids: SEC-001, SEC-002
    assert_eq!(log.runs[0].tool.driver.rules.len(), 2);
    // Two results (one per finding)
    assert_eq!(log.runs[0].results.len(), 2);
}

#[test]
fn test_build_sarif_log_levels() {
    let log = crate::formatters::build_sarif_log(&make_report());
    let results = &log.runs[0].results;
    // Finding 0: severity="high" -> level="error"
    assert_eq!(results[0].level, "error");
    // Finding 1: severity="medium" -> level="warning"
    assert_eq!(results[1].level, "warning");
}

#[test]
fn test_format_sarif_log_serializes() {
    let log = crate::formatters::build_sarif_log(&make_report());
    let json = crate::formatters::format_sarif_log(&log);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["version"], "2.1.0");
    assert_eq!(v["runs"][0]["tool"]["driver"]["name"], "cogent");
    // Our two rule IDs should appear in the results
    let results = v["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r["rule_id"] == "SEC-001"));
    assert!(results.iter().any(|r| r["rule_id"] == "SEC-002"));
}

#[test]
fn test_build_sarif_log_empty_findings() {
    let report = CheckReport {
        passed: true,
        path: ".".into(),
        checks: vec![CheckResult {
            name: "ok".into(), passed: true, score: None, threshold: None,
            message: "ok".into(), details: serde_json::Value::Null,
            severity: None, help: None, rule_id: None, findings: vec![],
        }],
        summary: CheckSummary {
            total_checks: 1, passed_checks: 1, failed_checks: 0,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let log = crate::formatters::build_sarif_log(&report);
    // No findings -> no results, no rules
    assert!(log.runs[0].results.is_empty());
    assert!(log.runs[0].tool.driver.rules.is_empty());
}

// ═══════════════════════════════════════════
// format_junit
// ═══════════════════════════════════════════

#[test]
fn test_format_junit_with_findings() {
    let xml = crate::formatters::format_junit(&make_report());
    assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<testsuites"));
    assert!(xml.contains("</testsuites>"));
    assert!(xml.contains("failures=\"1\""));
    assert!(xml.contains("tests=\"2\""));
    assert!(xml.contains("<failure"));
    assert!(xml.contains("SEC-001"));
    assert!(xml.contains("SEC-002"));
    assert!(xml.contains("hardcoded API key"));
    assert!(xml.contains("hardcoded password"));
    assert!(xml.contains("src/main.rs"));
    assert!(xml.contains("src/config.rs"));
}

#[test]
fn test_format_junit_all_passed() {
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
    let xml = crate::formatters::format_junit(&report);
    assert!(xml.contains("failures=\"0\""));
    assert!(!xml.contains("<failure"));
    assert!(xml.contains("<testcase name=\"crap\""));
}

// ═══════════════════════════════════════════
// format_findings_ndjson
// ═══════════════════════════════════════════

#[test]
fn test_format_findings_ndjson_with_findings() {
    let output = crate::formatters::format_findings_ndjson(&make_report());
    let lines: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "should have one NDJSON line per finding");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.get("rule_id").is_some());
        assert!(v.get("message").is_some());
    }
}

#[test]
fn test_format_findings_ndjson_empty() {
    let report = CheckReport {
        passed: true,
        path: ".".into(),
        checks: vec![CheckResult {
            name: "ok".into(), passed: true, score: None, threshold: None,
            message: "ok".into(), details: serde_json::Value::Null,
            severity: None, help: None, rule_id: None, findings: vec![],
        }],
        summary: CheckSummary {
            total_checks: 1, passed_checks: 1, failed_checks: 0,
            functions_analyzed: 0, avg_complexity: 0.0, avg_crap: 0.0,
        },
        file_summary: vec![],
    };
    let output = crate::formatters::format_findings_ndjson(&report);
    assert!(output.is_empty());
}
