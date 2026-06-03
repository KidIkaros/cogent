//! Output formatters for local CheckReport: ndjson, sarif, junit, markdown.

#![deny(clippy::all)]

use crate::types::{CheckReport, CheckResult};
use crate::progress::health_score;
use crate::report::render_markdown_report;
use cogent_common::{
    SarifArtifactLocation, SarifDriver, SarifInvocation, SarifLocation, SarifLog, SarifMessage,
    SarifPhysicalLocation, SarifRegion, SarifResult, SarifRule, SarifRuleConfig, SarifRun,
    SarifTool,
};

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// JSON OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_json(report: &CheckReport) {
    println!("{}", serde_json::to_string_pretty(report).unwrap());
}

// NDJSON OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_ndjson(report: &CheckReport) {
    for check in &report.checks {
        let severity = check.severity.as_deref().unwrap_or("warning");
        let rule_id = check.rule_id.as_deref().unwrap_or(&check.name);
        let help = check.help.as_deref().unwrap_or("");
        if !check.passed {
            let items = check
                .details
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if items.is_empty() {
                println!(
                    "{}",
                    serde_json::json!({
                        "tool": check.name,
                        "severity": severity,
                        "rule_id": rule_id,
                        "message": check.message,
                        "help": help,
                        "file": report.path,
                        "line": null,
                        "col": null,
                    })
                );
            } else {
                for item in &items {
                    println!(
                        "{}",
                        serde_json::json!({
                            "tool": check.name,
                            "severity": severity,
                            "rule_id": rule_id,
                            "message": item.get("type").and_then(|v| v.as_str()).unwrap_or(&check.name),
                            "help": help,
                            "file": item.get("file"),
                            "line": item.get("line"),
                            "col": null,
                        })
                    );
                }
            }
        }
    }
}

// ═══════════════════════════════════════════
// SARIF OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_sarif(report: &CheckReport) {
    let mut all_results: Vec<SarifResult> = Vec::new();
    let mut all_rules: Vec<SarifRule> = Vec::new();
    let mut rule_indices: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for check in &report.checks {
        for finding in &check.findings {
            let level = cogent_common::sarif_level(&finding.severity);
            let rule_id = finding.rule_id.clone();
            let idx = *rule_indices.entry(rule_id.clone()).or_insert_with(|| {
                let i = all_rules.len();
                all_rules.push(SarifRule {
                    id: rule_id.clone(),
                    name: Some(rule_id.clone()),
                    short_description: Some(SarifMessage {
                        text: finding.message.clone(),
                    }),
                    full_description: Some(SarifMessage {
                        text: finding.fix_hint.clone(),
                    }),
                    help: Some(SarifMessage {
                        text: finding.fix_hint.clone(),
                    }),
                    default_configuration: Some(SarifRuleConfig {
                        level: level.to_string(),
                    }),
                });
                i
            });
            let location = if !finding.file.is_empty() {
                vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: Some(SarifArtifactLocation {
                            uri: finding.file.clone(),
                        }),
                        region: finding.line.map(|l| SarifRegion {
                            start_line: Some(l as usize),
                            start_column: finding.column.map(|c| c as usize),
                            end_line: None,
                            end_column: None,
                        }),
                    },
                }]
            } else {
                Vec::new()
            };
            all_results.push(SarifResult {
                rule_id: finding.rule_id.clone(),
                rule_index: Some(idx),
                level: level.to_string(),
                message: SarifMessage {
                    text: finding.message.clone(),
                },
                locations: location,
            });
        }
    }

    let run = SarifRun {
        tool: SarifTool {
            driver: SarifDriver {
                name: "cogent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                rules: all_rules,
            },
        },
        invocations: Some(vec![SarifInvocation {
            execution_successful: report.passed,
            exit_code: Some(if report.passed { 0 } else { 1 }),
            end_time_utc: Some(chrono::Utc::now().to_rfc3339()),
        }]),
        results: all_results,
    };

    let log = SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![run],
    };
    println!("{}", serde_json::to_string_pretty(&log).unwrap());
}

// ═══════════════════════════════════════════
// JUNIT XML OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_junit(report: &CheckReport) {
    println!("{}", format_junit_xml(report));
}

