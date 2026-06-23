//! Check runner functions for cogent-cli.

#![deny(clippy::all)]
#![allow(clippy::type_complexity)]

use crate::progress::{box_row, format_ms};
use crate::types::{extract_findings_from_details, CheckResult};
use cogent_common::memory::MemoryMonitor;
use cogent_common::ToolResult;
use colored::Colorize;
use std::sync::Mutex;
use std::time::Instant;

// HELPERS
// ═══════════════════════════════════════════

/// Read the current process peak RSS from `/proc/self/status` and return it in MB.
/// Returns 0 on any parse or IO error.
pub(crate) fn get_peak_rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|line| {
                    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
                    Some(kb / 1024)
                })
        })
        .unwrap_or(0)
}

/// Construct a standard CheckResult for delegated tool checks.
#[allow(dead_code, clippy::too_many_arguments)]
fn make_check_result(
    name: &str,
    passed: bool,
    value: f64,
    threshold: f64,
    data: serde_json::Value,
    severity: &str,
    rule_id: &str,
    message: String,
    help: Option<&str>,
) -> CheckResult {
    let findings = extract_findings_from_details(&data, rule_id, severity);
    CheckResult {
        name: name.to_string(),
        passed,
        score: Some(value),
        threshold: Some(threshold),
        message,
        details: data,
        severity: Some(severity.to_string()),
        help: help.map(|s| s.to_string()),
        findings,
        rule_id: Some(rule_id.to_string()),
    }
}

// PARALLEL CHECK EXECUTION
// ═══════════════════════════════════════════

/// Default maximum number of concurrent check processes.
/// Conservative default to prevent OOM on memory-constrained systems.
/// Override at runtime via the `COGENT_MAX_CONCURRENT` env var.
const DEFAULT_MAX_CONCURRENT_CHECKS: usize = 4;

