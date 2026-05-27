use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("supply-chain").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
    cmd.assert().stdout(contains("Supply chain checker"));
}

#[test]
fn test_json_output_schema() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = 'test'\n").unwrap();
    fs::write(
        tmp.path().join("Cargo.lock"),
        "[[package]]\nname = 'serde'\nversion = '1.0'\nchecksum = 'abc'\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("supply-chain").unwrap();
    cmd.arg(tmp.path().to_str().unwrap());
    cmd.arg("--format");
    cmd.arg("json");
    cmd.assert().success();

    let stdout = String::from_utf8(cmd.output().unwrap().stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json.get("findings").is_some());
    assert!(json.get("summary").is_some());
}

#[test]
fn test_missing_lockfile() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = 'test'\n").unwrap();

    let mut cmd = Command::cargo_bin("supply-chain").unwrap();
    cmd.arg(tmp.path().to_str().unwrap());
    cmd.assert().success();
    cmd.assert().stdout(contains("missing_lockfile"));
}

#[test]
fn test_unpinned_python_deps() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("requirements.txt"),
        "requests\nflask==1.0\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("supply-chain").unwrap();
    cmd.arg(tmp.path().to_str().unwrap());
    cmd.assert().success();
    cmd.assert().stdout(contains("SUPPLY-PIN"));
}
