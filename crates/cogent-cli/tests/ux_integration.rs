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

#[test]
fn test_cogent_doctor_json() {
    // cogent doctor --format json should produce valid JSON diagnostics
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("doctor").arg("--format").arg("json");

    let output = cmd.output().expect("failed to run cogent doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stderr, stdout);

    // Exit 0 (doctor is informational, never fails)
    assert!(
        output.status.success(),
        "cogent doctor --format json should exit 0. stderr:\n{}",
        stderr
    );

    // Output should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("doctor JSON not valid: {}\nstdout:\n{}", e, stdout));

    let obj = json.as_object().expect("doctor output should be a JSON object");

    // Verify expected diagnostic fields
    assert!(
        obj.contains_key("cogent_version"),
        "missing 'cogent_version'. keys: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(
        obj.contains_key("rust_version"),
        "missing 'rust_version'"
    );
    assert!(
        obj.contains_key("cargo_version"),
        "missing 'cargo_version'"
    );
    assert!(
        obj.contains_key("platform"),
        "missing 'platform'"
    );
    assert!(
        obj.contains_key("arch"),
        "missing 'arch'"
    );
    assert!(
        obj.contains_key("path"),
        "missing 'path'"
    );
    assert!(
        obj.contains_key("cwd"),
        "missing 'cwd'"
    );
    assert!(
        obj.contains_key("config"),
        "missing 'config'"
    );
    assert!(
        obj.contains_key("binaries"),
        "missing 'binaries'"
    );

    // Verify types
    assert!(
        obj["cogent_version"].is_string(),
        "cogent_version should be a string"
    );
    assert!(
        obj["platform"].is_string(),
        "platform should be a string"
    );
    assert!(
        obj["arch"].is_string(),
        "arch should be a string"
    );
    assert!(
        obj["binaries"].is_object(),
        "binaries should be an object"
    );

    // cogent_version should not be empty
    let version = obj["cogent_version"].as_str().unwrap_or("");
    assert!(
        !version.is_empty(),
        "cogent_version should not be empty"
    );

    // Verify combined output is correct (no unexpected errors on stderr)
    assert!(
        stderr.is_empty() || stderr.contains("Diagnostic"),
        "unexpected stderr output. stderr:\n{}",
        stderr
    );
}

// ══════════════════════════════════════════════════════════════════════════
// HELP & VERSION
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("--help");

    let output = cmd.output().expect("failed to run cogent --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stderr, stdout);

    assert!(output.status.success(), "cogent --help should exit 0");
    assert!(combined.contains("Usage:"), "expected 'Usage:' in help output");
    assert!(combined.contains("cogent"), "expected 'cogent' in help");
    // Verify several key subcommands are listed
    assert!(combined.contains("check"), "expected 'check' subcommand");
    assert!(combined.contains("doctor"), "expected 'doctor' subcommand");
    assert!(combined.contains("explain"), "expected 'explain' subcommand");
    assert!(combined.contains("init"), "expected 'init' subcommand");
    assert!(combined.contains("discover"), "expected 'discover' subcommand");
}

#[test]
fn test_help_check() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check").arg("--help");

    let output = cmd.output().expect("failed to run cogent check --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stderr, stdout);

    assert!(output.status.success(), "cogent check --help should exit 0");
    assert!(combined.contains("--format"), "expected '--format' flag in check help");
    assert!(combined.contains("--recursive"), "expected '--recursive' flag");
    assert!(combined.contains("--force"), "expected '--force' flag");
}

#[test]
fn test_version_output() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("--version");

    let output = cmd.output().expect("failed to run cogent --version");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    assert!(output.status.success(), "cogent --version should exit 0");
    assert!(!stdout.is_empty(), "version should not be empty");
    assert!(
        stdout.contains("cogent") || stdout.chars().any(|c| c.is_ascii_digit()),
        "version output should include version number. got: '{}'",
        stdout
    );
}

