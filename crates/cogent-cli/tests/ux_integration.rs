//! UX and first-run integration tests.
//!
//! Validates new-user experience features: missing-config guard,
//! explain command output, and init ecosystem detection.

use assert_cmd::Command;
use std::path::Path;

/// Absolute path to a small fixture we can point the CLI at.
fn fixture_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).parent().unwrap().join("fixtures")
}

#[test]
fn test_missing_config_hint() {
    // Run cogent check on the fixture without --force.
    // If .quality.toml is missing, it should suggest "cogent init" and exit 2.
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--format")
        .arg("text");

    let output = cmd.output().expect("failed to run cogent check");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    if !Path::new(".quality.toml").exists() {
        assert!(
            combined.contains("cogent init") || combined.contains("Run cogent init"),
            "expected missing-config hint mentioning 'cogent init'. output:\n{}",
            combined
        );
        assert!(
            output.status.code() == Some(2),
            "expected exit code 2 for missing config, got {:?}",
            output.status.code()
        );
    }
}

#[test]
fn test_explain_command() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("explain").arg("debt");

    let output = cmd.output().expect("failed to run cogent explain");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("Technical Debt"),
        "expected 'Technical Debt' in explain output. got:\n{}",
        combined
    );
    assert!(
        combined.contains("TODO") || combined.contains("FIXME"),
        "expected marker names in explain output. got:\n{}",
        combined
    );
    assert!(
        combined.contains("Quick fixes") || combined.contains("Quick Fixes"),
        "expected 'Quick fixes' section in explain output. got:\n{}",
        combined
    );
}

#[test]
fn test_explain_unknown_tool() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("explain").arg("nonexistent_tool_xyz");

    let output = cmd.output().expect("failed to run cogent explain");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("Unknown tool"),
        "expected 'Unknown tool' for invalid tool name. got:\n{}",
        combined
    );
}

#[test]
fn test_init_shows_ecosystem() {
    // Run init in the workspace root (which has Cargo.toml)
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = Path::new(manifest).parent().unwrap().parent().unwrap();

    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("init")
        .arg("--output")
        .arg("test-quality-temp.toml")
        .current_dir(repo_root);

    let output = cmd.output().expect("failed to run cogent init");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("detected:") && combined.contains("Rust"),
        "expected ecosystem detection message. got:\n{}",
        combined
    );
    assert!(
        combined.contains("Key thresholds chosen"),
        "expected threshold preview in init output. got:\n{}",
        combined
    );

    // Cleanup
    let _ = std::fs::remove_file(repo_root.join("test-quality-temp.toml"));
}
