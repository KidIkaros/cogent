//! End-to-end integration tests for `cogent check`.
//!
//! Validates the full orchestration pipeline:
//!   config loading → parallel check execution → result aggregation → health scoring → output
//!
//! These tests exercise the real binary with real tool binaries, verifying that
//! the entire `cogent check` subsystem works as a cohesive unit.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

/// Absolute path to a small fixture directory with known source files.
fn fixture_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).parent().unwrap().join("fixtures")
}

/// Run `cogent check` with --force and --format json, return parsed JSON + exit code.
fn run_check_json(args: &[&str]) -> (Value, Option<i32>, String) {
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--force")
        .arg("--format")
        .arg("json")
        .args(args);

    let output = cmd.output().expect("failed to run cogent check");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("cogent check --format json produced invalid JSON: {}\nstdout:\n{}\nstderr:\n{}", e, stdout, stderr));

    (json, output.status.code(), stderr)
}

// ══════════════════════════════════════════════════════════════════════════
// Full Pipeline — JSON Output Structure
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_json_has_required_top_level_fields() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let obj = json.as_object().expect("check output should be a JSON object");
    for field in &["passed", "path", "checks", "summary"] {
        assert!(
            obj.contains_key(*field),
            "check output missing required field '{}'. Keys: {:?}",
            field,
            obj.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_e2e_check_json_checks_is_array() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    // We expect multiple checks to have run (at least the ones that work
    // without .quality.toml --force defaults)
    assert!(
        checks.len() >= 10,
        "expected at least 10 checks to run, got {}",
        checks.len()
    );
}

#[test]
fn test_e2e_check_json_each_check_has_required_fields() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    for (i, check) in checks.iter().enumerate() {
        let obj = check
            .as_object()
            .expect("each check should be a JSON object");

        for field in &["name", "passed", "message", "details"] {
            assert!(
                obj.contains_key(*field),
                "check[{}] ('{}') missing required field '{}'. Keys: {:?}",
                i,
                obj.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                field,
                obj.keys().collect::<Vec<_>>()
            );
        }

        // 'name' must be a non-empty string
        let name = obj["name"]
            .as_str()
            .expect("check 'name' should be a string");
        assert!(!name.is_empty(), "check[{}] has empty 'name'", i);

        // 'passed' must be a boolean
        assert!(
            obj["passed"].is_boolean(),
            "check[{}] ('{}') 'passed' should be a boolean, got: {}",
            i,
            name,
            obj["passed"]
        );

        // 'message' must be a string
        assert!(
            obj["message"].is_string(),
            "check[{}] ('{}') 'message' should be a string",
            i,
            name
        );
    }
}

#[test]
fn test_e2e_check_json_summary_matches_checks() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");
    let summary = json["summary"]
        .as_object()
        .expect("'summary' should be an object");

    let total_checks = summary["total_checks"]
        .as_u64()
        .expect("summary.total_checks should be a number") as usize;
    let passed_checks = summary["passed_checks"]
        .as_u64()
        .expect("summary.passed_checks should be a number") as usize;
    let failed_checks = summary["failed_checks"]
        .as_u64()
        .expect("summary.failed_checks should be a number") as usize;

    assert_eq!(
        total_checks,
        checks.len(),
        "total_checks ({}) should match checks array length ({})",
        total_checks,
        checks.len()
    );
    assert_eq!(
        passed_checks + failed_checks,
        total_checks,
        "passed_checks ({}) + failed_checks ({}) should equal total_checks ({})",
        passed_checks,
        failed_checks,
        total_checks
    );

    // Verify passed/failed counts match actual check results
    let actual_passed = checks.iter().filter(|c| c["passed"].as_bool().unwrap_or(false)).count();
    let actual_failed = checks.len() - actual_passed;
    assert_eq!(
        passed_checks, actual_passed,
        "summary.passed_checks ({}) should match actual passed count ({})",
        passed_checks, actual_passed
    );
    assert_eq!(
        failed_checks, actual_failed,
        "summary.failed_checks ({}) should match actual failed count ({})",
        failed_checks, actual_failed
    );
}