// ══════════════════════════════════════════════════════════════════════════
// EXPLAIN COMMAND (additional tools)
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_explain_crap() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("explain").arg("crap");

    let output = cmd.output().expect("failed to run cogent explain crap");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("CRAP"),
        "expected 'CRAP' in explain output. got:\n{}",
        combined
    );
    assert!(
        combined.contains("cyclomatic complexity") || combined.contains("complexity"),
        "expected complexity mention. got:\n{}",
        combined
    );
    assert!(
        combined.contains("Threshold"),
        "expected 'Threshold' section"
    );
    assert!(
        combined.contains("Quick fixes"),
        "expected 'Quick fixes' section"
    );
}

#[test]
fn test_explain_secrets() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("explain").arg("secrets");

    let output = cmd.output().expect("failed to run cogent explain secrets");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("Secret Detection") || combined.contains("Secret"),
        "expected 'Secret Detection' in explain output. got:\n{}",
        combined
    );
    assert!(
        combined.contains("API key") || combined.contains("tokens") || combined.contains("passwords"),
        "expected secret type mentions. got:\n{}",
        combined
    );
}

#[test]
fn test_explain_unknown_tool_lists_available() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("explain").arg("some_nonsense_tool_12345");

    let output = cmd.output().expect("failed to run cogent explain nonsense");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    assert!(
        combined.contains("Unknown tool"),
        "expected 'Unknown tool' message. got:\n{}",
        combined
    );
    // Should list available tools
    assert!(
        combined.contains("Available tools"),
        "expected 'Available tools' listing. got:\n{}",
        combined
    );
    // Should mention an example
    assert!(
        combined.contains("cogent explain"),
        "expected example usage. got:\n{}",
        combined
    );
}

// ══════════════════════════════════════════════════════════════════════════
// DISCOVER COMMAND
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_discover_json() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("discover").arg("--format").arg("json");

    let output = cmd.output().expect("failed to run cogent discover --format json");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cogent discover should exit 0");

    // Output should be valid JSON
    let tools: Vec<serde_json::Value> = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("discover JSON not valid: {}\nstdout:\n{}", e, stdout));

    assert!(!tools.is_empty(), "should have at least one tool entry");

    // Verify common tool entries exist
    let names: Vec<&str> = tools.iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(names.contains(&"crap"), "expected 'crap' in discovered tools");
    assert!(names.contains(&"debt"), "expected 'debt' in discovered tools");
    assert!(names.contains(&"check"), "expected 'check' in discovered tools");
    assert!(names.contains(&"init"), "expected 'init' in discovered tools");

    // Each tool should have required fields
    for tool in &tools {
        let obj = tool.as_object()
            .unwrap_or_else(|| panic!("each tool should be an object, got: {:?}", tool));
        assert!(obj.contains_key("name"), "tool missing 'name': {:?}", obj.keys().collect::<Vec<_>>());
        assert!(obj.contains_key("binary"), "tool '{}' missing 'binary'", obj["name"]);
        assert!(obj.contains_key("description"), "tool '{}' missing 'description'", obj["name"]);
        assert!(obj.contains_key("supported_formats"), "tool '{}' missing 'supported_formats'", obj["name"]);
        assert!(obj.contains_key("output_fields"), "tool '{}' missing 'output_fields'", obj["name"]);
    }
}

#[test]
fn test_discover_text() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("discover").arg("--format").arg("text");

    let output = cmd.output().expect("failed to run cogent discover --format text");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cogent discover should exit 0");

    // Text output should contain tool names and descriptions
    assert!(stdout.contains("crap"), "expected 'crap' in text output");
    assert!(stdout.contains("Description:"), "expected 'Description:' field");
    assert!(stdout.contains("Supported Formats:"), "expected 'Supported Formats:' field");
    assert!(stdout.contains("Output Fields:"), "expected 'Output Fields:' field");
    assert!(stdout.contains("Rule IDs:"), "expected 'Rule IDs:' field");
}

