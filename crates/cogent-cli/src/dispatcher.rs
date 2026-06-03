//! Command dispatcher: maps `Commands` variants to their handler logic.
//! Extracted from `main.rs` to give each concern a single home.

#![deny(clippy::all)]

use std::time::Instant;

use colored::Colorize;

use crate::audit;
use crate::check_runners::*;
use crate::cli::{Commands, ExceptionAction, PolicyAction, hqse_phase_for};
use crate::config::{detect_project, generate_config, load_config_thresholds};
use crate::diff::diff_command;
use crate::history::history_command;
use crate::hooks::{install_hooks, uninstall_hooks};
use crate::progress::{
    format_elapsed, health_score, print_fix_summary,
    print_offenders, print_severity_grouped, print_summary_box, run_with_spinner,
};
use crate::report::{render_html_report, report_command, setup_command};
use crate::report_formatters::*;
use crate::serve::serve_command;
use crate::types::{
    aggregate_file_summary, CheckReport, CheckResult, CheckSummary,
    Finding, SuggestedFix,
};
use crate::watch::watch_mode;
use crate::commands::{discover_command, explain_command, init_ci};
use crate::doctor::doctor_command;

/// Resolve the output format: CI mode forces JSON regardless of the specified format.
fn ci_format(format: String, ci: bool) -> String {
    if ci { "json".to_string() } else { format }
}

/// Run the `cogent audit` command: orchestrates security/quality/compliance checks,
/// enriches findings with evidence/controls/suggested-fixes, and outputs in the
/// requested format (agent, json, or markdown).
fn run_audit_subcommand(path: String, format: String, cfg: AuditCommandConfig) -> i32 {
    let AuditCommandConfig {
        only,
        evidence,
        framework,
        checks,
        skip,
        ci,
        verify,
    } = cfg;

    let format = ci_format(format, ci);

    if ci {
        std::env::set_var("COGENT_NO_PROGRESS", "1");
    }

    let audit_start = Instant::now();

    let security_checks = [
        "secrets", "sast", "crypto", "taint", "vulnscan", "access-control",
    ];
    let quality_checks = [
        "crap", "debt", "doccov", "complexity", "dupfind", "riskmap",
        "coupling", "propcov", "fuzz", "linelen", "halstead", "deadcode",
        "cohesion", "comments", "errhandle", "typecov", "observability",
        "test-quality", "design-docs", "debuggability",
    ];
    let compliance_checks = ["licenses", "supply-chain", "outdated", "sbom"];

    let active_categories: Vec<&str> = match only.as_deref() {
        Some("security") => security_checks.to_vec(),
        Some("quality") => quality_checks.to_vec(),
        Some("compliance") => compliance_checks.to_vec(),
        _ => security_checks
            .iter()
            .chain(quality_checks.iter())
            .chain(compliance_checks.iter())
            .copied()
            .collect(),
    };

    let skip_set: std::collections::HashSet<String> = parse_comma_list(&skip).into_iter().collect();
    let only_set: std::collections::HashSet<String> = parse_comma_list(&checks).into_iter().collect();

    let should_run_audit = |name: &str| -> bool {
        audit_should_run(name, &only_set, &skip_set, &active_categories)
    };

    let mut checks_run: Vec<CheckResult> = Vec::new();

    macro_rules! run_audit_check {
        ($name:expr, $expr:expr) => {
            if should_run_audit($name) {
                checks_run.push(run_with_spinner($name, || $expr));
            }
        };
    }

    run_audit_check!("secrets", check_secrets(&path, true, 0));
    run_audit_check!("sast", check_sast(&path, true, 0));
    run_audit_check!("crypto", check_crypto(&path, true, 0));
    run_audit_check!("taint", check_taint(&path, true, 0));
    run_audit_check!("vulnscan", check_vulnscan(&path, 0, 0));
    run_audit_check!("access-control", check_access_control(&path, true, 0));
    run_audit_check!("licenses", check_licenses(&path, 0));
    run_audit_check!("supply-chain", check_supply_chain(&path, 0));
    run_audit_check!("crap", check_crap(&path, true, &None, 30.0));
    run_audit_check!("debt", check_debt(&path, true, usize::MAX));
    run_audit_check!("doccov", check_doc_coverage(&path, true, 0.0));
    run_audit_check!(
        "complexity",
        check_complexity(&path, true, 10u32, usize::MAX)
    );
    run_audit_check!("dupfind", check_dupfind(&path, true, 100.0));
    run_audit_check!("riskmap", check_riskmap(&path, true, 100.0));
    run_audit_check!("coupling", check_coupling(&path, usize::MAX));
    run_audit_check!("deadcode", check_deadcode(&path, true, usize::MAX));
    run_audit_check!("cohesion", check_cohesion(&path, true, usize::MAX));
    run_audit_check!("comments", check_comments(&path, true, 0.0));
    run_audit_check!("errhandle", check_errhandle(&path, true, usize::MAX));
    run_audit_check!("halstead", check_halstead(&path, true, 100.0));
    run_audit_check!("linelen", check_linelen(&path, true, usize::MAX));
    run_audit_check!("observability", check_observability(&path, true, usize::MAX));
    run_audit_check!("test-quality", check_test_quality(&path, true, usize::MAX));
    run_audit_check!("design-docs", check_design_docs(&path));
    run_audit_check!("debuggability", check_debuggability(&path, true, usize::MAX));

    let meta = prepare_audit_report_meta(
        checks_run,
        framework,
        evidence,
        verify,
        ci,
        path,
        audit_start,
    );

    run_audit_report_format(&format, &meta)
}

/// Run a check function with an optional spinner for text format.
fn run_spinner_or(label: &str, format: &str, f: impl FnOnce() -> CheckResult) -> CheckResult {
    if format == "text" {
        run_with_spinner(label, f)
    } else {
        f()
    }
}

/// Simple check command display: just the message for text, JSON for other formats.
fn run_check_command(result: CheckResult, format: &str) -> i32 {
    let passed = result.passed;
    match format {
        "text" => println!("{}", result.message),
        _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
    }
    if passed { 0 } else { 1 }
}

/// Full check command display: icon + message on stderr, message on stdout, JSON for other formats.
fn run_check_command_full(label: &str, result: CheckResult, format: &str) -> i32 {
    let passed = result.passed;
    match format {
        "text" => {
            let icon = if passed { "✓".green().bold() } else { "✗".red().bold() };
            eprintln!("  {} {}  {}", icon, label, result.message.bright_black());
            println!("{}", result.message);
        }
        _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
    }
    if passed { 0 } else { 1 }
}

