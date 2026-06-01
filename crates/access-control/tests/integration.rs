use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::Path;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("access-control").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
    cmd.assert().stdout(contains("Access control checker"));
}

#[test]
fn test_json_output_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("test.py");
    fs::write(&src, "password = \"secret\"\n").unwrap();

    let mut cmd = Command::cargo_bin("access-control").unwrap();
    cmd.arg(src.to_str().unwrap());
    cmd.arg("--format");
    cmd.arg("json");
    cmd.arg("--max-violations");
    cmd.arg("100");
    cmd.assert().success();

    let stdout = String::from_utf8(cmd.output().unwrap().stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json.get("findings").is_some());
    assert!(json.get("summary").is_some());
    let findings = json["findings"].as_array().unwrap();
    assert!(!findings.is_empty());
    let first = &findings[0];
    assert!(first.get("rule_id").is_some());
    assert!(first.get("severity").is_some());
}

#[test]
fn test_table_output() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("test.py");
    fs::write(&src, "password = \"secret\"\n").unwrap();

    let mut cmd = Command::cargo_bin("access-control").unwrap();
    cmd.arg(src.to_str().unwrap());
    cmd.assert().failure();
    cmd.assert().stdout(contains("ACL-CRED"));
}

#[test]
fn test_threshold_message() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("test.py");
    fs::write(&src, "password = \"secret\"\n").unwrap();

    let mut cmd = Command::cargo_bin("access-control").unwrap();
    cmd.arg(src.to_str().unwrap());
    cmd.arg("--max-violations");
    cmd.arg("0");
    cmd.assert().failure();
    cmd.assert().stdout(contains("Exceeds threshold"));
}