// ══════════════════════════════════════════════════════════════════════════
// DOCTOR COMMAND (text format)
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_cogent_doctor_text() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("doctor").arg("--format").arg("text");

    let output = cmd.output().expect("failed to run cogent doctor --format text");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stderr, stdout);

    // Exit 0 (doctor is informational, never fails)
    assert!(
        output.status.success(),
        "cogent doctor --format text should exit 0. stderr:\n{}",
        stderr
    );

    // Text output should contain version and diagnostic info
    assert!(
        combined.contains("cogent"),
        "expected 'cogent' in doctor output. got:\n{}",
        combined
    );
    assert!(
        combined.contains("version") || combined.contains("Version"),
        "expected version info. got:\n{}",
        combined
    );
    assert!(
        combined.contains("rust") || combined.contains("Rust") || combined.contains("cargo"),
        "expected Rust/cargo info. got:\n{}",
        combined
    );
    assert!(
        combined.contains("platform") || combined.contains("Platform") || combined.contains("arch") || combined.contains("Arch"),
        "expected platform info. got:\n{}",
        combined
    );
}

// ══════════════════════════════════════════════════════════════════════════
// INIT --CI
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_init_ci_creates_workflow() {
    // Create a temp dir to avoid polluting the real repo
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let temp_path = dir.path().to_path_buf();

    // Create a minimal Cargo.toml so ecosystem detection finds Rust
    std::fs::write(
        temp_path.join("Cargo.toml"),
        "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
    ).expect("failed to write Cargo.toml");

    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("init")
        .arg("--ci")
        .arg("--output")
        .arg(".quality-temp-ci-test.toml")
        .current_dir(&temp_path);

    let output = cmd.output().expect("failed to run cogent init --ci");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    // Should succeed
    assert!(
        output.status.success(),
        "cogent init --ci should exit 0. output:\n{}",
        combined
    );

    // Should detect Rust ecosystem
    assert!(
        combined.contains("Rust"),
        "expected Rust ecosystem detection. got:\n{}",
        combined
    );

    // Should mention CI setup steps
    assert!(
        combined.contains(".github/workflows") || combined.contains("GitHub Actions"),
        "expected workflow mention. got:\n{}",
        combined
    );

    // The workflow file should have been created
    let workflow_path = temp_path.join(".github/workflows/cogent.yml");
    assert!(
        workflow_path.exists(),
        "expected workflow file at {:?}",
        workflow_path
    );

    // Workflow should be valid YAML-like content
    let workflow_content = std::fs::read_to_string(&workflow_path)
        .unwrap_or_else(|e| panic!("failed to read workflow: {}", e));
    assert!(
        workflow_content.contains("name:"),
        "workflow should have a name field"
    );
    assert!(
        workflow_content.contains("on:"),
        "workflow should have trigger events"
    );
    assert!(
        workflow_content.contains("jobs:"),
        "workflow should have jobs"
    );

    // Clean up temp file
    let _ = std::fs::remove_file(temp_path.join(".quality-temp-ci-test.toml"));
    // tempfile::drop cleans up the rest
}

// ══════════════════════════════════════════════════════════════════════════
// COMPLETIONS COMMAND
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_completions_bash() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("completions").arg("bash");

    let output = cmd.output().expect("failed to run cogent completions bash");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cogent completions bash should exit 0");
    assert!(
        stdout.contains("_cogent") || stdout.contains("complete") || stdout.contains("bash-"),
        "expected bash completion output. got:\n{}",
        stdout
    );
}

#[test]
fn test_completions_zsh() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("completions").arg("zsh");

    let output = cmd.output().expect("failed to run cogent completions zsh");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cogent completions zsh should exit 0");
    assert!(
        stdout.contains("_cogent") || stdout.contains("#compdef") || stdout.contains("compdef"),
        "expected zsh completion output. got:\n{}",
        stdout
    );
}

#[test]
fn test_completions_fish() {
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("completions").arg("fish");

    let output = cmd.output().expect("failed to run cogent completions fish");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cogent completions fish should exit 0");
    assert!(
        stdout.contains("complete") || stdout.contains("cogent"),
        "expected fish completion output. got:\n{}",
        stdout
    );
}
