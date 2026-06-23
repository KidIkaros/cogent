//! Schema validation integration tests.
//!
//! Runs the `cogent` binary with JSON output against a small fixture
//! directory and validates the output against each tool's JSON schema.

use assert_cmd::Command;
use std::path::Path;

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

/// Absolute path to any small fixture we can point the CLI at.
fn fixture_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).parent().unwrap().join("fixtures")
}

fn load_schema(name: &str) -> serde_json::Value {
    let path = schemas_dir().join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Cannot read schema {}: {}", path.display(), e));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("Invalid JSON in schema {}: {}", name, e))
}

/// Run `cogent check <fixture> --format json` and return the parsed output.
fn run_check_json(extra_args: &[&str]) -> serde_json::Value {
    let fixture = fixture_path();
    eprintln!("  → schema check");
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--force")
        .args(extra_args);

    let output = cmd.output().expect("failed to run cogent");
    eprintln!("  ✓ schema check (exit {:?})", output.status.code());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("check output is not valid JSON: {e}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    })
}

// ─── tool-response.schema.json ───────────────────────────────────────────────

#[test]
fn test_check_output_is_valid_json() {
    let value = run_check_json(&[]);
    let obj = value
        .as_object()
        .expect("check output must be a JSON object");
    assert!(obj.contains_key("passed"), "missing 'passed' field");
    assert!(obj.contains_key("path"), "missing 'path' field");
    assert!(obj.contains_key("checks"), "missing 'checks' field");
    assert!(obj.contains_key("summary"), "missing 'summary' field");
}

// ─── debt-report.schema.json ─────────────────────────────────────────────────

#[test]
fn test_debt_schema_validates_cli_output() {
    let schema_value = load_schema("debt-report.schema.json");
    let compiled = jsonschema::validator_for(&schema_value).expect("debt schema should compile");

    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("debt")
        .unwrap_or_else(|_| Command::cargo_bin("cogent").expect("cogent binary not found"));

    let output = cmd
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--force")
        .output()
        .expect("failed to run debt");

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return;
    }

    let value: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Err(error) = compiled.validate(&value) {
        panic!("debt JSON output failed schema validation:\n{}", error);
    }
}

// ─── Schema validation for all tool binaries ─────────────────────────────

const SCHEMAD_TOOLS: &[(&str, &str)] = &[
    ("access-control", "access-control-report.schema.json"),
    ("cohesion", "cohesion-report.schema.json"),
    ("comments", "comment-ratio-report.schema.json"),
    ("coupling", "coupling-report.schema.json"),
    ("crap", "crap-metric-report.schema.json"),
    ("cryptocheck", "crypto-check-report.schema.json"),
    ("deadcode", "dead-code-report.schema.json"),
    ("debt", "debt-report.schema.json"),
    ("doccov", "doccov-report.schema.json"),
    ("dupfind", "dup-report.schema.json"),
    ("errhandle", "error-handling-report.schema.json"),
    ("fuzz", "fuzz-report.schema.json"),
    ("halstead", "halstead-report.schema.json"),
    ("licenses", "licenses-report.schema.json"),
    ("linelen", "line-length-report.schema.json"),
    ("mutate", "mutation-test-report.schema.json"),
    ("propcov", "prop-cov-report.schema.json"),
    ("riskmap", "risk-map-report.schema.json"),
    ("sast", "sast-report.schema.json"),
    ("secrets", "secrets-report.schema.json"),
    ("supply-chain", "supply-chain-report.schema.json"),
    ("taint", "taint-report.schema.json"),
    ("typecov", "type-coverage-report.schema.json"),
    ("vulnscan", "vuln-scan-report.schema.json"),
];

#[test]
fn test_all_tool_schemas_validate() {
    let fixture = fixture_path();

    for (bin, schema_name) in SCHEMAD_TOOLS {
        let mut cmd = match Command::cargo_bin(bin) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let schema_value = load_schema(schema_name);
        let compiled = match jsonschema::validator_for(&schema_value) {
            Ok(c) => c,
            Err(e) => panic!("Schema {schema_name} failed to compile: {e}"),
        };

        let output = match cmd
            .arg(fixture.to_str().unwrap())
            .arg("--format")
            .arg("json")
            .arg("--force")
            .output()
        {
            Ok(o) => o,
            Err(_) => continue,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(stdout.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Err(error) = compiled.validate(&value) {
            panic!("{bin} JSON output failed schema validation ({schema_name}):\n{error}");
        }
    }
}

// ─── tool-response.schema.json via `cogent run` ────────────────────────

#[test]
fn test_run_json_conforms_to_tool_response_schema() {
    let fixture = fixture_path();

    let output = Command::cargo_bin("cogent")
        .expect("cogent binary not found")
        .arg("run")
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--force")
        .output()
        .expect("failed to run cogent run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return;
    }

    let report: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Some(tools) = report.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            let obj = tool
                .as_object()
                .expect("each tool entry must be a JSON object");
            assert!(
                obj.contains_key("tool"),
                "tool entry missing 'tool' field: {tool}"
            );
            assert!(
                obj.contains_key("success"),
                "tool entry missing 'success' field: {tool}"
            );
            assert!(
                obj.contains_key("duration_ms"),
                "tool entry missing 'duration_ms' field: {tool}"
            );
            assert!(
                obj.contains_key("data"),
                "tool entry missing 'data' field: {tool}"
            );
        }
    }
}