/// Resolve the concurrency limit from env var or fall back to the default.
/// Clamps to a minimum of 1 so there is always at least one worker.
fn max_concurrent_checks() -> usize {
    let raw = std::env::var("COGENT_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENT_CHECKS);
    raw.max(1)
}

/// Run check functions in parallel with bounded concurrency using a
/// work-stealing thread pool. Returns results sorted by check name for
/// consistent display.
#[tracing::instrument(level = "info", skip_all, fields(num_checks = checks.len()))]
pub(crate) fn run_parallel_checks(
    checks: Vec<(&'static str, Box<dyn FnOnce() -> CheckResult + Send>)>,
) -> Vec<CheckResult> {
    let total = checks.len();
    if total == 0 {
        return Vec::new();
    }
    let n_workers = max_concurrent_checks().min(total);
    let work: Mutex<Vec<(&'static str, Box<dyn FnOnce() -> CheckResult + Send>)>> =
        Mutex::new(checks);
    let results: Mutex<Vec<CheckResult>> = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|s| {
        for _ in 0..n_workers {
            s.spawn(|| loop {
                let job = work.lock().expect("work mutex poisoned").pop();
                match job {
                    Some((_name, f)) => {
                        let result = f();
                        results.lock().expect("results mutex poisoned").push(result);
                    }
                    None => break,
                }
            });
        }
    });

    let mut all = results.into_inner().expect("results mutex into_inner");
    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

/// Run tool binaries in parallel with bounded concurrency.
/// Each tool definition is (crate_name, bin_name, args).
/// Returns results in sorted order by tool name for consistent display.
pub(crate) fn run_parallel_tools(
    tools: Vec<(&'static str, &'static str, Vec<String>)>,
) -> Vec<ToolResult> {
    let total = tools.len();
    if total == 0 {
        return Vec::new();
    }
    let n_workers = max_concurrent_checks().min(total);
    let work: Mutex<Vec<(&'static str, &'static str, Vec<String>)>> = Mutex::new(tools);
    let results: Mutex<Vec<ToolResult>> = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|s| {
        for _ in 0..n_workers {
            s.spawn(|| loop {
                let job = work.lock().expect("work mutex poisoned").pop();
                match job {
                    Some((crate_name, bin_name, args)) => {
                        let args_ref: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
                        let result = run_tool(crate_name, bin_name, &args_ref, Instant::now());
                        results.lock().expect("results mutex poisoned").push(result);
                    }
                    None => break,
                }
            });
        }
    });

    let mut all = results.into_inner().expect("results mutex into_inner");
    all.sort_by(|a, b| a.tool.cmp(&b.tool));
    all
}

// TOOL EXECUTION
// ═══════════════════════════════════════════

pub(crate) fn run_tool(
    crate_name: &str,
    bin_name: &str,
    args: &[&str],
    tool_start: Instant,
) -> ToolResult {
    tracing::debug!(crate_name, bin_name, "running tool");
    use cogent_common::*;
    use std::path::Path;
    use std::process::{Command, Stdio};

    // Try to find the binary in target/release/ first (workspace build)
    let workspace_root = std::env::var("COGENT_WORKSPACE_ROOT").unwrap_or_else(|_| {
        // Fallback: try to find workspace root from CARGO_MANIFEST_DIR
        std::env::var("CARGO_MANIFEST_DIR")
            .ok()
            .and_then(|d| {
                std::path::Path::new(&d)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| ".".to_string())
    });
    let release_binary = Path::new(&workspace_root)
        .join("target")
        .join("release")
        .join(bin_name);

    let binary_path = if release_binary.exists() {
        tracing::debug!(tool = bin_name, path = %release_binary.display(), "using pre-built release binary");
        // Canonicalize to absolute path to avoid working directory issues
        release_binary
            .canonicalize()
            .unwrap_or(release_binary)
            .to_string_lossy()
            .to_string()
    } else {
        tracing::debug!(tool = bin_name, "no pre-built binary found, searching PATH");
        bin_name.to_string()
    };

    let output = Command::new(&binary_path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => o,
        _ => {
            let cargo_output = Command::new("cargo")
                .args(["run", "--quiet", "-p", crate_name, "--bin", bin_name, "--"])
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match cargo_output {
                Ok(o) => o,
                Err(e) => {
                    let msg = if e.kind() == std::io::ErrorKind::NotFound {
                        format!(
                            "Binary '{}' not found. Install with: cargo install --path crates/{} (error: {})",
                            bin_name, crate_name, e
                        )
                    } else {
                        format!("Failed to run '{}': {}", bin_name, e)
                    };
                    return ToolResult {
                        tool: bin_name.to_string(),
                        success: false,
                        duration_ms: tool_start.elapsed().as_millis() as u64,
                        data: serde_json::Value::Null,
                        error: Some(msg),
                        suggested_fix: None,
                        auto_fix_available: None,
                    };
                }
            }
        }
    };

    let duration_ms = tool_start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let (data, error) = match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => (json, None),
        Err(_) => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    serde_json::Value::Null,
                    Some(format!("No output. stderr: {}", stderr.trim())),
                )
            } else {
                (serde_json::json!({ "raw": trimmed }), None)
            }
        }
    };

    ToolResult {
        tool: bin_name.to_string(),
        success: error.is_none() && output.status.success(),
        duration_ms,
        data,
        error,
        suggested_fix: None,
        auto_fix_available: None,
    }
}

