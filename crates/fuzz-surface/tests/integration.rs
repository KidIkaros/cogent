//! Integration tests for the fuzz binary.

use assert_cmd::Command;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("fuzz").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}

#[test]
fn test_json_output_is_valid() {
    let mut cmd = Command::cargo_bin("fuzz").unwrap();
    cmd.arg("crates/fixtures");
    cmd.arg("--format");
    cmd.arg("json");
    let output = cmd.output().expect("failed to run fuzz");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        let _: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("fuzz JSON output should be valid");
    }
    // Tool may exit 0 or 1 depending on findings; just verify it runs
}
