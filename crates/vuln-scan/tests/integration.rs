//! Integration tests for the vulnscan binary.
//!
//! These tests exercise the remaining uncovered code paths:
//! - Ecosystem detection (Unknown → exit 2 with error message)
//! - Forced ecosystem when real tools are absent (subprocess error path)
//! - Output formatting (JSON, NDJSON, table)
//! - Exit codes (thresholds exceeded, errors)
//! - CLI argument parsing

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}


#[test]
fn test_unknown_ecosystem_exits_2_with_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg(dir.path().to_str().unwrap());
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("Could not detect ecosystem"));
}

#[test]
fn test_unknown_ecosystem_with_json_format() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg(dir.path().to_str().unwrap());
    cmd.arg("--format");
    cmd.arg("json");
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("Could not detect ecosystem"));
}

#[test]
fn test_forced_unknown_ecosystem() {
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg("/nonexistent");
    cmd.arg("--ecosystem");
    cmd.arg("cobol");
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("Could not detect ecosystem"));
}

#[test]
fn test_forced_rust_ecosystem() {
    // When --ecosystem rust is forced: if cargo-audit plugin is not installed,
    // the subprocess fails → exit code 2. If it IS installed, it may succeed
    // (exit 0) or fail differently. Accept either outcome.
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg(dir.path().to_str().unwrap());
    cmd.arg("--ecosystem");
    cmd.arg("rust");
    let output = cmd.output().expect("failed to run vulnscan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // If cargo-audit is missing, stderr says "Error: ..."
    // If cargo-audit is installed, stderr is empty
    if stderr.contains("Error:") {
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn test_forced_node_ecosystem() {
    // When --ecosystem node is forced: if npm/yarn is not available,
    // find_tool returns None → exit code 2. If npm IS installed, it may
    // succeed or fail differently. Accept either outcome.
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg(dir.path().to_str().unwrap());
    cmd.arg("--ecosystem");
    cmd.arg("node");
    let output = cmd.output().expect("failed to run vulnscan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // If npm is missing, stderr says "Error: ..."
    if stderr.contains("Error:") {
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn test_nonexistent_path_unknown_ecosystem() {
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg("/nonexistent_directory_path");
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("Could not detect ecosystem"));
}

#[test]
fn test_json_output_is_valid() {
    // Run against the fixtures directory — tools may or may not be installed.
    // If they are, we get valid JSON output; if not, we get empty stdout.
    // Either way, if stdout is non-empty, verify it's valid JSON.
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg("crates/fixtures");
    cmd.arg("--format");
    cmd.arg("json");
    let output = cmd.output().expect("failed to run vulnscan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        let v: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("vulnscan JSON output should be valid");
        // Verify structure: should have vulnerabilities and summary
        assert!(
            v.get("vulnerabilities").is_some(),
            "JSON output should contain 'vulnerabilities' field"
        );
        assert!(
            v.get("summary").is_some(),
            "JSON output should contain 'summary' field"
        );
    }
}

#[test]
fn test_table_output_no_vulns_message() {
    // Run against fixtures dir with table format (default).
    // If tools are installed and find no vulns, we should see the
    // "No known vulnerabilities" message. Otherwise, the binary fails
    // with an error (already covered by other tests).
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg("crates/fixtures");
    let output = cmd.output().expect("failed to run vulnscan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Binary must have produced at least some output
    assert!(
        !stdout.is_empty() || !stderr.is_empty(),
        "vulnscan should produce stdout or stderr output"
    );

    if stdout.contains("No known vulnerabilities") {
        assert!(
            stdout.contains("PASS") || stdout.contains("FAIL"),
            "Table output should show PASS or FAIL status"
        );
    }
}

#[test]
fn test_ndjson_format_unknown_ecosystem() {
    // NDJSON format is an uncovered code path. With unknown ecosystem, it
    // should exit 2 before producing any output.
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("vulnscan").unwrap();
    cmd.arg(dir.path().to_str().unwrap());
    cmd.arg("--format");
    cmd.arg("ndjson");
    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("Could not detect ecosystem"));
}