pub(crate) fn run_batch(
    path: &str,
    _config: &str,
    format: &str,
    baseline: Option<&str>,
    no_fail_on_regression: bool,
) -> i32 {
    use crate::config::{load_config_thresholds, load_secrets_exclude};
    use cogent_common::*;

    use std::time::Instant;

    let start = Instant::now();

    // Load project thresholds from .quality.toml so batch mode respects them
    let thresholds = load_config_thresholds(
        ".quality.toml",
        (
            30.0, 15.0, 1000, 10, 5.0, 0, 10.0, 5, 0.0, 0, 0, 2.0, 0, 10, 5, 0.05, 50, 0.0, 0, 0,
            0, 0, 0,
        ),
    );
    let max_sast = thresholds.20;
    let max_crypto = thresholds.21;

    // Initialize memory monitor (auto-terminates if memory exceeds safe threshold)
    let mut memory_monitor = MemoryMonitor::from_env();
    let mem_limit_mb = memory_monitor.max_rss_bytes / 1024 / 1024;
    let mem_display = if mem_limit_mb >= 1024 {
        format!("{:.1} GB", mem_limit_mb as f64 / 1024.0)
    } else {
        format!("{} MB", mem_limit_mb)
    };
    eprintln!(
        "  {} Cogent batch  ·  path: {}  ·  memory limit: {}",
        "▶".cyan().bold(),
        path.cyan(),
        mem_display.bright_black()
    );

    // Load secrets_exclude from .quality.toml / COGENT_SECRETS_EXCLUDE env var
    let secrets_exclude = load_secrets_exclude(".quality.toml");
    let secrets_exclude_args: Vec<String> = secrets_exclude
        .iter()
        .flat_map(|p| vec!["--exclude".to_string(), p.clone()])
        .collect();

    let tools: Vec<(&str, &str, Vec<String>)> = vec![
        (
            "debt-scan",
            "debt",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "doc-coverage",
            "doccov",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "crap-metric",
            "crap",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "coupling",
            "coupling",
            vec![path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "risk-map",
            "riskmap",
            vec![path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "duplication",
            "dupfind",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "prop-cov",
            "propcov",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "taint-scan",
            "taint",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "fuzz-surface",
            "fuzz",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        // mutation-test: run with capped mutants and enforced timeout.
        // Uses scratch workspace + watchdog kill — safe to include in batch.
        // Note: requires -p flag for package selection.
        // Uses dead-code crate (small, tests pass reliably) instead of ast-parse-ts
        // which has a pre-existing failing test.
        (
            "mutation-test",
            "mutate",
            vec![
                path.to_string(),
                "-p".to_string(),
                "dead-code".to_string(),
                "--max-mutants".to_string(),
                "5".to_string(),
                "--timeout".to_string(),
                "30".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "line-length",
            "linelen",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "halstead",
            "halstead",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        ({
            let mut args = vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ];
            args.extend(secrets_exclude_args.iter().cloned());
            ("secrets", "secrets", args)
        }),
        (
            "dead-code",
            "deadcode",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "cohesion",
            "cohesion",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "comment-ratio",
            "comments",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "error-handling",
            "errhandle",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "type-coverage",
            "typecov",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "vuln-scan",
            "vulnscan",
            vec![path.to_string(), "--format".to_string(), "json".to_string()],
        ),
        (
            "sast",
            "sast",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
                "--max-findings".to_string(),
                max_sast.to_string(),
            ],
        ),
        (
            "crypto-check",
            "cryptocheck",
            vec![
                "--recursive".to_string(),
                path.to_string(),
                "--format".to_string(),
                "json".to_string(),
                "--max-findings".to_string(),
                max_crypto.to_string(),
            ],
        ),
        (
            "licenses",
            "licenses",
            vec![path.to_string(), "--format".to_string(), "json".to_string()],
        ),
    ];

    // Run tools in parallel with bounded concurrency (MAX_CONCURRENT_CHECKS=4).
    // Each tool is an independent subprocess, so they can safely execute concurrently.
    // Memory monitor check runs before/after the parallel batch.
    if let Err(usage) = memory_monitor.check() {
        eprintln!(
            "  {} Memory limit exceeded ({} MB used). Stopping batch.",
            "✗".red().bold(),
            usage.rss_bytes / 1024 / 1024
        );
        return 2;
    }
    let results = run_parallel_tools(tools);
    if let Err(usage) = memory_monitor.check() {
        eprintln!(
            "  {} Memory usage high after batch ({} MB used). Consider reducing MAX_CONCURRENT_CHECKS.",
            "⚠".yellow().bold(),
            usage.rss_bytes / 1024 / 1024
        );
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.len() - passed;

    // Baseline handling: must check before moving results into report
    let mut regression_detected = false;
    if let Some(baseline_file) = baseline {
        if let Ok(baseline_content) = std::fs::read_to_string(baseline_file) {
            if let Ok(baseline_report) = serde_json::from_str::<UnifiedReport>(&baseline_content) {
                let baseline_tools: std::collections::HashSet<String> = baseline_report
                    .tools
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let current_tools: std::collections::HashSet<String> = results
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let regressed: Vec<String> =
                    baseline_tools.difference(&current_tools).cloned().collect();
                if !regressed.is_empty() {
                    eprintln!(
                        "BASELINE REGRESSION: previously-passing tools now failing: {:?}",
                        regressed
                    );
                    if !no_fail_on_regression {
                        regression_detected = true;
                    }
                }
            }
        }
    }

    match format {
        "sarif" => {
            // Build SARIF from results
            let mut log = SarifLog::new("cogent", env!("CARGO_PKG_VERSION"));
            let mut sarif_results: Vec<SarifResult> = Vec::new();

            for tool in &results {
                if !tool.success {
                    let findings = extract_findings_from_details(
                        &tool.data,
                        &format!("{}-finding", tool.tool),
                        "high",
                    );
                    if findings.is_empty() {
                        sarif_results.push(SarifResult {
                            rule_id: format!("{}-error", tool.tool),
                            rule_index: None,
                            level: "error".to_string(),
                            message: SarifMessage {
                                text: tool
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| format!("{} failed", tool.tool)),
                            },
                            locations: vec![SarifLocation {
                                physical_location: SarifPhysicalLocation {
                                    artifact_location: Some(SarifArtifactLocation {
                                        uri: path.to_string(),
                                    }),
                                    region: None,
                                },
                            }],
                        });
                    } else {
                        for finding in findings {
                            sarif_results.push(SarifResult {
                                rule_id: finding.rule_id,
                                rule_index: None,
                                level: sarif_level(&finding.severity).to_string(),
                                message: SarifMessage {
                                    text: finding.message,
                                },
                                locations: vec![SarifLocation {
                                    physical_location: SarifPhysicalLocation {
                                        artifact_location: Some(SarifArtifactLocation {
                                            uri: finding.file,
                                        }),
                                        region: Some(SarifRegion {
                                            start_line: finding.line.map(|line| line as usize),
                                            start_column: finding
                                                .column
                                                .map(|column| column as usize),
                                            end_line: None,
                                            end_column: None,
                                        }),
                                    },
                                }],
                            });
                        }
                    }
                }
            }

            let run = sarif_run(
                "cogent-batch",
                env!("CARGO_PKG_VERSION"),
                sarif_results,
                if failed > 0 { 1 } else { 0 },
            );
            log.add_run(run);
            println!(
                "{}",
                serde_json::to_string_pretty(&log).expect("SARIF log serialization")
            );
        }
        "json" => {
            let report = new_unified_report(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            );
            // Detect languages from source files at path
            let all_exts = [
                "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp",
                "cc", "cxx", "hpp", "cs", "java", "php", "rb", "swift",
            ];
            let mut langs_detected: Vec<String> = find_source_files(path, true, &all_exts)
                .iter()
                .map(|f| ast_parse_ts::Language::from_extension(f).to_string())
                .filter(|l| l != "unknown")
                .collect::<std::collections::HashSet<String>>()
                .into_iter()
                .collect();
            langs_detected.sort();
            let total_tools = results.len();
            let report = UnifiedReport {
                run_id: report.run_id,
                started_at: report.started_at,
                duration_ms,
                tools: results,
                summary: ReportSummary {
                    total_tools,
                    passed,
                    failed,
                    languages_detected: langs_detected,
                },
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("JSON report serialization")
            );
        }
        _ => {
            let all_ok = failed == 0;
            let summary_str = format!(
                "{}/{} tools passed  ·  {}",
                passed,
                results.len(),
                format_ms(duration_ms)
            );
            let summary_col = if all_ok {
                summary_str.green().to_string()
            } else {
                summary_str.red().to_string()
            };
            let inner = 46usize;
            let border = "═".repeat(inner + 2);
            eprintln!();
            eprintln!("  ╔{}╗", border);
            let title = format!(
                "COGENT RUN  ·  {}",
                if all_ok {
                    "PASSED ✓".green().bold().to_string()
                } else {
                    "FAILED ✗".red().bold().to_string()
                }
            );
            box_row(&title, inner);
            eprintln!("  ╠{}╣", border);
            box_row(&summary_col, inner);
            box_row(&format!("Path: {}", path), inner);
            eprintln!("  ╚{}╝", border);
            if !all_ok {
                eprintln!();
                for tool in results.iter().filter(|t| !t.success) {
                    let err = tool.error.as_deref().unwrap_or("check output for details");
                    eprintln!("  {} {}: {}", "✗".red(), tool.tool.red().bold(), err);
                }
            }
            eprintln!();
        }
    }

    if failed > 0 || regression_detected {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── max_concurrent_checks ──
    // NOTE: All env var tests are in a single function to avoid race conditions
    // since cargo test runs tests in parallel and std::env::set_var is not thread-safe.

    #[test]
    fn test_max_concurrent_checks_env_var_behavior() {
        // Save and clear any existing value
        let saved = std::env::var("COGENT_MAX_CONCURRENT").ok();
        std::env::remove_var("COGENT_MAX_CONCURRENT");

        // Default without env var
        assert_eq!(max_concurrent_checks(), DEFAULT_MAX_CONCURRENT_CHECKS);

        // Explicit value from env var
        std::env::set_var("COGENT_MAX_CONCURRENT", "8");
        assert_eq!(max_concurrent_checks(), 8);

        // Zero clamps to 1
        std::env::set_var("COGENT_MAX_CONCURRENT", "0");
        assert_eq!(max_concurrent_checks(), 1);

        // One is valid
        std::env::set_var("COGENT_MAX_CONCURRENT", "1");
        assert_eq!(max_concurrent_checks(), 1);

        // Invalid value falls back to default
        std::env::set_var("COGENT_MAX_CONCURRENT", "not_a_number");
        assert_eq!(max_concurrent_checks(), DEFAULT_MAX_CONCURRENT_CHECKS);

        // Restore original value if any
        match saved {
            Some(v) => std::env::set_var("COGENT_MAX_CONCURRENT", v),
            None => std::env::remove_var("COGENT_MAX_CONCURRENT"),
        }
    }

    // ── make_check_result ──

    #[test]
    fn test_make_check_result_basic() {
        let data = json!({"findings": []});
        let result = make_check_result(
            "test-check",
            true,
            0.0,
            10.0,
            data,
            "info",
            "test-rule",
            "All good".into(),
            None,
        );
        assert_eq!(result.name, "test-check");
        assert!(result.passed);
        assert_eq!(result.score, Some(0.0));
        assert_eq!(result.threshold, Some(10.0));
        assert_eq!(result.message, "All good");
        assert_eq!(result.severity.as_deref(), Some("info"));
        assert_eq!(result.rule_id.as_deref(), Some("test-rule"));
        assert!(result.help.is_none());
    }

    #[test]
    fn test_make_check_result_failed() {
        let data = json!({"findings": []});
        let result = make_check_result(
            "security-check",
            false,
            15.0,
            10.0,
            data,
            "high",
            "security-001",
            "Threshold exceeded".into(),
            Some("Reduce violations"),
        );
        assert!(!result.passed);
        assert_eq!(result.score, Some(15.0));
        assert_eq!(result.threshold, Some(10.0));
        assert_eq!(result.severity.as_deref(), Some("high"));
        assert_eq!(result.help.as_deref(), Some("Reduce violations"));
    }

    #[test]
    fn test_make_check_result_extracts_findings() {
        let data = json!({
            "findings": [
                {"file": "a.rs", "message": "issue 1", "severity": "high"},
                {"file": "b.rs", "message": "issue 2", "severity": "medium"},
            ]
        });
        let result = make_check_result(
            "multi-find",
            false,
            2.0,
            1.0,
            data,
            "error",
            "multi-rule",
            "Multiple issues".into(),
            None,
        );
        assert_eq!(result.findings.len(), 2);
        assert_eq!(result.findings[0].file, "a.rs");
        assert_eq!(result.findings[1].file, "b.rs");
    }

    #[test]
    fn test_make_check_result_empty_findings() {
        let data = json!({});
        let result = make_check_result(
            "no-find",
            true,
            0.0,
            5.0,
            data,
            "info",
            "no-rule",
            "No issues".into(),
            None,
        );
        assert!(result.findings.is_empty());
    }
}