/// Build JUnit XML string from a CheckReport.
pub(crate) fn format_junit_xml(report: &CheckReport) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        r#"<testsuites name="Cogent" tests="{}" failures="{}" time="0">"#,
        report.summary.total_checks, report.summary.failed_checks
    ));
    xml.push('\n');

    for check in &report.checks {
        let test_count = check.findings.len().max(1);
        let failure_count = if check.passed { 0 } else { test_count };
        xml.push_str(&format!(
            r#"  <testsuite name="{}" tests="{}" failures="{}" errors="0" skipped="0">"#,
            html_escape(&check.name),
            test_count,
            failure_count
        ));
        xml.push('\n');

        if check.findings.is_empty() {
            xml.push_str(&format!(
                r#"    <testcase name="{}" classname="cogent.{}" />"#,
                html_escape(&check.name),
                html_escape(&check.name)
            ));
            xml.push('\n');
        } else {
            for finding in &check.findings {
                let case_name = html_escape(&finding.message);
                let classname = html_escape(&check.name);
                xml.push_str(&format!(
                    r#"    <testcase name="{}" classname="cogent.{}">"#,
                    case_name, classname
                ));
                xml.push('\n');
                if !check.passed {
                    xml.push_str(&format!(
                        r#"      <failure message="{}" type="{}">{} at {}:{}</failure>"#,
                        html_escape(&finding.message),
                        html_escape(&finding.rule_id),
                        html_escape(&finding.message),
                        html_escape(&finding.file),
                        finding.line.unwrap_or(0)
                    ));
                    xml.push('\n');
                }
                xml.push_str("    </testcase>");
                xml.push('\n');
            }
        }
        xml.push_str("  </testsuite>");
        xml.push('\n');
    }
    xml.push_str("</testsuites>");
    xml
}

// ═══════════════════════════════════════════
// FINDINGS NDJSON OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_findings_ndjson(report: &CheckReport) {
    for check in &report.checks {
        for finding in &check.findings {
            println!("{}", serde_json::to_string(finding).unwrap());
        }
    }
}

// ═══════════════════════════════════════════
// MARKDOWN OUTPUT
// ═══════════════════════════════════════════

pub(crate) fn output_markdown(report: &CheckReport, path: &str) {
    output_markdown_with_framework(report, path, "")
}

pub(crate) fn output_markdown_with_framework(report: &CheckReport, path: &str, framework: &str) {
    let security_tools = [
        "taint",
        "secrets",
        "sast",
        "crypto",
        "vulnscan",
        "access-control",
    ];
    let quality_tools = [
        "crap",
        "debt",
        "doc_coverage",
        "complexity",
        "duplication",
        "riskmap",
        "coupling",
        "propcov",
        "fuzz",
        "linelen",
        "halstead",
        "deadcode",
        "cohesion",
        "comments",
        "errhandle",
        "typecov",
        "observability",
        "test-quality",
        "design-docs",
        "debuggability",
    ];
    let compliance_tools = ["licenses", "outdated", "supply-chain"];
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let md = render_markdown_report(
        report,
        path,
        &date,
        &security_tools,
        &quality_tools,
        &compliance_tools,
        framework,
    );
    println!("{}", md);
}

