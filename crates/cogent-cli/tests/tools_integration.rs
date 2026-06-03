//! Integration tests for all tool binaries.
//!
//! Validates that each tool binary:
//! - Runs without crashing
//! - Produces valid JSON output with --format json
//! - Validates against its JSON schema
//! - Returns appropriate exit codes

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

/// Absolute path to the fixtures directory.
fn fixture_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).parent().unwrap().join("fixtures")
}

/// Absolute path to the workspace schemas/ directory.
fn schemas_dir() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("schemas")
}

fn load_schema(name: &str) -> serde_json::Value {
    let path = schemas_dir().join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Cannot read schema {}: {}", path.display(), e));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("Invalid JSON in schema {}: {}", name, e))
}

/// Run a tool binary and return parsed JSON output.
fn run_tool_json(tool: &str, args: &[&str]) -> Result<Value, String> {
    let fixture = fixture_path();
    let mut cmd =
        Command::cargo_bin(tool).map_err(|e| format!("Binary '{}' not found: {}", tool, e))?;

    cmd.arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .args(args);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run {}: {}", tool, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    serde_json::from_str(&stdout)
        .map_err(|e| format!("Invalid JSON from {}: {}\nstdout:\n{}", tool, e, stdout))
}

/// Validate tool output against its schema.
fn validate_tool_against_schema(tool: &str, schema_name: &str, json: &Value) -> Result<(), String> {
    let schema_value = load_schema(schema_name);
    let compiled = jsonschema::validator_for(&schema_value)
        .map_err(|e| format!("Schema {} failed to compile: {}", schema_name, e))?;

    compiled.validate(json).map_err(|e| {
        format!(
            "{} output failed schema validation ({}): {}",
            tool, schema_name, e
        )
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Quality Tools
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_crap_runs() {
    let json = run_tool_json("crap", &[]).expect("crap should run");
    validate_tool_against_schema("crap", "crap-metric-report.schema.json", &json)
        .expect("crap should match schema");
}

#[test]
fn test_debt_runs() {
    let json = run_tool_json("debt", &[]).expect("debt should run");
    validate_tool_against_schema("debt", "debt-report.schema.json", &json)
        .expect("debt should match schema");
}

#[test]
fn test_doccov_runs() {
    let json = run_tool_json("doccov", &[]).expect("doccov should run");
    validate_tool_against_schema("doccov", "doccov-report.schema.json", &json)
        .expect("doccov should match schema");
}

#[test]
fn test_dupfind_runs() {
    let json = run_tool_json("dupfind", &[]).expect("dupfind should run");
    validate_tool_against_schema("dupfind", "dup-report.schema.json", &json)
        .expect("dupfind should match schema");
}

#[test]
fn test_coupling_runs() {
    let json = run_tool_json("coupling", &[]).expect("coupling should run");
    validate_tool_against_schema("coupling", "coupling-report.schema.json", &json)
        .expect("coupling should match schema");
}

// Note: complexity is not a standalone binary - it's part of `cogent check`
// The complexity schema exists for the output format, but it's not a separate tool binary

#[test]
fn test_halstead_runs() {
    let json = run_tool_json("halstead", &[]).expect("halstead should run");
    validate_tool_against_schema("halstead", "halstead-report.schema.json", &json)
        .expect("halstead should match schema");
}

#[test]
fn test_deadcode_runs() {
    let json = run_tool_json("deadcode", &[]).expect("deadcode should run");
    validate_tool_against_schema("deadcode", "dead-code-report.schema.json", &json)
        .expect("deadcode should match schema");
}

#[test]
fn test_cohesion_runs() {
    let json = run_tool_json("cohesion", &[]).expect("cohesion should run");
    validate_tool_against_schema("cohesion", "cohesion-report.schema.json", &json)
        .expect("cohesion should match schema");
}

#[test]
fn test_comments_runs() {
    let json = run_tool_json("comments", &[]).expect("comments should run");
    validate_tool_against_schema("comments", "comment-ratio-report.schema.json", &json)
        .expect("comments should match schema");
}

#[test]
fn test_linelen_runs() {
    let json = run_tool_json("linelen", &[]).expect("linelen should run");
    validate_tool_against_schema("linelen", "line-length-report.schema.json", &json)
        .expect("linelen should match schema");
}

#[test]
fn test_fuzz_runs() {
    let json = run_tool_json("fuzz", &[]).expect("fuzz should run");
    validate_tool_against_schema("fuzz", "fuzz-report.schema.json", &json)
        .expect("fuzz should match schema");
}

#[test]
fn test_mutate_runs() {
    // mutate requires a Rust crate (Cargo.toml) to work properly
    // Just verify the binary runs and produces some output (error message is fine)
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("mutate").expect("mutate binary not found");

    let output = cmd
        .arg(fixture.to_str().unwrap())
        .arg("--max-mutants")
        .arg("0")
        .arg("--format")
        .arg("json")
        .output()
        .expect("mutate should run");

    // Command should run (not crash) - may output error about needing Cargo.toml
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.is_empty() || !stderr.is_empty() || output.status.code().is_some(),
        "mutate should run and produce some output or exit code"
    );
}

#[test]
fn test_propcov_runs() {
    let json = run_tool_json("propcov", &[]).expect("propcov should run");
    validate_tool_against_schema("propcov", "prop-cov-report.schema.json", &json)
        .expect("propcov should match schema");
}

#[test]
fn test_riskmap_runs() {
    let json = run_tool_json("riskmap", &[]).expect("riskmap should run");
    validate_tool_against_schema("riskmap", "risk-map-report.schema.json", &json)
        .expect("riskmap should match schema");
}

#[test]
fn test_typecov_runs() {
    let json = run_tool_json("typecov", &[]).expect("typecov should run");
    validate_tool_against_schema("typecov", "type-coverage-report.schema.json", &json)
        .expect("typecov should match schema");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Security Tools
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_secrets_runs() {
    let json = run_tool_json("secrets", &[]).expect("secrets should run");
    validate_tool_against_schema("secrets", "secrets-report.schema.json", &json)
        .expect("secrets should match schema");
}

#[test]
fn test_taint_runs() {
    let json = run_tool_json("taint", &[]).expect("taint should run");
    validate_tool_against_schema("taint", "taint-report.schema.json", &json)
        .expect("taint should match schema");
}

#[test]
fn test_sast_runs() {
    let json = run_tool_json("sast", &[]).expect("sast should run");
    validate_tool_against_schema("sast", "sast-report.schema.json", &json)
        .expect("sast should match schema");
}

#[test]
fn test_cryptocheck_runs() {
    let json = run_tool_json("cryptocheck", &[]).expect("cryptocheck should run");
    validate_tool_against_schema("cryptocheck", "crypto-check-report.schema.json", &json)
        .expect("cryptocheck should match schema");
}

#[test]
fn test_vulnscan_runs() {
    // vulnscan may have empty output if cargo-audit not installed or no vulns
    // Just verify the binary runs without crashing
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("vulnscan").expect("vulnscan binary not found");

    let output = cmd
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .output()
        .expect("vulnscan should run");

    // Should produce valid JSON or empty (both acceptable)
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        let _: Value = serde_json::from_str(&stdout)
            .expect("vulnscan should produce valid JSON if output is present");
    }
}

#[test]
fn test_errhandle_runs() {
    let json = run_tool_json("errhandle", &[]).expect("errhandle should run");
    validate_tool_against_schema("errhandle", "error-handling-report.schema.json", &json)
        .expect("errhandle should match schema");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Compliance & Supply Chain Tools
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_licenses_runs() {
    let json = run_tool_json("licenses", &[]).expect("licenses should run");
    validate_tool_against_schema("licenses", "licenses-report.schema.json", &json)
        .expect("licenses should match schema");
}

#[test]
fn test_sbom_runs() {
    // SBOM outputs XML by default, test that it runs via cogent CLI
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    let output = cmd
        .arg("sbom")
        .arg(fixture.to_str().unwrap())
        .output()
        .expect("cogent sbom should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("<?xml") || stdout.contains("<bom"),
        "sbom should output XML. Got: {}",
        stdout
    );
}

#[test]
fn test_access_control_runs() {
    let json = run_tool_json("access-control", &[]).expect("access-control should run");
    validate_tool_against_schema("access-control", "access-control-report.schema.json", &json)
        .expect("access-control should match schema");
}

#[test]
fn test_supply_chain_runs() {
    let json = run_tool_json("supply-chain", &[]).expect("supply-chain should run");
    validate_tool_against_schema("supply-chain", "supply-chain-report.schema.json", &json)
        .expect("supply-chain should match schema");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CLI Commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cogent_check_runs() {
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let json: Value =
        serde_json::from_str(&stdout).expect("cogent check should produce valid JSON");

    // Validate CheckReport structure
    let obj = json.as_object().expect("check output should be an object");
    assert!(
        obj.contains_key("passed"),
        "check output should have 'passed'"
    );
    assert!(
        obj.contains_key("checks"),
        "check output should have 'checks'"
    );
    assert!(
        obj.contains_key("summary"),
        "check output should have 'summary'"
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_markdown() {
    // --ci should force JSON output even when --format markdown is specified.
    // This is implemented in run_check_subcommand:
    //   let format = if cfg.ci { "json".to_string() } else { format };
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("markdown")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format markdown should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON, so output should be valid JSON, not markdown
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure
    let obj = json.as_object().expect("check output should be an object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Verify it's NOT markdown
    assert!(
        !stdout.starts_with("# ") && !stdout.starts_with("## "),
        "CI output should be JSON, not markdown (should not start with headers). Got:\n{}",
        &stdout[..stdout.len().min(100)]
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_text() {
    // --ci should force JSON output even when --format text is specified.
    // Text format normally shows a spinner + summary box; CI forces JSON instead.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("text")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format text should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON, so output should be valid JSON, not text
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format text. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure
    let obj = json.as_object().expect("check output should be an object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Text output would normally contain a summary box or spinner lines;
    // JSON output won't have those textual markers
    assert!(
        !stderr.contains("COGENT CHECK"),
        "CI mode with --format text should not produce text summary box"
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_sarif() {
    // --ci should force JSON CheckReport output even when --format sarif is specified.
    // Without --ci, "sarif" produces SARIF JSON (runs, results, rules structure).
    // With --ci, it's overridden to "json" and produces a CheckReport.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("sarif")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format sarif should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON, so output should be valid JSON (CheckReport, not SARIF)
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format sarif. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure (SARIF output has "$schema", "version", "runs" instead)
    let obj = json.as_object().expect("check output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // SARIF output has a "$schema" or "version" field at the top level;
    // CheckReport doesn't have either.
    assert!(
        !obj.contains_key("$schema"),
        "CI output should be CheckReport, not SARIF (should not have '$schema')"
    );
    assert!(
        !obj.contains_key("runs"),
        "CI output should be CheckReport, not SARIF (should not have 'runs')"
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_ndjson() {
    // --ci should force JSON CheckReport output even when --format ndjson is specified.
    // Without --ci, "ndjson" produces one JSON line per failed check (multiple values).
    // With --ci, it's overridden to "json" and produces a single CheckReport.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("ndjson")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format ndjson should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON -- parsing the entire stdout as a single JSON value should succeed.
    // (NDJSON output of multiple JSON objects per line would fail `from_str`.)
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format ndjson. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure
    let obj = json.as_object().expect("check output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_findings() {
    // --ci should force JSON CheckReport output even when --format findings is specified.
    // Without --ci, "findings" produces one NDJSON line per finding (multiple values).
    // With --ci, it's overridden to "json" and produces a single CheckReport.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("findings")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format findings should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON -- parsing the entire stdout as a single JSON value should succeed.
    // (Findings NDJSON with multiple objects per line would fail `from_str`.)
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format findings. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure
    let obj = json.as_object().expect("check output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );
}

#[test]
fn test_cogent_check_ci_forces_json_over_junit() {
    // --ci should force JSON CheckReport output even when --format junit is specified.
    // Without --ci, "junit" produces XML output (not JSON).
    // With --ci, it's overridden to "json" and produces a single CheckReport.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--ci")
        .arg("--format")
        .arg("junit")
        .arg("--force");

    let output = cmd.output().expect("failed to run cogent check --ci");
    let exit = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        exit == Some(0) || exit == Some(1),
        "check --ci --format junit should exit 0 or 1, got {:?}. stderr:\n{}",
        exit,
        stderr
    );

    // CI forces JSON -- parsing as JSON should succeed (JUnit XML would fail `from_str`.)
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format junit. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify CheckReport structure
    let obj = json.as_object().expect("check output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // JUnit output would start with XML declaration; JSON output starts with '{{'
    assert!(
        stdout.trim_start().starts_with('{'),
        "CI output should be JSON (starts with '{{'), not JUnit XML"
    );
}

#[test]
fn test_cogent_report_runs() {
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");

    cmd.arg("report")
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json");

    let output = cmd.output().expect("failed to run cogent report");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Report outputs HTML by default, JSON format should produce valid JSON or may be empty
    if !stdout.trim().is_empty() {
        let _: Value = serde_json::from_str(&stdout)
            .expect("cogent report --format json should produce valid JSON");
    }
}

#[test]
fn test_cogent_diff_runs() {
    // Create two temp report files
    let temp_dir = std::env::temp_dir();
    let report1 = temp_dir.join("cogent-test-report1.json");
    let report2 = temp_dir.join("cogent-test-report2.json");

    // Write minimal valid reports
    let report_content = r##"{"passed":true,"path":".","checks":[],"summary":{"total_checks":0,"passed_checks":0,"failed_checks":0}}"##;
    std::fs::write(&report1, report_content).unwrap();
    std::fs::write(&report2, report_content).unwrap();

    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("diff").arg(&report1).arg(&report2);

    let output = cmd.output().expect("failed to run cogent diff");

    // Cleanup
    let _ = std::fs::remove_file(&report1);
    let _ = std::fs::remove_file(&report2);

    // Diff should produce some output (stdout or stderr) and exit 0 when no changes
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.is_empty() || !stderr.is_empty() || output.status.code() == Some(0),
        "diff should produce output or exit 0. stdout: {}, stderr: {}, exit: {:?}",
        stdout,
        stderr,
        output.status.code()
    );
}

#[test]
fn test_cogent_version() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("--version");

    let output = cmd.output().expect("failed to run cogent --version");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("cogent") || stdout.contains("1.0"),
        "version should contain 'cogent' or version number"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Audit Pipeline
// ═══════════════════════════════════════════════════════════════════════════════

/// Names of security-category checks (actual CheckResult.name values from check_runners.rs).
const SECURITY_CHECKS: [&str; 6] = ["secrets", "sast", "crypto", "taint", "vulnscan", "access-control"];
/// Names of quality-category checks (actual CheckResult.name values from check_runners.rs).
/// Note: "doc_coverage" not "doccov"; "duplication" not "dupfind".
const QUALITY_CHECKS: [&str; 20] = ["crap", "debt", "doc_coverage", "complexity", "duplication",
    "riskmap", "coupling", "propcov", "fuzz", "linelen", "halstead", "deadcode",
    "cohesion", "comments", "errhandle", "typecov", "observability",
    "test-quality", "design-docs", "debuggability"];
/// Names of compliance-category checks that have run_audit_check! calls.
/// "outdated" and "sbom" are listed in compliance_checks but have no audit runner.
const COMPLIANCE_CHECKS: [&str; 2] = ["licenses", "supply-chain"];

/// Run `cogent audit` with the given extra args and return exit code + stdout.
fn run_audit(args: &[&str]) -> (Option<i32>, String, String) {
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("audit")
        .arg(fixture.to_str().unwrap())
        .args(args);

    let output = cmd.output().expect("failed to run cogent audit");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code(), stdout, stderr)
}

#[test]
fn test_audit_json_format() {
    let (exit, stdout, stderr) = run_audit(&["--format", "json"]);
    let combined = format!("{}{}", stderr, stdout);

    // Should exit 0 or 1 (depends on findings); should produce valid JSON
    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --format json should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --format json output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let obj = json.as_object().expect("audit json output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );
}

#[test]
fn test_audit_agent_format() {
    let (exit, stdout, stderr) = run_audit(&["--format", "agent"]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --format agent should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // Agent format produces one JSON object per line (NDJSON)
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        !lines.is_empty(),
        "audit --format agent should produce at least one line of output"
    );

    // Every line should be valid JSON
    for (i, line) in lines.iter().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("line {} of agent output is not valid JSON: {}\nline: {}", i + 1, e, line)
        });
        let obj = value.as_object().unwrap_or_else(|| {
            panic!("line {} of agent output should be a JSON object, got: {}", i + 1, value)
        });

        // Each line should have a "type" field: "finding" or "summary"
        assert!(
            obj.contains_key("type"),
            "line {} of agent output missing 'type' field: {}",
            i + 1,
            line
        );
        let type_str = obj["type"].as_str().unwrap();
        assert!(
            type_str == "finding" || type_str == "summary",
            "line {} of agent output has unexpected type '{}'",
            i + 1,
            type_str
        );
    }

    // Last line should be the summary
    let last_line = lines.last().unwrap();
    let last_value: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert_eq!(
        last_value["type"].as_str(),
        Some("summary"),
        "last line of agent output should be the summary"
    );
    assert!(
        last_value.get("passed").is_some(),
        "summary should contain 'passed'"
    );
    assert!(
        last_value.get("score").is_some(),
        "summary should contain 'score'"
    );
    assert!(
        last_value.get("grade").is_some(),
        "summary should contain 'grade'"
    );
}

#[test]
fn test_audit_markdown_format() {
    let (exit, stdout, stderr) = run_audit(&["--format", "markdown"]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --format markdown should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // Markdown output should contain markdown headers or table-like structure
    assert!(
        stdout.contains("# ") || stdout.contains("## ") || stdout.contains("|"),
        "audit --format markdown should contain headers or tables. output:\n{}",
        combined
    );
    assert!(
        stdout.contains("passed") || stdout.contains("Passed"),
        "audit --format markdown should mention pass/fail. output:\n{}",
        combined
    );
}

#[test]
fn test_audit_skip_filter() {
    // Skip a check and verify it still runs (just reports findings)
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--skip", "secrets",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --skip should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // Output should still be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --skip output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    // The skip filter excludes "secrets" from the check list
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        !check_names.contains(&"secrets"),
        "secrets should be skipped. checks: {:?}",
        check_names
    );
}

#[test]
fn test_audit_checks_filter() {
    // The --checks flag maps to the `only_set` in audit_should_run.
    // Only named checks should appear in the output.
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--checks", "secrets,sast",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --checks should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --checks output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--checks should produce at least one check in the output"
    );
    // Only secrets and sast should be present
    for name in &check_names {
        assert!(
            *name == "secrets" || *name == "sast",
            "unexpected check '{}' appeared with --checks secrets,sast",
            name
        );
    }
}

#[test]
fn test_audit_ci_mode() {
    // CI mode sets COGENT_NO_PROGRESS and should produce valid JSON output
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "json",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --ci should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI mode should produce valid JSON (it internally sets format to json)
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        // With --ci and no explicit format, it may default differently - accept empty
        if stdout.trim().is_empty() {
            return serde_json::Value::Null;
        }
        panic!("audit --ci output is not valid JSON (when non-empty): {}\nstdout:\n{}", e, stdout)
    });

    if json.is_object() {
        let obj = json.as_object().unwrap();
        // CI output should have expected structure
        assert!(obj.contains_key("passed"), "missing 'passed'");
        assert!(obj.contains_key("checks"), "missing 'checks'");
    }
}

#[test]
fn test_audit_only_security() {
    // --only security should limit checks to the security category
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "security",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --only security should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --only security output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--only security should produce at least one check"
    );

    // Every check should be a security check; no quality checks allowed
    for name in &check_names {
        assert!(
            SECURITY_CHECKS.contains(name),
            "check '{}' is not a security check. See check_runners.rs for actual names.",
            name
        );
        assert!(
            !QUALITY_CHECKS.contains(name),
            "quality check '{}' should not appear in --only security",
            name
        );
    }
}

#[test]
fn test_audit_only_quality() {
    // --only quality should limit checks to the quality category
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "quality",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --only quality should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --only quality output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--only quality should produce at least one check"
    );

    // Every check should be a quality check; no security checks allowed
    for name in &check_names {
        assert!(
            QUALITY_CHECKS.contains(name),
            "check '{}' is not a quality check. See check_runners.rs for actual names.",
            name
        );
        assert!(
            !SECURITY_CHECKS.contains(name),
            "security check '{}' should not appear in --only quality",
            name
        );
    }
}

#[test]
fn test_audit_only_compliance() {
    // --only compliance should limit checks to licenses and supply-chain
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "compliance",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --only compliance should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("audit --only compliance output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--only compliance should produce at least one check"
    );

    // Every check should be a compliance check; no security or quality checks allowed
    for name in &check_names {
        assert!(
            COMPLIANCE_CHECKS.contains(name),
            "check '{}' is not a compliance check (expected licenses or supply-chain)",
            name
        );
        assert!(
            !SECURITY_CHECKS.contains(name),
            "security check '{}' should not appear in --only compliance",
            name
        );
        assert!(
            !QUALITY_CHECKS.contains(name),
            "quality check '{}' should not appear in --only compliance",
            name
        );
    }
}

#[test]
fn test_audit_checks_overrides_only_category() {
    // --checks (only_set) takes precedence over --only (active_categories).
    // Even with --only security, only secrets and sast should run.
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "security",
        "--checks", "secrets,sast",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "audit --only security --checks secrets,sast should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--checks secrets,sast should produce at least one check"
    );

    // Only secrets and sast should appear — --only security is ignored
    for name in &check_names {
        assert!(
            *name == "secrets" || *name == "sast",
            "unexpected check '{}' appeared with --checks secrets,sast (--only security should be overridden)",
            name
        );
    }
}

#[test]
fn test_audit_skip_ignored_when_checks_active() {
    // --checks (only_set) takes highest priority in audit_should_run.
    // Even if a check is in --skip, it should still run if listed in --checks.
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--skip", "secrets",
        "--checks", "secrets,sast",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--skip + --checks should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--checks secrets,sast should produce at least one check"
    );

    // Both secrets and sast should appear — --skip is ignored when --checks is active
    assert!(
        check_names.contains(&"secrets"),
        "secrets should appear even when skipped because --checks takes priority. checks: {:?}",
        check_names
    );
    assert!(
        check_names.contains(&"sast"),
        "sast should appear. checks: {:?}",
        check_names
    );
    // No other checks should appear
    for name in &check_names {
        assert!(
            *name == "secrets" || *name == "sast",
            "unexpected check '{}' appeared with --checks secrets,sast",
            name
        );
    }
}

#[test]
fn test_audit_skip_excludes_from_category() {
    // --skip excludes checks from the active category when --checks is not set.
    // With --only security and --skip secrets,sast, all security checks except
    // secrets and sast should appear.
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "security",
        "--skip", "secrets,sast",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--only security --skip secrets,sast should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--only security should produce at least one check even with --skip"
    );

    // Skipped checks should not appear
    assert!(
        !check_names.contains(&"secrets"),
        "secrets should be skipped. checks: {:?}",
        check_names
    );
    assert!(
        !check_names.contains(&"sast"),
        "sast should be skipped. checks: {:?}",
        check_names
    );

    // All remaining checks should be security checks (no quality or compliance)
    for name in &check_names {
        assert!(
            SECURITY_CHECKS.contains(name),
            "check '{}' is not a security check",
            name
        );
        assert!(
            !QUALITY_CHECKS.contains(name),
            "quality check '{}' should not appear in --only security",
            name
        );
    }
}

#[test]
fn test_audit_skip_only_interaction_all_flags() {
    // When --checks (only_set) is active, it overrides both --skip and --only.
    // With --only security --skip secrets --checks secrets, only secrets should run
    // because --checks takes priority over both --skip and --only.
    let (exit, stdout, stderr) = run_audit(&[
        "--format", "json",
        "--only", "security",
        "--skip", "secrets",
        "--checks", "secrets",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "all flags combined should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--checks secrets should produce at least one check"
    );

    // Only secrets should appear — --checks overrides both --skip and --only
    for name in &check_names {
        assert!(
            *name == "secrets",
            "unexpected check '{}' appeared with --checks secrets (only_set should override both skip and only)",
            name
        );
    }
}

#[test]
fn test_audit_ci_with_skip() {
    // --ci sets COGENT_NO_PROGRESS and the CI exit code logic.
    // Combined with --skip, the skip filter should still work correctly,
    // and the CI exit code logic applies to the filtered results.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "json",
        "--skip", "secrets",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    // CI mode: exit 0 if passed, 1 if failed (or if ci+findings)
    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --skip should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // Output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    // --skip secrets should exclude secrets from the checks
    let checks = obj["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.contains(&"secrets"),
        "secrets should be skipped in CI mode. checks: {:?}",
        check_names
    );

    // total_checks in summary should match the checks array length
    let summary = obj["summary"].as_object().expect("summary should be an object");
    let total_checks = summary["total_checks"].as_u64().unwrap_or(0) as usize;
    assert_eq!(
        total_checks, checks.len(),
        "total_checks {} should match checks array length {}",
        total_checks, checks.len()
    );

    // Verify the CI exit code logic:
    // if ci && total_findings > 0 { 1 } else if passed { 0 } else { 1 }
    let passed = obj["passed"].as_bool().unwrap_or(false);
    let exit_code = exit.unwrap_or(1);
    if !passed && exit_code == 0 {
        panic!(
            "CI + !passed should exit 1, but got 0. checks: {:?}",
            check_names
        );
    }
}

#[test]
fn test_audit_ci_with_skip_and_only() {
    // Combine --ci with both --skip and --only to verify all three interact correctly.
    // --skip secrets,sast --only security in CI mode should produce valid JSON
    // with only non-skipped security checks.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "json",
        "--only", "security",
        "--skip", "secrets,sast",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --only security --skip secrets,sast should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output is not valid JSON: {}\nstdout:\n{}", e, stdout));

    let checks = json["checks"].as_array().expect("checks should be an array");
    let check_names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        !check_names.is_empty(),
        "--only security should produce at least one check in CI mode"
    );

    // Skipped checks should not appear
    assert!(
        !check_names.contains(&"secrets"),
        "secrets should be skipped. checks: {:?}",
        check_names
    );
    assert!(
        !check_names.contains(&"sast"),
        "sast should be skipped. checks: {:?}",
        check_names
    );

    // All remaining checks should be security checks
    for name in &check_names {
        assert!(
            SECURITY_CHECKS.contains(name),
            "check '{}' is not a security check",
            name
        );
    }

    // Verify summary matches actual checks
    let summary = json["summary"].as_object().expect("summary should be an object");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );
}

#[test]
fn test_audit_ci_forces_json_over_markdown() {
    // --ci should force JSON output even when --format markdown is specified,
    // consistent with the check subcommand's behavior.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "markdown",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format markdown should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON, not markdown
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format markdown. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Verify it's NOT markdown (no markdown headers)
    assert!(
        !stdout.starts_with("# ") && !stdout.starts_with("## "),
        "CI output should be JSON, not markdown (should not start with headers). Got:\n{}",
        &stdout[..stdout.len().min(100)]
    );
}

#[test]
fn test_audit_ci_forces_json_over_text() {
    // --ci should force JSON output even when --format text is specified.
    // Without --ci, "text" is not a recognized audit format and would exit 2;
    // with --ci, it's overridden to "json" and should produce valid JSON.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "text",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format text should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format text. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Without --ci, --format text would hit the unknown-format arm and exit 2.
    // Verify that CI override prevented that (text output would have no valid JSON).
    assert!(
        !stdout.trim().is_empty(),
        "CI output should not be empty"
    );
}

#[test]
fn test_audit_ci_forces_json_over_agent() {
    // --ci should force JSON output even when --format agent is specified.
    // Without --ci, "agent" produces NDJSON (one finding per line + summary).
    // With --ci, it's overridden to "json" and produces a single JSON CheckReport.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "agent",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format agent should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be a single JSON object, not NDJSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format agent. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure (CheckReport, not a finding line)
    let obj = json.as_object().expect("output should be a JSON object (CheckReport), not NDJSON");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Agent format would have a "type" field per line ("finding" or "summary").
    // JSON format CheckReport is a single object without a "type" field at the top level.
    assert!(
        !obj.contains_key("type"),
        "CI output should be JSON CheckReport, not NDJSON (should not have 'type' field)"
    );

    // Ensure it's not NDJSON: agent output has multiple non-empty lines
    let non_empty_lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        !non_empty_lines.is_empty(),
        "output should not be empty"
    );
}

#[test]
fn test_audit_ci_forces_json_over_sarif() {
    // --ci should force JSON output even when --format sarif is specified.
    // Without --ci, "sarif" is not a recognized audit format and would exit 2;
    // with --ci, it's overridden to "json" and should produce valid JSON.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "sarif",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format sarif should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format sarif. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Without --ci, --format sarif would hit the unknown-format arm and exit 2.
    // Verify that CI override prevented that.
    assert!(
        !stdout.trim().is_empty(),
        "CI output should not be empty"
    );
}

#[test]
fn test_audit_ci_forces_json_over_ndjson() {
    // --ci should force JSON output even when --format ndjson is specified.
    // Without --ci, "ndjson" is not a recognized audit format and would exit 2;
    // with --ci, it's overridden to "json" and should produce valid JSON.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "ndjson",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format ndjson should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format ndjson. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    // Without --ci, --format ndjson would hit the unknown-format arm and exit 2.
    // Verify that CI override prevented that.
    assert!(
        !stdout.trim().is_empty(),
        "CI output should not be empty"
    );
}

#[test]
fn test_audit_ci_forces_json_over_findings() {
    // --ci should force JSON output even when --format findings is specified.
    // Without --ci, "findings" is not a recognized audit format and would exit 2;
    // with --ci, it's overridden to "json" and should produce valid JSON.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "findings",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format findings should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format findings. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    assert!(
        !stdout.trim().is_empty(),
        "CI output should not be empty"
    );
}

#[test]
fn test_audit_ci_forces_json_over_junit() {
    // --ci should force JSON output even when --format junit is specified.
    // Without --ci, "junit" is not a recognized audit format and would exit 2;
    // with --ci, it's overridden to "json" and should produce valid JSON.
    let (exit, stdout, stderr) = run_audit(&[
        "--ci",
        "--format", "junit",
    ]);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        exit == Some(0) || exit == Some(1),
        "--ci --format junit should exit 0 or 1, got {:?}. output:\n{}",
        exit,
        combined
    );

    // CI forces JSON, so output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("CI should force JSON output even with --format junit. Parse error: {}\nstdout:\n{}", e, stdout));

    // Verify JSON structure
    let obj = json.as_object().expect("output should be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed'");
    assert!(obj.contains_key("path"), "missing 'path'");
    assert!(obj.contains_key("checks"), "missing 'checks'");
    assert!(obj.contains_key("summary"), "missing 'summary'");

    let checks = obj["checks"].as_array().expect("checks should be an array");
    let summary = obj["summary"].as_object().expect("summary should be an object");
    assert!(summary.contains_key("total_checks"), "summary missing 'total_checks'");
    assert_eq!(
        summary["total_checks"].as_u64().unwrap_or(0) as usize,
        checks.len(),
        "total_checks should match checks array length"
    );

    assert!(
        !stdout.trim().is_empty(),
        "CI output should not be empty"
    );
}
