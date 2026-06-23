//! Output formatters (JSON, NDJSON, SARIF, JUnit, Markdown).
//! Extracted from cogent-cli/src/main.rs.

#![deny(clippy::all)]

use std::time::Duration;

use serde::Serialize;

use crate::html_escape;
use cogent_common::{
    CheckReport, CheckResult, SarifArtifactLocation, SarifDriver, SarifInvocation, SarifLocation,
    SarifLog, SarifMessage, SarifPhysicalLocation, SarifRegion, SarifResult, SarifRule,
    SarifRuleConfig, SarifRun, SarifTool,
};
use tracing::info;

// Re-export for tests
#[derive(Serialize)]
pub struct CheckReportRef<'a> {
    pub checks: &'a [CheckResult],
    pub passed: bool,
    pub path: String,
}

pub fn format_elapsed(d: Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        format!("{:.1}s", total_ms as f64 / 1000.0)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

pub fn format_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn box_row(content: &str, inner_width: usize) {
    let vlen = visible_len(content);
    let padding = inner_width.saturating_sub(vlen);
    eprintln!("  ║  {}{}║", content, " ".repeat(padding));
}

/// Format a CheckReport as pretty-printed JSON.
pub fn format_json(report: &CheckReport) -> String {
    serde_json::to_string_pretty(&report).unwrap()
}

pub fn output_json(report: &CheckReport) {
    info!(checks = report.checks.len(), path = %report.path, "formatting report as JSON");
    println!("{}", format_json(report));
}

/// Format non-passing checks as NDJSON lines. Returns one line per finding/item.
pub fn format_ndjson(report: &CheckReport) -> String {
    let mut lines = Vec::new();
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
                lines.push(
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
                    .to_string(),
                );
            } else {
                for item in &items {
                    lines.push(serde_json::json!({
                        "tool": check.name,
                        "severity": severity,
                        "rule_id": rule_id,
                        "message": item.get("type").and_then(|v| v.as_str()).unwrap_or(&check.name),
                        "help": help,
                        "file": item.get("file"),
                        "line": item.get("line"),
                        "col": null,
                    }).to_string());
                }
            }
        }
    }
    lines.join("\n")
}

pub fn output_ndjson(report: &CheckReport) {
    info!(checks = report.checks.len(), "formatting report as NDJSON");
    let output = format_ndjson(report);
    if !output.is_empty() {
        println!("{}", output);
    }
}

/// Build a SarifLog from a CheckReport. Timestamps are set to the current time.
pub fn build_sarif_log(report: &CheckReport) -> SarifLog {
    let mut all_results: Vec<SarifResult> = Vec::new();
    let mut all_rules: Vec<SarifRule> = Vec::new();
    let mut rule_indices: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for check in &report.checks {
        for finding in &check.findings {
            let level = sarif_level(&finding.severity);
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
            end_time_utc: Some(
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
        }]),
        results: all_results,
    };

    SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![run],
    }
}

/// Serialize a SarifLog to a pretty-printed JSON string.
pub fn format_sarif_log(log: &SarifLog) -> String {
    serde_json::to_string_pretty(&log).unwrap()
}

pub fn output_sarif(report: &CheckReport) {
    info!(checks = report.checks.len(), "formatting report as SARIF");
    let log = build_sarif_log(report);
    println!("{}", format_sarif_log(&log));
}

/// Format a CheckReport as JUnit XML string.
pub fn format_junit(report: &CheckReport) -> String {
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

pub fn output_junit(report: &CheckReport) {
    info!(
        checks = report.checks.len(),
        "formatting report as JUnit XML"
    );
    println!("{}", format_junit(report));
}

/// Format findings as NDJSON lines. Returns one JSON line per finding.
pub fn format_findings_ndjson(report: &CheckReport) -> String {
    let mut lines = Vec::new();
    for check in &report.checks {
        for finding in &check.findings {
            lines.push(serde_json::to_string(&finding).unwrap());
        }
    }
    lines.join("\n")
}

pub fn output_findings_ndjson(report: &CheckReport) {
    info!(
        checks = report.checks.len(),
        "formatting findings as NDJSON"
    );
    let output = format_findings_ndjson(report);
    if !output.is_empty() {
        println!("{}", output);
    }
}

pub fn format_ts(ts: u64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    let month = (days % 365) / 30 + 1;
    let day = (days % 365) % 30 + 1;
    let h = (ts % 86400) / 3600;
    let m = (ts % 3600) / 60;
    format!("{}-{:02}-{:02} {:02}:{:02}", year, month, day, h, m)
}

pub(crate) fn sarif_level(severity: &str) -> &'static str {
    match severity.to_lowercase().as_str() {
        "critical" | "high" | "error" => "error",
        "medium" | "warning" => "warning",
        "low" | "note" => "note",
        _ => "warning",
    }
}

// ---------------------------------------------------------------------------
// Helpers (present in the original formatter bloc)
// ---------------------------------------------------------------------------

pub(crate) fn visible_len(s: &str) -> usize {
    let plain = strip_ansi(s);
    plain.chars().count()
}

pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[expect(dead_code)]
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'&' => out.push('&'),
            b'<' => out.push('<'),
            b'>' => out.push('>'),
            b'"' => out.push_str(""),
            b'\'' => out.push('\''),
            _ => out.push(b as char),
        }
    }
    out
}