/// Generate a markdown snippet suitable for posting as a GitHub PR comment.
pub(crate) fn pr_comment_md(report: &CheckReport, path: &str) -> String {
    let (health, grade) = health_score(&report.checks);
    let overall = if report.passed {
        "✅ PASSED"
    } else {
        "❌ FAILED"
    };
    let mut md = format!(
        "## 🔍 Cogent Quality Check — {}\n\n**Status:** {}  \n**Health Score:** {}/100 (Grade {})  \n**Path:** {}\n\n",
        path, overall, health, grade, path
    );

    // Summary table
    md.push_str("| Metric | Value |\n|---|---|\n");
    md.push_str(&format!(
        "| Total Checks | {} |\n",
        report.summary.total_checks
    ));
    md.push_str(&format!("| Passed | {} |\n", report.summary.passed_checks));
    md.push_str(&format!("| Failed | {} |\n", report.summary.failed_checks));
    md.push('\n');

    // Failed checks collapsible sections
    let failed: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();
    if !failed.is_empty() {
        md.push_str("### ⚠️ Failed Checks\n\n");
        for check in failed {
            md.push_str(&format!(
                "<details>\n<summary><code>{}</code> — {}</summary>\n\n",
                check.name, check.message
            ));
            md.push_str("| File | Line | Message | Rule |\n|---|---|---|---|\n");
            for finding in &check.findings {
                let line = finding
                    .line
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!(
                    "| `{}` | {} | {} | `{}` |\n",
                    finding.file, line, finding.message, finding.rule_id
                ));
            }
            md.push_str("</details>\n\n");
        }
    }

    // File heatmap (top 10)
    if !report.file_summary.is_empty() {
        md.push_str("### 🔥 Top Files by Issue Count\n\n");
        md.push_str("| File | Issues | Severity Score |\n|---|---|---|\n");
        for fs in report.file_summary.iter().take(10) {
            md.push_str(&format!(
                "| `{}` | {} | {} |\n",
                fs.file, fs.issue_count, fs.severity_score
            ));
        }
        md.push('\n');
    }

    md.push_str("---\n\n*Generated by Cogent — automated code quality & security auditing.*\n");
    md
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CheckReport, CheckSummary, CheckResult, Finding, FileSummary};

    // Helper: build a minimal CheckReport for testing
    fn make_report(checks: Vec<CheckResult>) -> CheckReport {
        let total = checks.len();
        let passed = checks.iter().filter(|c| c.passed).count();
        CheckReport {
            passed: passed == total,
            path: "/test".to_string(),
            checks,
            summary: CheckSummary {
                total_checks: total,
                passed_checks: passed,
                failed_checks: total - passed,
                functions_analyzed: 10,
                avg_complexity: 5.0,
                avg_crap: 10.0,
            },
            file_summary: vec![],
        }
    }

    fn make_check(name: &str, passed: bool, findings: Vec<Finding>) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            passed,
            score: None,
            threshold: None,
            message: format!("{} check", name),
            details: serde_json::json!({}),
            severity: Some(if passed { "info" } else { "high" }.into()),
            help: Some("Fix it".into()),
            rule_id: Some(format!("rule-{}", name)),
            findings,
        }
    }

    fn make_finding(file: &str, msg: &str, severity: &str, rule_id: &str, line: Option<u64>) -> Finding {
        Finding {
            file: file.to_string(),
            line,
            column: None,
            severity: severity.to_string(),
            message: msg.to_string(),
            rule_id: rule_id.to_string(),
            fix_hint: "".to_string(),
            evidence: None,
            suggested_fix: None,
            controls: None,
        }
    }

    // ── html_escape ──

    #[test]
    fn test_html_escape_no_special_chars() {
        assert_eq!(html_escape("hello world"), "hello world");
    }

    #[test]
    fn test_html_escape_ampersand() {
        assert_eq!(html_escape("AT&T"), "AT&amp;T");
    }

    #[test]
    fn test_html_escape_angle_brackets() {
        assert_eq!(html_escape("a < b > c"), "a &lt; b &gt; c");
    }

    #[test]
    fn test_html_escape_quotes() {
        assert_eq!(html_escape("she said \"hi\""), "she said &quot;hi&quot;");
    }

    #[test]
    fn test_html_escape_all() {
        assert_eq!(
            html_escape("<div class=\"test\">AT&T</div>"),
            "&lt;div class=&quot;test&quot;&gt;AT&amp;T&lt;/div&gt;"
        );
    }

    #[test]
    fn test_html_escape_empty() {
        assert_eq!(html_escape(""), "");
    }

    // ── format_junit_xml ──

    #[test]
    fn test_junit_xml_structure() {
        let report = make_report(vec![]);
        let xml = format_junit_xml(&report);
        assert!(xml.starts_with(r#"<?xml"#), "should have XML declaration");
        assert!(xml.contains("<testsuites"), "should have testsuites root");
        assert!(xml.contains("</testsuites>"), "should close testsuites");
    }

    #[test]
    fn test_junit_xml_all_passed() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
            make_check("debt", true, vec![]),
        ]);
        let xml = format_junit_xml(&report);
        assert!(xml.contains("tests=\"2\""), "should have 2 tests");
        assert!(xml.contains("failures=\"0\""), "should have 0 failures");
        assert!(xml.contains("<testcase"), "should have testcase elements");
        // No failure elements since all passed
        assert!(!xml.contains("<failure"), "no failures when passed");
    }

    #[test]
    fn test_junit_xml_with_findings_all_failed() {
        let findings = vec![
            make_finding("main.rs", "too complex", "high", "complexity-high", Some(42)),
            make_finding("lib.rs", "no docs", "medium", "doccov-low", Some(10)),
        ];
        let report = make_report(vec![
            make_check("complexity", false, findings),
        ]);
        let xml = format_junit_xml(&report);
        assert!(xml.contains("tests=\"2\""), "findings count = test count");
        assert!(xml.contains("failures=\"2\""), "all failed = 2 failures");
        assert!(xml.contains("<failure"), "should have failure elements");
        assert!(xml.contains("too complex"), "should include finding message");
        assert!(xml.contains("no docs"), "should include other finding");
        assert!(xml.contains("complexity-high"), "should include rule_id");
        assert!(xml.contains("main.rs"), "should include file path");
        assert!(xml.contains("lib.rs"), "should include other file");
    }

    #[test]
    fn test_junit_xml_mixed_pass_fail() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
            make_check("complexity", false, vec![
                make_finding("main.rs", "too complex", "high", "complexity", Some(42)),
            ]),
        ]);
        let xml = format_junit_xml(&report);
        assert!(xml.contains("tests=\"2\""), "2 checks");
        assert!(xml.contains("failures=\"1\""), "1 failure");
        // Two testsuite elements (exclude the root <testsuites> tag)
        assert_eq!(xml.matches("name=\"\"").count(), 0, "should find no empty name");
    }

    #[test]
    fn test_junit_xml_special_chars_escaped() {
        let report = make_report(vec![
            make_check("AT&T", false, vec![
                make_finding("<script>", "xss risk", "critical", "sast-xss", None),
            ]),
        ]);
        let xml = format_junit_xml(&report);
        // Special chars should be HTML-escaped
        assert!(xml.contains("AT&amp;T"), "ampersand in name should be escaped");
        assert!(xml.contains("&lt;script&gt;"), "angle brackets should be escaped");
        assert!(!xml.contains("<script>"), "raw script tag should not appear");
    }

    #[test]
    fn test_junit_xml_no_findings_shows_testcase() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
        ]);
        let xml = format_junit_xml(&report);
        // Empty findings → one testcase with just the check name
        assert!(xml.contains("testcase name=\"crap\""), "should have testcase for check");
        assert!(xml.contains("/>"), "self-closing testcase when no findings");
    }

    // ── pr_comment_md ──

    #[test]
    fn test_pr_comment_md_passed() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(md.contains("PASSED"), "passed check should show PASSED");
        assert!(md.contains("/repo"), "should include path");
        assert!(md.contains("Total Checks"), "should have summary table");
        assert!(md.contains("1"), "should have count");
    }

    #[test]
    fn test_pr_comment_md_failed() {
        let report = make_report(vec![
            make_check("complexity", false, vec![
                make_finding("main.rs", "too complex", "high", "complexity", Some(42)),
            ]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(md.contains("FAILED"), "failed check should show FAILED");
        assert!(md.contains("Failed Checks"), "should show failed section");
        assert!(md.contains("<details>"), "should have collapsible section");
        assert!(md.contains("too complex"), "should include finding message");
        assert!(md.contains("main.rs"), "should include file");
        assert!(md.contains("42"), "should include line number");
        assert!(md.contains("Cogent"), "should have Cogent footer");
    }

    #[test]
    fn test_pr_comment_md_no_failed_section_when_all_pass() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(!md.contains("Failed Checks"), "no Failed Checks section when all pass");
        assert!(!md.contains("<details>"), "no collapsible sections when all pass");
    }

    #[test]
    fn test_pr_comment_md_file_summary() {
        let mut report = make_report(vec![
            make_check("crap", false, vec![
                make_finding("main.rs", "too complex", "high", "complexity", Some(42)),
                make_finding("lib.rs", "no docs", "medium", "doccov", Some(10)),
            ]),
        ]);
        report.file_summary = vec![
            FileSummary {
                file: "main.rs".to_string(),
                issue_count: 1,
                severity_score: 3,
                findings_by_severity: std::collections::HashMap::new(),
            },
            FileSummary {
                file: "lib.rs".to_string(),
                issue_count: 1,
                severity_score: 2,
                findings_by_severity: std::collections::HashMap::new(),
            },
        ];
        let md = pr_comment_md(&report, "/repo");
        assert!(md.contains("Top Files"), "should show file heatmap");
        assert!(md.contains("| `main.rs` | 1 | 3 |"), "should list main.rs");
        assert!(md.contains("| `lib.rs` | 1 | 2 |"), "should list lib.rs");
    }

    #[test]
    fn test_pr_comment_md_no_file_summary_when_empty() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(!md.contains("Top Files"), "no file heatmap when empty");
    }

    #[test]
    fn test_pr_comment_md_line_none_defaults_to_dash() {
        let report = make_report(vec![
            make_check("crap", false, vec![
                make_finding("main.rs", "global issue", "medium", "some-rule", None),
            ]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(md.contains("| `main.rs` | - |"), "no line number → shows dash");
    }

    #[test]
    fn test_pr_comment_md_health_score() {
        let report = make_report(vec![
            make_check("crap", true, vec![]),
            make_check("debt", true, vec![]),
        ]);
        let md = pr_comment_md(&report, "/repo");
        assert!(md.contains("Health Score"), "should show health score");
        assert!(md.contains("Grade"), "should show grade");
    }
}