#[test]
fn test_e2e_check_json_passed_is_consistent() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let passed = json["passed"]
        .as_bool()
        .expect("'passed' should be a boolean");
    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    let all_passed = checks.iter().all(|c| c["passed"].as_bool().unwrap_or(false));
    assert_eq!(
        passed, all_passed,
        "top-level 'passed' ({}) should be true iff ALL checks passed ({})",
        passed, all_passed
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Exit Code Behavior
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_exit_code_matches_passed() {
    let (json, exit, _stderr) = run_check_json(&[]);
    let passed = json["passed"].as_bool().unwrap_or(false);
    let exit_code = exit.unwrap_or(2);

    if passed {
        assert_eq!(exit_code, 0, "when all checks pass, exit code should be 0");
    } else {
        assert_eq!(exit_code, 1, "when any check fails, exit code should be 1");
    }
}

#[test]
fn test_e2e_check_ci_exit_code() {
    // CI mode should still respect pass/fail exit codes
    let (json, exit, _stderr) = run_check_json(&["--ci"]);
    let passed = json["passed"].as_bool().unwrap_or(false);
    let exit_code = exit.unwrap_or(2);

    if passed {
        assert_eq!(exit_code, 0, "CI mode: when passed, exit code should be 0");
    } else {
        assert_eq!(exit_code, 1, "CI mode: when failed, exit code should be 1");
    }
}

// ══════════════════════════════════════════════════════════════════════════
// Check Uniqueness & Deduplication
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_no_duplicate_names() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    let names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();

    let mut sorted_names = names.clone();
    sorted_names.sort();
    sorted_names.dedup();

    assert_eq!(
        names.len(),
        sorted_names.len(),
        "duplicate check names found. Names: {:?}",
        names
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Check Details Structure
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_details_is_object_or_null() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    for check in checks {
        let details = &check["details"];
        assert!(
            details.is_object() || details.is_null(),
            "check '{}' has unexpected details type: {}",
            check["name"].as_str().unwrap_or("?"),
            details
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// Selective Check Execution (--only / --skip)
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_only_specific_checks() {
    let (json, _exit, _stderr) = run_check_json(&["--only", "secrets,debt"]);

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    let names: Vec<&str> = checks
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();

    for name in &names {
        assert!(
            *name == "secrets" || *name == "debt",
            "unexpected check '{}' with --only secrets,debt",
            name
        );
    }
}

#[test]
fn test_e2e_check_skip_specific_checks() {
    let (json_full, _, _) = run_check_json(&[]);
    let (json_skipped, _, _) = run_check_json(&["--skip", "secrets,debt"]);

    let full_count = json_full["checks"].as_array().unwrap().len();
    let skipped_count = json_skipped["checks"].as_array().unwrap().len();

    assert!(
        skipped_count < full_count,
        "--skip should reduce check count. full={}, skipped={}",
        full_count,
        skipped_count
    );

    let skipped_names: Vec<&str> = json_skipped["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();

    assert!(
        !skipped_names.contains(&"secrets"),
        "secrets should be skipped"
    );
    assert!(
        !skipped_names.contains(&"debt"),
        "debt should be skipped"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Parallel Execution — Concurrency Limiting
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_respects_concurrency_env_var() {
    // Set COGENT_MAX_CONCURRENT=2 and verify checks still pass
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--force")
        .arg("--format")
        .arg("json")
        .env("COGENT_MAX_CONCURRENT", "2");

    let output = cmd.output().expect("failed to run cogent check");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON with COGENT_MAX_CONCURRENT=2: {}\nstderr:\n{}", e, stderr));

    let checks = json["checks"]
        .as_array()
        .expect("'checks' should be an array");

    // With concurrency=2, all checks should still run and produce valid results
    assert!(
        checks.len() >= 10,
        "expected at least 10 checks with COGENT_MAX_CONCURRENT=2, got {}",
        checks.len()
    );

    // Each check should have valid structure
    for check in checks {
        assert!(check["name"].is_string(), "check name should be string");
        assert!(check["passed"].is_boolean(), "check passed should be bool");
    }
}

#[test]
fn test_e2e_check_concurrency_env_var_min_clamped_to_one() {
    // Setting COGENT_MAX_CONCURRENT=0 should clamp to 1 (minimum)
    let fixture = fixture_path();
    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(fixture.to_str().unwrap())
        .arg("--force")
        .arg("--format")
        .arg("json")
        .env("COGENT_MAX_CONCURRENT", "0");

    let output = cmd.output().expect("failed to run cogent check");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    let json: Value = serde_json::from_str(&stdout)
        .expect("should produce valid JSON even with COGENT_MAX_CONCURRENT=0");

    let checks = json["checks"].as_array().unwrap();
    assert!(
        checks.len() >= 10,
        "checks should still run with concurrency clamped to 1"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// File Summary Aggregation
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_file_summary_structure() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    // file_summary is optional — only present when findings exist
    if let Some(file_summary) = json.get("file_summary") {
        let arr = file_summary
            .as_array()
            .expect("file_summary should be an array");

        for entry in arr {
            let obj = entry
                .as_object()
                .expect("each file summary entry should be an object");

            assert!(
                obj.contains_key("file"),
                "file_summary entry missing 'file'"
            );
            assert!(
                obj.contains_key("issue_count"),
                "file_summary entry missing 'issue_count'"
            );
            assert!(
                obj.contains_key("severity_score"),
                "file_summary entry missing 'severity_score'"
            );

            // file should be a string (may be empty for aggregate/summary findings)
            assert!(obj["file"].is_string(), "file should be a string");

            // issue_count and severity_score should be numbers
            assert!(
                obj["issue_count"].is_number(),
                "issue_count should be a number"
            );
            assert!(
                obj["severity_score"].is_number(),
                "severity_score should be a number"
            );
        }

        // file_summary should be sorted by severity_score descending
        if arr.len() >= 2 {
            for window in arr.windows(2) {
                let a_score = window[0]["severity_score"].as_u64().unwrap_or(0);
                let b_score = window[1]["severity_score"].as_u64().unwrap_or(0);
                assert!(
                    a_score >= b_score,
                    "file_summary should be sorted by severity_score descending. \
                     Got {} before {}",
                    a_score,
                    b_score
                );
            }
        }

        // file_summary should be truncated to at most 20 entries
        assert!(
            arr.len() <= 20,
            "file_summary should be truncated to 20 entries, got {}",
            arr.len()
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// Path Field Validation
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_path_matches_input() {
    let fixture = fixture_path();
    let expected_path = fixture.to_str().unwrap();

    let mut cmd = Command::cargo_bin("cogent").expect("cogent binary not found");
    cmd.arg("check")
        .arg(expected_path)
        .arg("--force")
        .arg("--format")
        .arg("json");

    let output = cmd.output().expect("failed to run cogent check");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let json: Value = serde_json::from_str(&stdout).expect("invalid JSON");

    let path = json["path"]
        .as_str()
        .expect("'path' should be a string");

    assert_eq!(
        path, expected_path,
        "output 'path' should match the input path"
    );
}
