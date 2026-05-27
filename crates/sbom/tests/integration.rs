//! Integration tests for the sbom binary.

use assert_cmd::Command;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("sbom").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}

#[test]
fn test_cyclonedx_output() {
    let mut cmd = Command::cargo_bin("sbom").unwrap();
    cmd.arg("crates/fixtures");
    cmd.arg("--format");
    cmd.arg("cyclonedx");
    let output = cmd.output().expect("failed to run sbom");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("<bom"), "sbom should emit XML output");
}

#[test]
fn test_spdx_output() {
    let mut cmd = Command::cargo_bin("sbom").unwrap();
    cmd.arg("crates/fixtures");
    cmd.arg("--format");
    cmd.arg("spdx");
    let output = cmd.output().expect("failed to run sbom");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("SPDXVersion:"),
        "sbom should emit SPDX output"
    );
}