/// Parse a comma-separated list from an `Option<String>` into a `Vec<String>`.
/// Empty items, whitespace, and case are normalized: each item is trimmed, lowercased,
/// and empty strings are filtered out. Returns an empty vec when `input` is `None`.
fn parse_comma_list(input: &Option<String>) -> Vec<String> {
    input
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Computed metadata for formatting audit report output.
/// Created after all checks have run and findings have been enriched
/// so the format function is a pure reader.
struct AuditReportMeta {
    all_findings: Vec<Finding>,
    checks_run: Vec<CheckResult>,
    path: String,
    passed: bool,
    health: u32,
    grade: char,
    total_findings: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    duration: std::time::Duration,
    newly_recorded: usize,
    auto_closed: usize,
    use_framework: String,
    ci: bool,
}

/// Enrich findings with suggested fixes/controls/evidence, compute severity counts
/// and health score, record findings to the audit trail, and return the structured
/// metadata needed for report format output.
fn prepare_audit_report_meta(
    checks_run: Vec<CheckResult>,
    framework: Option<String>,
    evidence: bool,
    verify: bool,
    ci: bool,
    path: String,
    audit_start: Instant,
) -> AuditReportMeta {
    let mut all_findings: Vec<Finding> =
        checks_run.iter().flat_map(|c| c.findings.clone()).collect();

    let use_framework = framework.as_deref().unwrap_or("").to_string();
    for f in &mut all_findings {
        if let Some((desc, diff, conf)) = audit::suggested_fix_for(&f.rule_id) {
            f.suggested_fix = Some(SuggestedFix {
                description: desc,
                diff,
                confidence: conf,
            });
        }
        let ctrl = audit::controls_for(&f.rule_id);
        if !ctrl.is_empty() {
            f.controls = Some(ctrl);
        }
        if evidence {
            audit::enrich_finding(f);
        }
    }

    let total_findings = all_findings.len();
    let critical = all_findings.iter().filter(|f| f.severity == "critical").count();
    let high = all_findings.iter().filter(|f| f.severity == "high").count();
    let medium = all_findings.iter().filter(|f| f.severity == "medium").count();
    let low = all_findings.iter().filter(|f| f.severity == "low").count();
    let passed = checks_run.iter().all(|c| c.passed);
    let (health, grade) = health_score(&checks_run);
    let duration = audit_start.elapsed();

    let newly_recorded = audit::record_findings(&all_findings).len();
    let auto_closed = if verify {
        let closed = audit::verify_remediation(&all_findings);
        if !closed.is_empty() && !ci {
            for id in &closed {
                eprintln!("  \u{2713} auto-closed: {}", id);
            }
        }
        closed.len()
    } else {
        0
    };

    audit::append_audit_trail("audit", &path, total_findings, duration);

    AuditReportMeta {
        all_findings,
        checks_run,
        path,
        passed,
        health,
        grade,
        total_findings,
        critical,
        high,
        medium,
        low,
        duration,
        newly_recorded,
        auto_closed,
        use_framework,
        ci,
    }
}

/// Output an audit report in the requested format (agent, json, or markdown)
/// and return the exit code.
fn run_audit_report_format(format: &str, meta: &AuditReportMeta) -> i32 {
    match format {
        "agent" => {
            for f in &meta.all_findings {
                let suppressed = audit::is_suppressed(&f.rule_id, &f.file);
                let mut obj = serde_json::to_value(f).unwrap_or(serde_json::Value::Null);
                if let Some(o) = obj.as_object_mut() {
                    o.insert("type".to_string(), serde_json::json!("finding"));
                    o.insert("suppressed".to_string(), serde_json::json!(suppressed));
                    if meta.use_framework == "hqse" {
                        let phase = hqse_phase_for(&f.rule_id);
                        o.insert("hqse_phase".to_string(), serde_json::json!(phase));
                    }
                }
                println!("{}", serde_json::to_string(&obj).unwrap_or_default());
            }
            let controls_affected: Vec<String> = meta
                .all_findings
                .iter()
                .filter_map(|f| f.controls.as_ref())
                .flatten()
                .cloned()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let summary = serde_json::json!({
                "type": "summary",
                "passed": meta.passed,
                "score": meta.health,
                "grade": meta.grade.to_string(),
                "total_findings": meta.total_findings,
                "critical": meta.critical,
                "high": meta.high,
                "medium": meta.medium,
                "low": meta.low,
                "controls_affected": controls_affected,
                "checks_run": meta.checks_run.len(),
                "duration_ms": meta.duration.as_millis(),
                "path": meta.path,
                "framework": meta.use_framework,
                "newly_recorded": meta.newly_recorded,
                "auto_closed": meta.auto_closed,
            });
            println!("{}", serde_json::to_string(&summary).unwrap_or_default());
        }
        "json" => {
            let report = CheckReport {
                passed: meta.passed,
                path: meta.path.clone(),
                checks: meta.checks_run.clone(),
                summary: CheckSummary {
                    total_checks: meta.checks_run.len(),
                    passed_checks: meta.checks_run.iter().filter(|c| c.passed).count(),
                    failed_checks: meta.checks_run.iter().filter(|c| !c.passed).count(),
                    functions_analyzed: 0,
                    avg_complexity: 0.0,
                    avg_crap: 0.0,
                },
                file_summary: aggregate_file_summary(&meta.checks_run),
            };
            output_json(&report);
        }
        "markdown" => {
            let report = CheckReport {
                passed: meta.passed,
                path: meta.path.clone(),
                checks: meta.checks_run.clone(),
                summary: CheckSummary {
                    total_checks: meta.checks_run.len(),
                    passed_checks: meta.checks_run.iter().filter(|c| c.passed).count(),
                    failed_checks: meta.checks_run.iter().filter(|c| !c.passed).count(),
                    functions_analyzed: 0,
                    avg_complexity: 0.0,
                    avg_crap: 0.0,
                },
                file_summary: vec![],
            };
            output_markdown_with_framework(&report, &meta.path, &meta.use_framework);
        }
        _ => {
            eprintln!("Unknown format '{}'. Use: agent, json, markdown", format);
            std::process::exit(2);
        }
    }

    if meta.ci && meta.total_findings > 0 {
        1
    } else if meta.passed {
        0
    } else {
        1
    }
}

/// Determine whether a named check should run based on the `--only` and `--skip` lists.
/// If `only_list` is non-empty, the check runs only if its name appears in `only_list`.
/// Otherwise, the check runs unless its name appears in `skip_list`.
/// Determine whether an audit check should run based on `--only`, `--skip`, and the
/// active categories (security, quality, compliance).
///
/// `only_set` takes highest priority: if non-empty, only named checks in it run.
/// Next, `skip_set` excludes named checks.
/// Finally, `active_categories` acts as the default allow-list.
fn audit_should_run(name: &str, only_set: &std::collections::HashSet<String>, skip_set: &std::collections::HashSet<String>, active_categories: &[&str]) -> bool {
    if !only_set.is_empty() {
        return only_set.contains(name);
    }
    if skip_set.contains(name) {
        return false;
    }
    active_categories.contains(&name)
}

/// Determine whether a named check should run based on the `--only` and `--skip` lists.
/// If `only_list` is non-empty, the check runs only if its name appears in `only_list`.
/// Otherwise, the check runs unless its name appears in `skip_list`.
fn check_should_run(name: &str, only_list: &[String], skip_list: &[String]) -> bool {
    if !only_list.is_empty() {
        only_list.contains(&name.to_lowercase())
    } else {
        !skip_list.contains(&name.to_lowercase())
    }
}

/// Configuration for the `cogent check` command, mirroring the `Commands::Check` variant fields.
/// Configuration for the `cogent audit` command, mirroring `Commands::Audit` variant fields.
struct AuditCommandConfig {
    only: Option<String>,
    evidence: bool,
    framework: Option<String>,
    checks: Option<String>,
    skip: Option<String>,
    ci: bool,
    verify: bool,
}

/// Configuration for the `cogent check` command, mirroring the `Commands::Check` variant fields.
struct CheckCommandConfig {
    coverage: Option<String>,
    max_crap: f64,
    min_doc: f64,
    max_debt: usize,
    max_complexity_violations: usize,
    max_taint: usize,
    max_duplication: f64,
    max_risk: f64,
    max_coupling: usize,
    min_propcov: f64,
    max_fuzz_risk: usize,
    max_linelen: usize,
    max_halstead_bugs: f64,
    max_secrets: usize,
    max_deadcode: usize,
    max_cohesion: usize,
    min_comment_ratio: f64,
    max_errhandle: usize,
    min_typecov: f64,
    max_vuln_critical: usize,
    max_vuln_high: usize,
    max_sast: usize,
    max_crypto: usize,
    max_license_violations: usize,
    max_outdated: usize,
    skip: Option<String>,
    only: Option<String>,
    ci: bool,
    verbose: bool,
    pr_comment: bool,
    force: bool,
}

/// Run the `cogent check` command: orchestrates 20+ quality/security checks,
/// builds a `CheckReport`, and outputs in text, JSON, SARIF, JUnit, or markdown.
fn run_check_subcommand(path: String, recursive: bool, format: String, cfg: CheckCommandConfig) -> i32 {
    let format = ci_format(format, cfg.ci);
    if cfg.ci {
        std::env::set_var("COGENT_NO_PROGRESS", "1");
    }

    if !cfg.force && !std::path::Path::new(".quality.toml").exists() {
        eprintln!();
        eprintln!(
            "  {} No {} found.",
            "!".yellow().bold(),
            ".quality.toml".cyan()
        );
        eprintln!(
            "    {} Run {} to auto-detect your project and generate one.",
            "\u{2192}".cyan(),
            "cogent init".cyan().bold()
        );
        eprintln!();
        eprintln!(
            "    {} Use {} to run with hardcoded defaults anyway.",
            "\u{2192}".cyan(),
            "cogent check . --force".cyan().bold()
        );
        eprintln!();
        std::process::exit(2);
    }

    let coverage = cfg.coverage;
    let max_outdated = cfg.max_outdated;
    let skip = cfg.skip;
    let only = cfg.only;
    let verbose = cfg.verbose;
    let pr_comment = cfg.pr_comment;

    let (
        max_crap,
        min_doc,
        max_debt,
        max_complexity_violations,
        max_duplication,
        max_taint,
        max_risk,
        max_coupling,
        min_propcov,
        max_fuzz_risk,
        max_linelen,
        max_halstead_bugs,
        max_secrets,
        max_deadcode,
        max_cohesion,
        min_comment_ratio,
        max_errhandle,
        min_typecov,
        max_vuln_critical,
        max_vuln_high,
        max_sast,
        max_crypto,
        max_license_violations,
    ) = load_config_thresholds(
        ".quality.toml",
        (
            cfg.max_crap,
            cfg.min_doc,
            cfg.max_debt,
            cfg.max_complexity_violations,
            cfg.max_duplication,
            cfg.max_taint,
            cfg.max_risk,
            cfg.max_coupling,
            cfg.min_propcov,
            cfg.max_fuzz_risk,
            cfg.max_linelen,
            cfg.max_halstead_bugs,
            cfg.max_secrets,
            cfg.max_deadcode,
            cfg.max_cohesion,
            cfg.min_comment_ratio,
            cfg.max_errhandle,
            cfg.min_typecov,
            cfg.max_vuln_critical,
            cfg.max_vuln_high,
            cfg.max_sast,
            cfg.max_crypto,
            cfg.max_license_violations,
        ),
    );

    let skip_list: Vec<String> = parse_comma_list(&skip);
    let only_list: Vec<String> = parse_comma_list(&only);


    let check_start = Instant::now();
    let show_progress = format == "text";

    macro_rules! run_check {
        ($label:expr, $expr:expr) => {{
            if show_progress {
                let label = $label;
                let t = Instant::now();
                let result = run_with_spinner(label, || $expr);
                let elapsed = format_elapsed(t.elapsed());
                let detail = &result.message;
                let icon = if result.passed {
                    "\u{2713}".green().bold()
                } else {
                    "\u{2717}".red().bold()
                };
                let name_col = if result.passed {
                    label.normal()
                } else {
                    label.red()
                };
                let msg_col = if result.passed {
                    detail.bright_black()
                } else {
                    detail.red()
                };
                eprintln!(
                    "  {} {:<18} {}  {}",
                    icon,
                    name_col,
                    elapsed.bright_black(),
                    msg_col
                );
                if !result.passed || verbose {
                    print_offenders(&result);
                }
                result
            } else {
                $expr
            }
        }};
    }

    let mut checks = Vec::new();

    if check_should_run("crap", &only_list, &skip_list) {
        checks.push(run_check!(
            "crap",
            check_crap(&path, recursive, &coverage, max_crap)
        ));
    }
    if check_should_run("debt", &only_list, &skip_list) {
        checks.push(run_check!("debt", check_debt(&path, recursive, max_debt)));
    }
    if check_should_run("doc", &only_list, &skip_list) {
        checks.push(run_check!(
            "doc_coverage",
            check_doc_coverage(&path, recursive, min_doc)
        ));
    }
    if check_should_run("complexity", &only_list, &skip_list) {
        checks.push(run_check!(
            "complexity",
            check_complexity(&path, recursive, 10, max_complexity_violations)
        ));
    }
    if check_should_run("taint", &only_list, &skip_list) {
        checks.push(run_check!(
            "taint",
            check_taint(&path, recursive, max_taint)
        ));
    }
    if check_should_run("dup", &only_list, &skip_list) || check_should_run("dupfind", &only_list, &skip_list) || check_should_run("duplication", &only_list, &skip_list) {
        checks.push(run_check!(
            "duplication",
            check_dupfind(&path, recursive, max_duplication)
        ));
    }
    if check_should_run("risk", &only_list, &skip_list) || check_should_run("riskmap", &only_list, &skip_list) {
        checks.push(run_check!(
            "riskmap",
            check_riskmap(&path, recursive, max_risk)
        ));
    }
    if check_should_run("coupling", &only_list, &skip_list) {
        checks.push(run_check!("coupling", check_coupling(&path, max_coupling)));
    }
    if check_should_run("propcov", &only_list, &skip_list) {
        checks.push(run_check!(
            "propcov",
            check_propcov(&path, recursive, min_propcov)
        ));
    }
    if check_should_run("fuzz", &only_list, &skip_list) {
        checks.push(run_check!(
            "fuzz",
            check_fuzz(&path, recursive, max_fuzz_risk)
        ));
    }
    if check_should_run("linelen", &only_list, &skip_list) {
        checks.push(run_check!(
            "linelen",
            check_linelen(&path, recursive, max_linelen)
        ));
    }
    if check_should_run("halstead", &only_list, &skip_list) {
        checks.push(run_check!(
            "halstead",
            check_halstead(&path, recursive, max_halstead_bugs)
        ));
    }
    if check_should_run("secrets", &only_list, &skip_list) {
        checks.push(run_check!(
            "secrets",
            check_secrets(&path, recursive, max_secrets)
        ));
    }
    if check_should_run("deadcode", &only_list, &skip_list) {
        checks.push(run_check!(
            "deadcode",
            check_deadcode(&path, recursive, max_deadcode)
        ));
    }
    if check_should_run("cohesion", &only_list, &skip_list) {
        checks.push(run_check!(
            "cohesion",
            check_cohesion(&path, recursive, max_cohesion)
        ));
    }
    if check_should_run("comments", &only_list, &skip_list) {
        checks.push(run_check!(
            "comments",
            check_comments(&path, recursive, min_comment_ratio)
        ));
    }
    if check_should_run("errhandle", &only_list, &skip_list) {
        checks.push(run_check!(
            "errhandle",
            check_errhandle(&path, recursive, max_errhandle)
        ));
    }
    if check_should_run("typecov", &only_list, &skip_list) && min_typecov > 0.0 {
        checks.push(run_check!(
            "typecov",
            check_typecov(&path, recursive, min_typecov)
        ));
    }
    if check_should_run("vulnscan", &only_list, &skip_list) {
        checks.push(run_check!(
            "vulnscan",
            check_vulnscan(&path, max_vuln_critical, max_vuln_high)
        ));
    }
    if check_should_run("sast", &only_list, &skip_list) {
        checks.push(run_check!("sast", check_sast(&path, recursive, max_sast)));
    }
    if check_should_run("crypto", &only_list, &skip_list) {
        checks.push(run_check!(
            "crypto",
            check_crypto(&path, recursive, max_crypto)
        ));
    }
    if check_should_run("licenses", &only_list, &skip_list) {
        checks.push(run_check!(
            "licenses",
            check_licenses(&path, max_license_violations)
        ));
    }
    if check_should_run("mutate", &only_list, &skip_list) {
        checks.push(run_check!("mutate", {
            let args = vec![&path, "--format", "json"];
            let res = run_tool("mutation-test", "mutate", &args, Instant::now());
            let score = res
                .data
                .get("summary")
                .and_then(|s| s.get("kill_rate"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let min_kill_rate = std::fs::read_to_string(".quality.toml")
                .ok()
                .and_then(|content| {
                    content.lines().find_map(|line| {
                        let line = line.trim();
                        if line.starts_with("min_kill_rate") {
                            line.split('=').nth(1)?.trim().parse::<f64>().ok()
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(0.0);
            let passed = score >= min_kill_rate;
            let msg = if passed {
                format!("Mutation testing passed (kill rate {:.1}%)", score)
            } else {
                format!(
                    "Mutation testing failed (kill rate {:.1}% < {:.0}%)",
                    score, min_kill_rate
                )
            };
            CheckResult {
                name: "mutate".into(),
                passed,
                score: Some(score),
                threshold: Some(min_kill_rate),
                message: msg,
                details: serde_json::json!({}),
                severity: None,
                help: None,
                rule_id: None,
                findings: Vec::new(),
            }
        }));
    }
    if check_should_run("access-control", &only_list, &skip_list) {
        checks.push(run_check!("access-control", {
            check_access_control(&path, true, 0)
        }));
    }
    if check_should_run("supply-chain", &only_list, &skip_list) {
        checks.push(run_check!("supply-chain", check_supply_chain(&path, 0)));
    }
    if check_should_run("outdated", &only_list, &skip_list) {
        checks.push(run_check!("outdated", check_outdated(&path, max_outdated)));
    }

    let passed = checks.iter().all(|c| c.passed);
    let total_funcs: usize = checks
        .iter()
        .filter_map(|c| c.details.get("total_functions").and_then(|v| v.as_u64()))
        .map(|v| v as usize)
        .sum();

    let passed_count = checks.iter().filter(|c| c.passed).count();
    let failed_count = checks.len() - passed_count;
    let total_checks = checks.len();

    let report = CheckReport {
        passed,
        path: path.clone(),
        checks: checks.clone(),
        summary: CheckSummary {
            total_checks,
            passed_checks: passed_count,
            failed_checks: failed_count,
            functions_analyzed: total_funcs,
            avg_complexity: 0.0,
            avg_crap: 0.0,
        },
        file_summary: aggregate_file_summary(&checks),
    };
    let (health, grade) = health_score(&report.checks);

    if pr_comment {
        let md = pr_comment_md(&report, &path);
        println!("{}", md);
        std::process::exit(if passed { 0 } else { 1 });
    }

    match format.as_str() {
        "text" => {
            print_summary_box(
                "COGENT CHECK",
                passed,
                &path,
                passed_count,
                total_checks,
                check_start.elapsed(),
                &report.checks,
            );
            if !passed {
                print_severity_grouped(&report.checks);
                print_fix_summary(&report.checks);
            }
        }
        "ndjson" => output_ndjson(&report),
        "sarif" => output_sarif(&report),
        "junit" => output_junit(&report),
        "findings" => output_findings_ndjson(&report),
        "markdown" => output_markdown(&report, &path),
        _ => output_json(&report),
    }

    if cfg.ci {
        let summary = serde_json::json!({
            "passed": report.passed,
            "score": health,
            "grade": grade.to_string(),
            "failed_checks": report.checks.iter().filter(|c| !c.passed).map(|c| c.name.clone()).collect::<Vec<_>>(),
            "critical_findings": report.checks.iter().map(|c| c.findings.iter().filter(|f| f.severity == "critical").count()).sum::<usize>(),
            "report_url": "./cogent-report.html",
        });
        if let Err(e) = std::fs::write(
            "cogent-summary.json",
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        ) {
            eprintln!("Warning: could not write cogent-summary.json: {}", e);
        }
        let html = render_html_report(
            &report,
            &path,
            &chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
            &[
                "taint",
                "secrets",
                "sast",
                "crypto",
                "vulnscan",
                "access-control",
            ],
            &[
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
            ],
            &["licenses", "outdated", "supply-chain"],
        );
        if let Err(e) = std::fs::write("cogent-report.html", html) {
            eprintln!("Warning: could not write cogent-report.html: {}", e);
        }
    }

    let total_findings: usize = report.checks.iter().map(|c| c.findings.len()).sum();
    audit::append_audit_trail("check", &path, total_findings, check_start.elapsed());

    if passed { 0 } else { 1 }
}

/// Dispatch a parsed `Commands` value to its handler and return an exit code.
pub fn dispatch(command: Commands) -> i32 {
    match command {
        Commands::Check {
            path,
            recursive,
            format,
            coverage,
            max_crap,
            min_doc,
            max_debt,
            max_complexity_violations,
            max_taint,
            max_duplication,
            max_risk,
            max_coupling,
            min_propcov,
            max_fuzz_risk,
            max_linelen,
            max_halstead_bugs,
            max_secrets,
            max_deadcode,
            max_cohesion,
            min_comment_ratio,
            max_errhandle,
            min_typecov,
            max_vuln_critical,
            max_vuln_high,
            max_sast,
            max_crypto,
            max_license_violations,
            max_outdated,
            skip,
            only,
            ci,
            verbose,
            pr_comment,
            force,
        } => run_check_subcommand(
            path,
            recursive,
            format,
            CheckCommandConfig {
                coverage,
                max_crap,
                min_doc,
                max_debt,
                max_complexity_violations,
                max_taint,
                max_duplication,
                max_risk,
                max_coupling,
                min_propcov,
                max_fuzz_risk,
                max_linelen,
                max_halstead_bugs,
                max_secrets,
                max_deadcode,
                max_cohesion,
                min_comment_ratio,
                max_errhandle,
                min_typecov,
                max_vuln_critical,
                max_vuln_high,
                max_sast,
                max_crypto,
                max_license_violations,
                max_outdated,
                skip,
                only,
                ci,
                verbose,
                pr_comment,
                force,
            },
        ),

        Commands::Crap { path, recursive, coverage, format } => run_check_command_full("crap", run_spinner_or("crap", &format, || check_crap(&path, recursive, &coverage, 30.0)), &format),

        Commands::Debt { path, recursive, format, .. } => run_check_command_full("debt", run_spinner_or("debt", &format, || check_debt(&path, recursive, 1000)), &format),

        Commands::Doccov { path, recursive, format } => run_check_command_full("doccov", run_spinner_or("doccov", &format, || check_doc_coverage(&path, recursive, 0.0)), &format),

        Commands::Dupfind { path, recursive, min_lines, format } => run_check_command(run_spinner_or("dupfind", &format, || check_dupfind(&path, recursive, min_lines as f64)), &format),

        Commands::Complexity { path, recursive, min_complexity, format } => run_check_command_full("complexity", run_spinner_or("complexity", &format, || check_complexity(&path, recursive, min_complexity, 0)), &format),

        Commands::Taint { path, recursive, max_taint, format } => run_check_command(run_spinner_or("taint", &format, || check_taint(&path, recursive, max_taint)), &format),

        Commands::Coupling { path, max_coupling, format } => run_check_command(run_spinner_or("coupling", &format, || check_coupling(&path, max_coupling)), &format),

        Commands::Riskmap { path, max_risk, format } => run_check_command(run_spinner_or("riskmap", &format, || check_riskmap(&path, false, max_risk)), &format),

        Commands::Mutate { path, format } => {
            let args = vec![&path, "--format", "json"];
            let res = run_tool("mutation-test", "mutate", &args, Instant::now());
            let passed = res.success;
            let msg = res.error.unwrap_or_else(|| {
                res.data
                    .get("summary")
                    .and_then(|s| s.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });
            match format.as_str() {
                "text" => println!("{}", msg),
                _ => println!("{}", serde_json::to_string_pretty(&res.data).unwrap()),
            }
            if passed { 0 } else { 1 }
        }

        Commands::Fuzz { path, recursive, max_fuzz_risk, format } => run_check_command(run_spinner_or("fuzz", &format, || check_fuzz(&path, recursive, max_fuzz_risk)), &format),
        Commands::Propcov { path, recursive, min_propcov, format } => run_check_command(run_spinner_or("propcov", &format, || check_propcov(&path, recursive, min_propcov)), &format),
        Commands::Linelen { path, recursive, max_violations, format } => run_check_command(run_spinner_or("linelen", &format, || check_linelen(&path, recursive, max_violations)), &format),
        Commands::Halstead { path, recursive, max_bugs, format } => run_check_command(run_spinner_or("halstead", &format, || check_halstead(&path, recursive, max_bugs)), &format),
        Commands::Secrets { path, recursive, max_findings, format } => run_check_command(run_spinner_or("secrets", &format, || check_secrets(&path, recursive, max_findings)), &format),
        Commands::Deadcode { path, recursive, max_findings, format } => run_check_command(run_spinner_or("deadcode", &format, || check_deadcode(&path, recursive, max_findings)), &format),

        Commands::Cohesion { path, recursive, max_violations, format } => run_check_command(run_spinner_or("cohesion", &format, || check_cohesion(&path, recursive, max_violations)), &format),
        Commands::Comments { path, recursive, min_ratio, format } => run_check_command(run_spinner_or("comments", &format, || check_comments(&path, recursive, min_ratio)), &format),
        Commands::Errhandle { path, recursive, max_violations, format } => run_check_command(run_spinner_or("errhandle", &format, || check_errhandle(&path, recursive, max_violations)), &format),
        Commands::Typecov { path, recursive, min_pct, format } => run_check_command(run_spinner_or("typecov", &format, || check_typecov(&path, recursive, min_pct)), &format),
        Commands::Vulnscan { path, max_critical, max_high, format } => run_check_command(run_spinner_or("vulnscan", &format, || check_vulnscan(&path, max_critical, max_high)), &format),
        Commands::Sast { path, recursive, max_findings, format } => run_check_command(run_spinner_or("sast", &format, || check_sast(&path, recursive, max_findings)), &format),
        Commands::Crypto { path, recursive, max_findings, format } => run_check_command(run_spinner_or("crypto", &format, || check_crypto(&path, recursive, max_findings)), &format),
        Commands::Licenses { path, max_violations, format } => run_check_command(run_spinner_or("licenses", &format, || check_licenses(&path, max_violations)), &format),
        Commands::Outdated { path, max_major_behind, format } => run_check_command(run_spinner_or("outdated", &format, || check_outdated(&path, max_major_behind)), &format),
        Commands::AccessControl { path, recursive, max_violations, format } => run_check_command(run_spinner_or("access-control", &format, || check_access_control(&path, recursive, max_violations)), &format),
        Commands::SupplyChain { path, max_risks, format } => run_check_command(run_spinner_or("supply-chain", &format, || check_supply_chain(&path, max_risks)), &format),

        Commands::Sbom { path, format, output } => {
            let args = vec![&path, "--format", &format];
            let res = run_tool("sbom", "sbom", &args, Instant::now());
            let stdout = res.data.get("raw").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(out_path) = output {
                if let Err(e) = std::fs::write(&out_path, stdout) {
                    eprintln!("Failed to write SBOM to {}: {}", out_path, e);
                    return 1;
                }
                println!("SBOM written to {}", out_path);
            } else {
                println!("{}", stdout);
            }
            if res.success { 0 } else { 1 }
        }

        Commands::Doctor { format } => doctor_command(&format),

        Commands::Setup => {
            setup_command();
            0
        }

        Commands::Init { output, ci } => {
            let detect_start = Instant::now();
            let profile = run_with_spinner("detecting project ecosystem", || detect_project("."));

            let reason = if std::path::Path::new("Cargo.toml").exists() {
                "Cargo.toml found"
            } else if std::path::Path::new("go.mod").exists() {
                "go.mod found"
            } else if std::path::Path::new("pyproject.toml").exists()
                || std::path::Path::new("setup.py").exists()
            {
                "pyproject.toml / setup.py found"
            } else if std::path::Path::new("package.json").exists() {
                "package.json found"
            } else {
                "no known manifest found — using generic defaults"
            };

            eprintln!(
                "  {} detected: {}  ({})  {}",
                "✓".green().bold(),
                profile.ecosystem.to_string().cyan().bold(),
                reason.bright_black(),
                if profile.test_cmd.is_empty() {
                    "no test runner".to_string().bright_black().to_string()
                } else {
                    format!("test: {}", profile.test_cmd.join(" "))
                        .bright_black()
                        .to_string()
                }
            );
            let _ = detect_start;
            if ci {
                init_ci(&output, &profile)
            } else {
                let write_start = Instant::now();
                generate_config(&output, &profile);
                eprintln!(
                    "  {} wrote {}  ({})",
                    "✓".green().bold(),
                    output.cyan(),
                    format_elapsed(write_start.elapsed()).bright_black()
                );
                eprintln!();
                eprintln!("  {} Key thresholds chosen:", "▶".cyan().bold());
                eprintln!(
                    "    {} max_crap    = {}",
                    "·".bright_black(),
                    profile.max_crap.to_string().cyan()
                );
                eprintln!(
                    "    {} min_doc     = {}%",
                    "·".bright_black(),
                    profile.min_doc.to_string().cyan()
                );
                eprintln!(
                    "    {} max_debt    = {}",
                    "·".bright_black(),
                    profile.max_debt.to_string().cyan()
                );
                eprintln!(
                    "    {} max_complexity_violations = {}",
                    "·".bright_black(),
                    profile.max_complexity_violations.to_string().cyan()
                );
                eprintln!();
                eprintln!(
                    "  {} {} runs 20+ checks and produces a 0-100 score + letter grade.",
                    "▶".cyan().bold(),
                    "cogent check .".cyan().bold()
                );
                eprintln!();
                eprintln!("  {} Next steps:", "▶".cyan().bold());
                eprintln!(
                    "    1. {} cogent check .          {}",
                    "$".bright_black(),
                    "— run all checks now".bright_black()
                );
                eprintln!(
                    "    2. {} cogent report .         {}",
                    "$".bright_black(),
                    "— generate HTML audit report".bright_black()
                );
                eprintln!(
                    "    3. {} cogent init --ci        {}",
                    "$".bright_black(),
                    "— wire GitHub Actions + pre-commit hook".bright_black()
                );
                eprintln!(
                    "    4. {} cogent watch .          {}",
                    "$".bright_black(),
                    "— live re-check on file save".bright_black()
                );
                eprintln!();
                eprintln!(
                    "  {} Tip: edit {} to tune thresholds for your project.",
                    "ℹ".cyan(),
                    output.cyan()
                );
                0
            }
        }

        Commands::Explain { tool } => {
            explain_command(&tool);
            0
        }

        Commands::Discover { format } => {
            discover_command(&format);
            0
        }

        Commands::Run {
            path,
            config,
            format,
            baseline,
            no_fail_on_regression,
        } => run_batch(
            &path,
            &config,
            &format,
            baseline.as_deref(),
            no_fail_on_regression,
        ),

        Commands::History {
            action,
            dir,
            last,
            report,
            format,
        } => history_command(&action, &dir, last, report.as_deref(), &format),

        Commands::InstallHooks { repo, fast } => install_hooks(&repo, fast),

        Commands::UninstallHooks { repo } => uninstall_hooks(&repo),

        Commands::Watch {
            path,
            checks,
            debounce_ms,
            no_tests,
            full,
        } => watch_mode(&path, &checks, debounce_ms, no_tests, full),

        Commands::Report {
            path,
            format,
            output,
            project,
            from_json,
            skip,
            open,
        } => report_command(
            &path,
            &format,
            output.as_deref(),
            project.as_deref(),
            from_json.as_deref(),
            skip.as_deref(),
            open,
        ),

        Commands::Diff { before, after, format } => diff_command(&before, &after, &format),

        Commands::Serve { port, history_dir } => {
            serve_command(port, &history_dir);
            0
        }

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            use clap_complete::{generate, Shell};
            let shell = match shell.as_str() {
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                "fish" => Shell::Fish,
                "powershell" => Shell::PowerShell,
                "elvish" => Shell::Elvish,
                _ => {
                    eprintln!(
                        "Unknown shell '{}'. Supported: bash, zsh, fish, powershell, elvish",
                        shell
                    );
                    std::process::exit(2);
                }
            };
            let mut cli = crate::cli::Cli::command();
            generate(shell, &mut cli, "cogent", &mut std::io::stdout());
            0
        }

        Commands::Policy { action } => match action {
            PolicyAction::Validate => {
                let known_tools: Vec<String> = [
                    "secrets", "sast", "debt", "dupfind", "deadcode", "linelen",
                    "comments", "coupling", "cohesion", "halstead", "crap", "riskmap",
                    "cryptocheck", "errhandle", "taint", "typecov", "propcov", "fuzz",
                    "licenses", "supply-chain", "access-control", "vulnscan", "mutate",
                    "doccov", "sbom", "complexity",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                let policies = audit::discover_policies(".");
                if policies.is_empty() {
                    eprintln!("No policies found in ./.cogent-policies/");
                    0
                } else {
                    let mut all_valid = true;
                    for p in &policies {
                        let result = audit::validate_policy(p, &known_tools);
                        println!(
                            "{}  {}",
                            if result.valid { "✓".green() } else { "✗".red() },
                            p.display()
                        );
                        for w in &result.warnings {
                            println!("  ⚠  {}", w);
                        }
                        for e in &result.errors {
                            println!("  ✗  {}", e);
                            all_valid = false;
                        }
                    }
                    if all_valid { 0 } else { 1 }
                }
            }
            PolicyAction::Check { path, format, force } => {
                println!(
                    "Policy-based check on {} (format: {}, force: {})",
                    path, format, force
                );
                0
            }
        },

        Commands::Exception { action } => match action {
            ExceptionAction::Add {
                finding_id,
                rule_id,
                file,
                reason,
                reviewer,
            } => match audit::add_exception(&finding_id, &rule_id, &file, &reason, &reviewer) {
                Ok(id) => {
                    println!(
                        "Exception {} proposed (pending approval by {})",
                        id, reviewer
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Failed to add exception: {}", e);
                    2
                }
            },
            ExceptionAction::List { status } => {
                let exceptions = audit::list_exceptions(status.as_deref());
                if exceptions.is_empty() {
                    println!("No exceptions found.");
                } else {
                    println!(
                        "{:<10} {:<16} {:<20} {:<12} Reviewer",
                        "ID", "Finding", "Rule", "Status"
                    );
                    for e in &exceptions {
                        println!(
                            "{:<10} {:<16} {:<20} {:<12} {}",
                            e.id, e.finding_id, e.rule_id, e.status, e.reviewer
                        );
                    }
                }
                0
            }
            ExceptionAction::Approve { id } => match audit::approve_exception(&id) {
                Ok(()) => {
                    println!("Exception {} approved.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to approve exception: {}", e);
                    2
                }
            },
            ExceptionAction::Revoke { id } => match audit::revoke_exception(&id) {
                Ok(()) => {
                    println!("Exception {} revoked.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to revoke exception: {}", e);
                    2
                }
            },
        },

        Commands::Remediate { verify, path } => {
            if verify {
                println!("Verifying remediation on path: {}", path);
                audit::print_remediation_summary();
            } else {
                println!("Showing remediation status for: {}", path);
                audit::print_remediation_summary();
            }
            0
        }

        Commands::AuditTrail { verify, command, since } => {
            if verify {
                let (ok, errors) = audit::verify_audit_trail();
                if ok {
                    println!("{} Audit trail integrity verified.", "✓".green());
                } else {
                    println!("{} Audit trail verification failed:", "✗".red());
                    for e in &errors {
                        println!("  {}", e);
                    }
                }
                if ok { 0 } else { 2 }
            } else {
                let entries = audit::query_audit_trail(since.as_deref(), command.as_deref());
                if entries.is_empty() {
                    println!("No audit trail entries found.");
                } else {
                    println!(
                        "{:<26} {:<12} {:<20} {:<20} Findings",
                        "Timestamp", "Actor", "Command", "Scope"
                    );
                    for e in &entries {
                        println!(
                            "{:<26} {:<12} {:<20} {:<20} {}",
                            e.timestamp, e.actor, e.command, e.scope, e.findings_count
                        );
                    }
                }
                0
            }
        }

        Commands::Audit {
            path,
            format,
            only,
            evidence,
            framework,
            checks,
            skip,
            ci,
            verify,
        } => run_audit_subcommand(
            path,
            format,
            AuditCommandConfig {
                only,
                evidence,
                framework,
                checks,
                skip,
                ci,
                verify,
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CheckResult;

    fn make_result(passed: bool, msg: &str) -> CheckResult {
        CheckResult {
            name: "test-check".into(),
            passed,
            score: Some(42.0),
            threshold: Some(50.0),
            message: msg.into(),
            details: serde_json::json!({"key": "value"}),
            severity: Some("info".into()),
            help: Some("Test help".into()),
            findings: Vec::new(),
            rule_id: Some("test-rule".into()),
        }
    }

    // ── run_spinner_or ────────────────────────────────────────────────────

    #[test]
    fn test_run_spinner_or_non_text_direct_call() {
        // When format is not "text", the closure is called directly
        let r = run_spinner_or("test", "json", || make_result(true, "direct"));
        assert!(r.passed);
        assert_eq!(r.message, "direct");
    }

    #[test]
    fn test_run_spinner_or_non_text_fail() {
        let r = run_spinner_or("test", "ndjson", || make_result(false, "failed"));
        assert!(!r.passed);
        assert_eq!(r.message, "failed");
    }

    // ── run_check_command (simple display) ────────────────────────────────

    #[test]
    fn test_run_check_command_text_returns_0_on_pass() {
        assert_eq!(run_check_command(make_result(true, "ok"), "text"), 0);
    }

    #[test]
    fn test_run_check_command_text_returns_1_on_fail() {
        assert_eq!(run_check_command(make_result(false, "fail"), "text"), 1);
    }

    #[test]
    fn test_run_check_command_json_returns_0_on_pass() {
        assert_eq!(run_check_command(make_result(true, "ok"), "json"), 0);
    }

    #[test]
    fn test_run_check_command_json_returns_1_on_fail() {
        assert_eq!(run_check_command(make_result(false, "fail"), "json"), 1);
    }

    #[test]
    fn test_run_check_command_any_non_text_returns_properly() {
        assert_eq!(run_check_command(make_result(true, "ok"), "sarif"), 0);
        assert_eq!(run_check_command(make_result(false, "fail"), "ndjson"), 1);
    }

    // ── run_check_command_full (icon + message display) ──────────────────

    #[test]
    fn test_run_check_command_full_text_returns_0_on_pass() {
        assert_eq!(run_check_command_full("greeter", make_result(true, "ok"), "text"), 0);
    }

    #[test]
    fn test_run_check_command_full_text_returns_1_on_fail() {
        assert_eq!(run_check_command_full("greeter", make_result(false, "fail"), "text"), 1);
    }

    #[test]
    fn test_run_check_command_full_json_returns_0_on_pass() {
        assert_eq!(run_check_command_full("greeter", make_result(true, "ok"), "json"), 0);
    }

    #[test]
    fn test_run_check_command_full_json_returns_1_on_fail() {
        assert_eq!(run_check_command_full("greeter", make_result(false, "fail"), "json"), 1);
    }

    #[test]
    fn test_run_check_command_full_any_non_text_returns_properly() {
        assert_eq!(run_check_command_full("greeter", make_result(true, "ok"), "markdown"), 0);
        assert_eq!(run_check_command_full("greeter", make_result(false, "fail"), "junit"), 1);
    }

    // ── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn test_run_check_command_empty_message() {
        // Empty messages should not crash anything
        assert_eq!(run_check_command(make_result(true, ""), "text"), 0);
        assert_eq!(run_check_command_full("greeter", make_result(false, ""), "text"), 1);
    }

    #[test]
    fn test_run_check_command_full_with_empty_label() {
        // Empty labels should not crash
        assert_eq!(run_check_command_full("", make_result(true, "ok"), "text"), 0);
        assert_eq!(run_check_command_full("", make_result(false, "fail"), "json"), 1);
    }

    #[test]
    fn test_run_spinner_or_various_formats_all_skip_spinner() {
        // "json", "sarif", "ndjson", "markdown", "junit" — none are "text", so no spinner
        for fmt in &["json", "sarif", "ndjson", "markdown", "junit", "findings"] {
            let r = run_spinner_or("test", fmt, || make_result(true, fmt));
            assert!(r.passed, "format '{}' should skip spinner and pass", fmt);
            assert_eq!(r.message, *fmt);
        }
    }

    // ── ci_format ────────────────────────────────────────────────────────

    #[test]
    fn test_ci_format_false_preserves_format() {
        // ci=false should preserve the original format unchanged
        assert_eq!(ci_format("json".to_string(), false), "json");
        assert_eq!(ci_format("markdown".to_string(), false), "markdown");
        assert_eq!(ci_format("text".to_string(), false), "text");
        assert_eq!(ci_format("agent".to_string(), false), "agent");
        assert_eq!(ci_format("sarif".to_string(), false), "sarif");
        assert_eq!(ci_format("ndjson".to_string(), false), "ndjson");
    }

    #[test]
    fn test_ci_format_true_always_returns_json() {
        // ci=true should force "json" regardless of the requested format
        assert_eq!(ci_format("json".to_string(), true), "json");
        assert_eq!(ci_format("markdown".to_string(), true), "json");
        assert_eq!(ci_format("text".to_string(), true), "json");
        assert_eq!(ci_format("agent".to_string(), true), "json");
        assert_eq!(ci_format("sarif".to_string(), true), "json");
        assert_eq!(ci_format("ndjson".to_string(), true), "json");
        assert_eq!(ci_format("junit".to_string(), true), "json");
        assert_eq!(ci_format("findings".to_string(), true), "json");
    }

    #[test]
    fn test_ci_format_empty_string_false_preserves_empty() {
        // Edge case: empty format string with ci=false is preserved
        assert_eq!(ci_format(String::new(), false), "");
    }

    #[test]
    fn test_ci_format_empty_string_true_returns_json() {
        // Edge case: empty format string with ci=true forces json
        assert_eq!(ci_format(String::new(), true), "json");
    }

    // ── CheckCommandConfig construction ────────────────────────────────────

    #[test]
    fn test_check_command_config_default_values() {
        // Verify the struct can be constructed with typical defaults
        let cfg = CheckCommandConfig {
            coverage: None,
            max_crap: 30.0,
            min_doc: 50.0,
            max_debt: 100,
            max_complexity_violations: 0,
            max_taint: 0,
            max_duplication: 5.0,
            max_risk: 75.0,
            max_coupling: 10,
            min_propcov: 60.0,
            max_fuzz_risk: 30,
            max_linelen: 120,
            max_halstead_bugs: 15.0,
            max_secrets: 0,
            max_deadcode: 0,
            max_cohesion: 0,
            min_comment_ratio: 10.0,
            max_errhandle: 0,
            min_typecov: 50.0,
            max_vuln_critical: 0,
            max_vuln_high: 0,
            max_sast: 0,
            max_crypto: 0,
            max_license_violations: 0,
            max_outdated: 0,
            skip: None,
            only: None,
            ci: false,
            verbose: false,
            pr_comment: false,
            force: false,
        };
        assert!(!cfg.ci);
        assert_eq!(cfg.max_crap, 30.0);
        assert_eq!(cfg.max_fuzz_risk, 30);
        assert!(cfg.coverage.is_none());
    }

    #[test]
    fn test_check_command_config_with_coverage_and_skip() {
        let cfg = CheckCommandConfig {
            coverage: Some("lcov.info".into()),
            max_crap: 15.0,
            min_doc: 80.0,
            max_debt: 50,
            max_complexity_violations: 5,
            max_taint: 10,
            max_duplication: 3.0,
            max_risk: 50.0,
            max_coupling: 25,
            min_propcov: 70.0,
            max_fuzz_risk: 20,
            max_linelen: 100,
            max_halstead_bugs: 10.0,
            max_secrets: 5,
            max_deadcode: 10,
            max_cohesion: 10,
            min_comment_ratio: 15.0,
            max_errhandle: 5,
            min_typecov: 60.0,
            max_vuln_critical: 0,
            max_vuln_high: 3,
            max_sast: 10,
            max_crypto: 5,
            max_license_violations: 0,
            max_outdated: 3,
            skip: Some("crypto,licenses".into()),
            only: None,
            ci: true,
            verbose: true,
            pr_comment: false,
            force: true,
        };
        assert!(cfg.ci);
        assert!(cfg.force);
        assert_eq!(cfg.coverage.as_deref(), Some("lcov.info"));
        assert_eq!(cfg.skip.as_deref(), Some("crypto,licenses"));
        assert!(cfg.only.is_none());
    }

    #[test]
    fn test_check_command_config_with_only() {
        let cfg = CheckCommandConfig {
            coverage: None,
            max_crap: 30.0,
            min_doc: 50.0,
            max_debt: 100,
            max_complexity_violations: 0,
            max_taint: 0,
            max_duplication: 5.0,
            max_risk: 75.0,
            max_coupling: 10,
            min_propcov: 60.0,
            max_fuzz_risk: 30,
            max_linelen: 120,
            max_halstead_bugs: 15.0,
            max_secrets: 0,
            max_deadcode: 0,
            max_cohesion: 0,
            min_comment_ratio: 10.0,
            max_errhandle: 0,
            min_typecov: 50.0,
            max_vuln_critical: 0,
            max_vuln_high: 0,
            max_sast: 0,
            max_crypto: 0,
            max_license_violations: 0,
            max_outdated: 0,
            skip: None,
            only: Some("secrets,sast".into()),
            ci: false,
            verbose: false,
            pr_comment: true,
            force: false,
        };
        assert!(cfg.pr_comment);
        assert_eq!(cfg.only.as_deref(), Some("secrets,sast"));
    }

    // ── run_spinner_or edge cases ──────────────────────────────────────────

    #[test]
    fn test_run_spinner_or_text_format_calls_closure() {
        // When format IS "text", the closure is passed to run_with_spinner which
        // is a visual spinner function. We verify that the closure still returns
        // the expected result (the spinner wraps it).
        let r = run_spinner_or("test", "text", || make_result(true, "text-ok"));
        // The result message shows the closure was called
        assert!(r.passed);
    }

    #[test]
    fn test_run_spinner_or_text_format_fail() {
        let r = run_spinner_or("test", "text", || make_result(false, "text-fail"));
        assert!(!r.passed);
    }

    // ── check_should_run ───────────────────────────────────────────────────

    #[test]
    fn test_check_should_run_empty_lists() {
        // With empty only and skip lists, everything should run
        assert!(check_should_run("crap", &[], &[]));
        assert!(check_should_run("anything", &[], &[]));
    }

    #[test]
    fn test_check_should_run_skip_list() {
        let skip = vec!["crypto".to_string(), "licenses".to_string()];
        assert!(!check_should_run("crypto", &[], &skip));
        assert!(!check_should_run("licenses", &[], &skip));
        assert!(check_should_run("sast", &[], &skip));
        assert!(check_should_run("taint", &[], &skip));
    }

    #[test]
    fn test_check_should_run_only_list() {
        let only = vec!["secrets".to_string(), "sast".to_string()];
        assert!(check_should_run("secrets", &only, &[]));
        assert!(check_should_run("sast", &only, &[]));
        assert!(!check_should_run("crypto", &only, &[]));
        assert!(!check_should_run("licenses", &only, &[]));
    }

    #[test]
    fn test_check_should_run_only_takes_precedence_over_skip() {
        // When only_list is set, skip_list is ignored
        let only = vec!["crap".to_string()];
        let skip = vec!["crap".to_string()];
        assert!(check_should_run("crap", &only, &skip));
    }

    #[test]
    fn test_check_should_run_case_insensitive() {
        // Lists are always lowercased before being passed to check_should_run.
        let only = vec!["sast".to_string()];
        assert!(check_should_run("sast", &only, &[]));
        assert!(check_should_run("Sast", &only, &[]));
        assert!(check_should_run("SAST", &only, &[]));
        assert!(!check_should_run("crypto", &only, &[]));

        let skip = vec!["sast".to_string()];
        assert!(!check_should_run("sast", &[], &skip));
        assert!(!check_should_run("SAST", &[], &skip));
        assert!(check_should_run("crap", &[], &skip));
    }

    #[test]
    fn test_check_should_run_empty_only_list_falls_back_to_skip() {
        // Empty only list means "run all" minus skip
        let skip = vec!["comments".to_string()];
        assert!(!check_should_run("comments", &Vec::new(), &skip));
        assert!(check_should_run("debt", &Vec::new(), &skip));
    }

    // ── audit_should_run ───────────────────────────────────────────────────

    fn make_set(items: &[&str]) -> std::collections::HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_audit_should_run_empty_categories() {
        let only: std::collections::HashSet<String> = make_set(&[]);
        let skip: std::collections::HashSet<String> = make_set(&[]);
        let cats: Vec<&str> = vec!["secrets", "sast"];
        assert!(audit_should_run("secrets", &only, &skip, &cats));
        assert!(audit_should_run("sast", &only, &skip, &cats));
        assert!(!audit_should_run("crypto", &only, &skip, &cats));
    }

    #[test]
    fn test_audit_should_run_skip_set() {
        let only: std::collections::HashSet<String> = make_set(&[]);
        let skip: std::collections::HashSet<String> = make_set(&["secrets", "sast"]);
        let cats: Vec<&str> = vec!["secrets", "sast", "crypto"];
        assert!(!audit_should_run("secrets", &only, &skip, &cats));
        assert!(!audit_should_run("sast", &only, &skip, &cats));
        assert!(audit_should_run("crypto", &only, &skip, &cats));
    }

    #[test]
    fn test_audit_should_run_only_set() {
        let only: std::collections::HashSet<String> = make_set(&["crap"]);
        let skip: std::collections::HashSet<String> = make_set(&[]);
        let cats: Vec<&str> = vec!["secrets", "sast"];
        assert!(audit_should_run("crap", &only, &skip, &cats));
        assert!(!audit_should_run("secrets", &only, &skip, &cats));
    }

    #[test]
    fn test_audit_should_run_only_takes_precedence() {
        // only_set beats skip_set
        let only: std::collections::HashSet<String> = make_set(&["crap"]);
        let skip: std::collections::HashSet<String> = make_set(&["crap"]);
        let cats: Vec<&str> = vec!["crap"];
        assert!(audit_should_run("crap", &only, &skip, &cats));
    }

    #[test]
    fn test_audit_should_run_only_set_and_categories() {
        // Non-empty only_set ignores active_categories entirely
        let only: std::collections::HashSet<String> = make_set(&["crap"]);
        let skip: std::collections::HashSet<String> = make_set(&[]);
        let cats: Vec<&str> = vec!["secrets"];
        assert!(audit_should_run("crap", &only, &skip, &cats));
        // "secrets" is in categories but NOT in only_set
        assert!(!audit_should_run("secrets", &only, &skip, &cats));
    }

    #[test]
    fn test_audit_should_run_none_match() {
        let only: std::collections::HashSet<String> = make_set(&[]);
        let skip: std::collections::HashSet<String> = make_set(&[]);
        let cats: Vec<&str> = vec!["secrets", "sast"];
        // Not in any category
        assert!(!audit_should_run("nonexistent", &only, &skip, &cats));
    }

    // ── AuditCommandConfig construction ────────────────────────────────────

    #[test]
    fn test_audit_command_config_default_values() {
        let cfg = AuditCommandConfig {
            only: None,
            evidence: false,
            framework: None,
            checks: None,
            skip: None,
            ci: false,
            verify: false,
        };
        assert!(!cfg.ci);
        assert!(!cfg.evidence);
        assert!(!cfg.verify);
        assert!(cfg.only.is_none());
        assert!(cfg.framework.is_none());
        assert!(cfg.checks.is_none());
        assert!(cfg.skip.is_none());
    }

    #[test]
    fn test_audit_command_config_with_framework_and_ci() {
        let cfg = AuditCommandConfig {
            only: Some("security".into()),
            evidence: true,
            framework: Some("hqse".into()),
            checks: Some("secrets,sast".into()),
            skip: Some("crypto".into()),
            ci: true,
            verify: true,
        };
        assert!(cfg.ci);
        assert!(cfg.evidence);
        assert!(cfg.verify);
        assert_eq!(cfg.only.as_deref(), Some("security"));
        assert_eq!(cfg.framework.as_deref(), Some("hqse"));
        assert_eq!(cfg.checks.as_deref(), Some("secrets,sast"));
        assert_eq!(cfg.skip.as_deref(), Some("crypto"));
    }

    #[test]
    fn test_audit_command_config_all_none_false() {
        let cfg = AuditCommandConfig {
            only: None,
            evidence: false,
            framework: None,
            checks: None,
            skip: None,
            ci: false,
            verify: false,
        };
        // Default behavior: run all categories, no CI mode
        assert_eq!(cfg.only, None);
        assert_eq!(cfg.framework, None);
        assert_eq!(cfg.checks, None);
        assert_eq!(cfg.skip, None);
    }

    // ── prepare_audit_report_meta ───────────────────────────────────────────

    #[test]
    fn test_prepare_audit_report_meta_empty_checks() {
        let meta = prepare_audit_report_meta(
            vec![],
            None,
            false,
            false,
            false,
            ".".to_string(),
            std::time::Instant::now(),
        );
        assert!(meta.all_findings.is_empty(), "no checks → no findings");
        assert_eq!(meta.total_findings, 0);
        assert!(meta.passed, "empty checks should pass vacuously");
        assert_eq!(meta.health, 100);
        assert_eq!(meta.grade, 'A');
        assert_eq!(meta.use_framework, "");
        assert!(!meta.ci);
        assert_eq!(meta.newly_recorded, 0);
        assert_eq!(meta.auto_closed, 0);
    }

    #[test]
    fn test_prepare_audit_report_meta_with_framework() {
        let result = make_result(true, "all good");
        let meta = prepare_audit_report_meta(
            vec![result],
            Some("hqse".into()),
            false,
            false,
            false,
            ".".to_string(),
            std::time::Instant::now(),
        );
        assert_eq!(meta.use_framework, "hqse");
        assert_eq!(meta.total_findings, 0);
        assert!(meta.passed);
    }

    #[test]
    fn test_prepare_audit_report_meta_with_findings_and_ci() {
        let result = CheckResult {
            name: "test-check".into(),
            passed: false,
            score: Some(50.0),
            threshold: Some(100.0),
            message: "Found issues".into(),
            details: serde_json::json!({}),
            severity: Some("warning".into()),
            help: None,
            findings: vec![
                Finding {
                    file: "src/main.rs".into(),
                    line: None,
                    column: None,
                    severity: "critical".into(),
                    message: "Critical security issue".into(),
                    rule_id: "test-critical".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
                Finding {
                    file: "src/lib.rs".into(),
                    line: None,
                    column: None,
                    severity: "high".into(),
                    message: "High severity issue".into(),
                    rule_id: "test-high".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
                Finding {
                    file: "src/utils.rs".into(),
                    line: None,
                    column: None,
                    severity: "medium".into(),
                    message: "Medium issue".into(),
                    rule_id: "test-medium".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
            ],
            rule_id: Some("test-check".into()),
        };
        let meta = prepare_audit_report_meta(
            vec![result],
            None,
            false,   // no evidence enrichment
            false,   // no verify remediation
            true,    // CI mode
            "src".to_string(),
            std::time::Instant::now(),
        );
        assert_eq!(meta.total_findings, 3);
        assert_eq!(meta.critical, 1);
        assert_eq!(meta.high, 1);
        assert_eq!(meta.medium, 1);
        assert_eq!(meta.low, 0);
        assert!(!meta.passed);
        assert!(meta.ci);
    }

    #[test]
    fn test_prepare_audit_report_meta_evidence_and_verify() {
        // This test exercises the evidence enrichment and verify remediation paths.
        // With line=None and non-matching rule IDs, enrich_finding does no file I/O,
        // and verify_remediation with an empty remediation log returns 0 closed.
        let result = CheckResult {
            name: "audit-check".into(),
            passed: false,
            score: Some(30.0),
            threshold: Some(80.0),
            message: "Multiple issues found".into(),
            details: serde_json::json!({}),
            severity: None,
            help: Some("Review findings".into()),
            findings: vec![
                Finding {
                    file: "/tmp/test.rs".into(),
                    line: None,
                    column: None,
                    severity: "critical".into(),
                    message: "Critical vuln".into(),
                    rule_id: "test-sqli".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
                Finding {
                    file: "/tmp/test.rs".into(),
                    line: None,
                    column: None,
                    severity: "high".into(),
                    message: "XSS risk".into(),
                    rule_id: "test-xss".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
                Finding {
                    file: "/tmp/test.rs".into(),
                    line: None,
                    column: None,
                    severity: "low".into(),
                    message: "Info".into(),
                    rule_id: "test-info".into(),
                    fix_hint: "".into(),
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                },
            ],
            rule_id: Some("audit-check".into()),
        };
        // evidence=true triggers enrich_finding: line=None means snippet/context skipped,
        // but "test-sqli" and "test-xss" DO match suggested_fix_for/controls_for patterns
        // (sqli, xss) so suggested_fix and controls WILL be added to those findings.
        // verify=true triggers verify_remediation: no remediation log exists, so 0 closed.
        let meta = prepare_audit_report_meta(
            vec![result],
            None,
            true,   // evidence enrichment
            true,   // verify remediation
            false,
            "/tmp".to_string(),
            std::time::Instant::now(),
        );
        // Severity counts unchanged by enrichment (no matching rule IDs)
        assert_eq!(meta.total_findings, 3);
        assert_eq!(meta.critical, 1);
        assert_eq!(meta.high, 1);
        assert_eq!(meta.low, 1);
        assert!(!meta.passed);
        // auto_closed is 0 because no remediation log entries match these findings
        assert_eq!(
            meta.auto_closed, 0,
            "no remediation log → auto_closed is 0"
        );
        // newly_recorded will vary depending on audit state, but should be >= 0
        assert!(
            meta.newly_recorded <= meta.total_findings,
            "at most total_findings are newly recorded"
        );
    }

    // ── run_audit_report_format ─────────────────────────────────────────────

    /// Helper: an empty AuditReportMeta with vacuously-passing defaults.
    fn empty_audit_meta() -> AuditReportMeta {
        AuditReportMeta {
            all_findings: vec![],
            checks_run: vec![],
            path: "".to_string(),
            passed: true,
            health: 100,
            grade: 'A',
            total_findings: 0,
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
            duration: std::time::Duration::from_secs(0),
            newly_recorded: 0,
            auto_closed: 0,
            use_framework: "".to_string(),
            ci: false,
        }
    }

    #[test]
    fn test_run_audit_report_format_agent_does_not_panic() {
        // Agent format: iterates findings (empty), prints summary JSON, returns exit code
        let exit = run_audit_report_format("agent", &empty_audit_meta());
        assert_eq!(exit, 0, "empty meta with passed=true should return 0");
    }

    #[test]
    fn test_run_audit_report_format_json_does_not_panic() {
        // Json format: constructs CheckReport from meta, calls output_json
        let exit = run_audit_report_format("json", &empty_audit_meta());
        assert_eq!(exit, 0, "empty meta with passed=true should return 0");
    }

    #[test]
    fn test_run_audit_report_format_markdown_does_not_panic() {
        // Markdown format: constructs CheckReport, calls output_markdown_with_framework
        let exit = run_audit_report_format("markdown", &empty_audit_meta());
        assert_eq!(exit, 0, "empty meta with passed=true should return 0");
    }

    #[test]
    fn test_run_audit_report_format_ci_with_findings_returns_1() {
        // CI + findings > 0 should return 1 (failure exit code)
        let meta = AuditReportMeta {
            passed: false,
            total_findings: 5,
            ci: true,
            ..empty_audit_meta()
        };
        let exit = run_audit_report_format("json", &meta);
        assert_eq!(exit, 1, "ci + findings should return 1");
    }

    #[test]
    fn test_run_audit_report_format_failed_no_ci_returns_1() {
        // !passed + !ci should return 1
        let meta = AuditReportMeta {
            passed: false,
            total_findings: 3,
            ci: false,
            ..empty_audit_meta()
        };
        let exit = run_audit_report_format("json", &meta);
        assert_eq!(exit, 1, "!passed + !ci should return 1");
    }

    #[test]
    fn test_run_audit_report_format_ci_with_findings_overrides_pass() {
        // Edge case: ci=true + total_findings>0 takes priority over passed=true
        let meta = AuditReportMeta {
            passed: true,
            total_findings: 1,
            ci: true,
            ..empty_audit_meta()
        };
        let exit = run_audit_report_format("agent", &meta);
        assert_eq!(exit, 1, "ci+findings should return 1 even when passed=true");
    }

    // ── parse_comma_list ───────────────────────────────────────────────────

    #[test]
    fn test_parse_comma_list_none() {
        let result: Vec<String> = parse_comma_list(&None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_comma_list_empty() {
        let result: Vec<String> = parse_comma_list(&Some(String::new()));
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_comma_list_single() {
        let result = parse_comma_list(&Some("secrets".into()));
        assert_eq!(result, vec!["secrets"]);
    }

    #[test]
    fn test_parse_comma_list_multiple() {
        let result = parse_comma_list(&Some("secrets,sast,crypto".into()));
        assert_eq!(result, vec!["secrets", "sast", "crypto"]);
    }

    #[test]
    fn test_parse_comma_list_trims_whitespace() {
        let result = parse_comma_list(&Some("secrets, sast ,  crypto  ".into()));
        assert_eq!(result, vec!["secrets", "sast", "crypto"]);
    }

    #[test]
    fn test_parse_comma_list_lowercases() {
        let result = parse_comma_list(&Some("SECRETS,Sast".into()));
        assert_eq!(result, vec!["secrets", "sast"]);
    }

    #[test]
    fn test_parse_comma_list_filters_empty() {
        let result = parse_comma_list(&Some("secrets,,sast,".into()));
        assert_eq!(result, vec!["secrets", "sast"]);
    }

    #[test]
    fn test_parse_comma_list_to_hashset() {
        let vec = parse_comma_list(&Some("secrets,sast".into()));
        let set: std::collections::HashSet<String> = vec.into_iter().collect();
        assert!(set.contains("secrets"));
        assert!(set.contains("sast"));
        assert!(!set.contains("crypto"));
    }
}




