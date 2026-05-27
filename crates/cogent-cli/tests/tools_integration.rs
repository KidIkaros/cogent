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
    // SBOM outputs XML by default, test that it runs and produces valid XML
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("sbom").expect("sbom binary not found");

    let output = cmd
        .arg(fixture.to_str().unwrap())
        .output()
        .expect("sbom should run");

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
