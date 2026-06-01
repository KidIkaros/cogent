//! Output formatters for Cogent reports: JSON, SARIF, JUnit, NDJSON, Markdown.

#![deny(clippy::all)]

use cogent_common::{
    CheckReport, CheckResult, health_score,
    SarifArtifactLocation, SarifDriver, SarifInvocation, SarifLocation, SarifLog, SarifMessage,
    SarifPhysicalLocation, SarifRegion, SarifResult, SarifRule, SarifRuleConfig, SarifRun,
    SarifTool,
};

/// Write report as pretty-printed JSON to stdout.
pub fn output_json(report: &CheckReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize report to JSON");
        }
    }
}

/// Write each failing check as an NDJSON line to stdout.
pub fn output_ndjson(report: &CheckReport) {
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

/// Write report as SARIF JSON to stdout.
pub fn output_sarif(report: &CheckReport) {
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
    match serde_json::to_string_pretty(&log) {
        Ok(json) => println!("{}", json),
        Err(e) => tracing::error!(error = %e, "failed to serialize SARIF output"),
    }
}

/// Write report as JUnit XML to stdout.
pub fn output_junit(report: &CheckReport) {
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
    println!("{}", xml);
}

/// Write each finding as an NDJSON line to stdout.
pub fn output_findings_ndjson(report: &CheckReport) {
    for check in &report.checks {
        for finding in &check.findings {
            match serde_json::to_string(finding) {
                Ok(json) => println!("{}", json),
                Err(e) => tracing::error!(error = %e, "failed to serialize finding"),
            }
        }
    }
}

/// Write report as Markdown to stdout.
pub fn output_markdown(report: &CheckReport, path: &str) {
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
    ];
    let compliance_tools = ["licenses", "outdated", "supply-chain"];
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let md = cogent_report::html::render_markdown_report(
        report,
        path,
        &date,
        &security_tools,
        &quality_tools,
        &compliance_tools,
    );
    println!("{}", md);
}

/// Generate a markdown snippet suitable for posting as a GitHub PR comment.
pub fn pr_comment_md(report: &CheckReport, path: &str) -> String {
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

    md.push_str("| Metric | Value |\n|---|---|\n");
    md.push_str(&format!(
        "| Total Checks | {} |\n",
        report.summary.total_checks
    ));
    md.push_str(&format!("| Passed | {} |\n", report.summary.passed_checks));
    md.push_str(&format!("| Failed | {} |\n", report.summary.failed_checks));
    md.push('\n');

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

/// Minimal HTML escape for JUnit XML output.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
