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

    let json: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "cogent check --format json produced invalid JSON: {}\nstdout:\n{}\nstderr:\n{}",
            e, stdout, stderr
        )
    });

    (json, output.status.code(), stderr)
}

// ══════════════════════════════════════════════════════════════════════════
// Full Pipeline — JSON Output Structure
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_e2e_check_json_has_required_top_level_fields() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let obj = json
        .as_object()
        .expect("check output should be a JSON object");
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
    let actual_passed = checks
        .iter()
        .filter(|c| c["passed"].as_bool().unwrap_or(false))
        .count();
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

    let all_passed = checks
        .iter()
        .all(|c| c["passed"].as_bool().unwrap_or(false));
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

    let names: Vec<&str> = checks.iter().filter_map(|c| c["name"].as_str()).collect();

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

    let names: Vec<&str> = checks.iter().filter_map(|c| c["name"].as_str()).collect();

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
    assert!(!skipped_names.contains(&"debt"), "debt should be skipped");
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

    let json: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON with COGENT_MAX_CONCURRENT=2: {}\nstderr:\n{}",
            e, stderr
        )
    });

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

    let path = json["path"].as_str().expect("'path' should be a string");

    assert_eq!(
        path, expected_path,
        "output 'path' should match the input path"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Audit Opinion JSON Schema
// ══════════════════════════════════════════════════════════════════════════

/// Valid audit opinion values per the AuditOpinion enum.
const VALID_OPINIONS: &[&str] = &[
    "UNQUALIFIED_PASS",
    "QUALIFIED_PASS",
    "ADVERSE",
    "DISCLAIMER",
];

/// Valid category names per the compute_audit category definitions.
const EXPECTED_CATEGORIES: &[&str] =
    &["Security", "Compliance", "Quality", "Hygiene", "Operations"];

#[test]
fn test_e2e_audit_opinion_present() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    let audit = json
        .get("audit")
        .expect("JSON output should contain 'audit' field");
    let obj = audit.as_object().expect("'audit' should be a JSON object");

    // Required fields
    for field in &[
        "opinion",
        "overall_score",
        "grade",
        "gate_killers_passed",
        "gate_killer_names",
        "gate_killer_passed_names",
        "categories",
        "margin_risks",
        "unavailable_count",
    ] {
        assert!(
            obj.contains_key(*field),
            "audit missing required field '{}'. Keys: {:?}",
            field,
            obj.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_e2e_audit_opinion_value_is_valid() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let opinion = json["audit"]["opinion"]
        .as_str()
        .expect("audit.opinion should be a string");

    assert!(
        VALID_OPINIONS.contains(&opinion),
        "audit.opinion '{}' is not one of {:?}",
        opinion,
        VALID_OPINIONS
    );
}

#[test]
fn test_e2e_audit_overall_score_in_range() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let score = json["audit"]["overall_score"]
        .as_u64()
        .expect("audit.overall_score should be a number") as u32;

    assert!(
        score <= 100,
        "audit.overall_score {} should be 0-100",
        score
    );
}

#[test]
fn test_e2e_audit_grade_matches_score() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let score = json["audit"]["overall_score"]
        .as_u64()
        .expect("overall_score should be a number") as u32;
    let grade = json["audit"]["grade"]
        .as_str()
        .expect("audit.grade should be a string");

    let expected_grade = match score {
        90..=100 => "A",
        80..=89 => "B",
        65..=79 => "C",
        50..=64 => "D",
        _ => "F",
    };

    assert_eq!(
        grade, expected_grade,
        "audit.grade '{}' does not match overall_score {} (expected '{}')",
        grade, score, expected_grade
    );
}

#[test]
fn test_e2e_audit_gate_killers_structure() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let audit = &json["audit"];

    let gk_passed = audit["gate_killers_passed"]
        .as_bool()
        .expect("gate_killers_passed should be a bool");

    let gk_names = audit["gate_killer_names"]
        .as_array()
        .expect("gate_killer_names should be an array");

    let gk_passed_names = audit["gate_killer_passed_names"]
        .as_array()
        .expect("gate_killer_passed_names should be an array");

    // Should have exactly 4 gate killers
    assert_eq!(
        gk_names.len(),
        4,
        "should have 4 gate killer names, got {}",
        gk_names.len()
    );

    // All names should be strings
    for name in gk_names {
        assert!(
            name.is_string(),
            "gate_killer_names entries should be strings"
        );
    }

    // gate_killers_passed should be consistent with passed_names count
    assert_eq!(
        gk_passed,
        gk_passed_names.len() == gk_names.len(),
        "gate_killers_passed ({}) should be true iff all gate killers passed ({}/{})",
        gk_passed,
        gk_passed_names.len(),
        gk_names.len()
    );
}

#[test]
fn test_e2e_audit_categories_structure() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let categories = json["audit"]["categories"]
        .as_array()
        .expect("audit.categories should be an array");

    assert!(
        !categories.is_empty(),
        "audit.categories should have at least one entry"
    );

    let mut seen_names = Vec::new();
    for (i, cat) in categories.iter().enumerate() {
        let obj = cat
            .as_object()
            .expect("each category should be a JSON object");

        // Required fields
        for field in &["name", "weight", "score", "checks_passed", "checks_total"] {
            assert!(
                obj.contains_key(*field),
                "categories[{}] missing '{}'. Keys: {:?}",
                i,
                field,
                obj.keys().collect::<Vec<_>>()
            );
        }

        let name = obj["name"]
            .as_str()
            .expect("category name should be string");
        assert!(
            EXPECTED_CATEGORIES.contains(&name),
            "category name '{}' not in {:?}",
            name,
            EXPECTED_CATEGORIES
        );

        // No duplicate category names
        assert!(
            !seen_names.contains(&name),
            "duplicate category name '{}' at index {}",
            name,
            i
        );
        seen_names.push(name);

        // Weight should be 1-5
        let weight = obj["weight"].as_u64().expect("weight should be number");
        assert!(
            (1..=5).contains(&weight),
            "category '{}' weight {} should be 1-5",
            name,
            weight
        );

        // Score should be 0-100
        let score = obj["score"].as_f64().expect("score should be a number");
        assert!(
            (0.0..=100.0).contains(&score),
            "category '{}' score {} should be 0-100",
            name,
            score
        );

        // checks_passed <= checks_total
        let passed = obj["checks_passed"]
            .as_u64()
            .expect("checks_passed should be number");
        let total = obj["checks_total"]
            .as_u64()
            .expect("checks_total should be number");
        assert!(
            passed <= total,
            "category '{}': checks_passed ({}) > checks_total ({})",
            name,
            passed,
            total
        );

        // checks_total should be > 0 (empty categories are filtered out)
        assert!(
            total > 0,
            "category '{}' should have checks_total > 0",
            name
        );
    }
}

#[test]
fn test_e2e_audit_margin_risks_structure() {
    let (json, _exit, _stderr) = run_check_json(&[]);
    let margin_risks = json["audit"]["margin_risks"]
        .as_array()
        .expect("audit.margin_risks should be an array");

    // Each entry should be a [string, number] tuple
    for (i, entry) in margin_risks.iter().enumerate() {
        let arr = entry
            .as_array()
            .expect("margin_risks entries should be arrays (tuples)");
        assert_eq!(
            arr.len(),
            2,
            "margin_risks[{}] should have 2 elements, got {}",
            i,
            arr.len()
        );
        assert!(
            arr[0].is_string(),
            "margin_risks[{}] name should be string",
            i
        );
        assert!(
            arr[1].is_number(),
            "margin_risks[{}] margin should be number",
            i
        );

        let margin = arr[1].as_f64().unwrap();
        assert!(
            (0.0..=100.0).contains(&margin),
            "margin_risks[{}] margin {} should be 0-100",
            i,
            margin
        );
    }
}

#[test]
fn test_e2e_audit_health_score_fields_present() {
    let (json, _exit, _stderr) = run_check_json(&[]);

    // health_score and grade should be at the top level
    let health = json
        .get("health_score")
        .expect("JSON should contain 'health_score'");
    let hs = health.as_u64().expect("health_score should be a number");
    assert!(hs <= 100, "health_score {} should be 0-100", hs);

    let grade = json.get("grade").expect("JSON should contain 'grade'");
    let g = grade.as_str().expect("grade should be a string");
    assert!(
        ["A", "B", "C", "D", "F"].contains(&g),
        "grade '{}' should be one of A/B/C/D/F",
        g
    );
}

#[test]
fn test_e2e_audit_opinion_adverse_when_gate_killer_fails() {
    // Run with only secrets check forced to run (likely fails in fixture = Adverse)
    let (json, _exit, _stderr) = run_check_json(&["--only", "secrets"]);
    let opinion = json["audit"]["opinion"]
        .as_str()
        .expect("audit.opinion should be present");

    // When secrets is the only gate killer and it fails, opinion should be Adverse
    // When it passes, opinion could be UnqualifiedPass or QualifiedPass
    let gk_passed = json["audit"]["gate_killers_passed"].as_bool().unwrap();
    if !gk_passed {
        assert_eq!(
            opinion, "ADVERSE",
            "when gate killer fails, opinion should be ADVERSE, got {}",
            opinion
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// SIGPIPE Handling — piping to head/less should not produce errors
// ══════════════════════════════════════════════════════════════════════════

/// Absolute path to the project root (contains Cargo.toml and .quality.toml).
fn project_root() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    // CARGO_MANIFEST_DIR = crates/cogent-cli, so grandparent = project root
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Verify that `cogent <subcommand> --format <fmt> | head -1` does not
/// produce a "BrokenPipe" error.  The reset_sigpipe() call in main()
/// resets SIGPIPE to SIG_DFL so the kernel kills the writer silently.
///
/// The process may exit via SIGPIPE (signal 13) or exit normally (if
/// stdout is fully buffered and the BufWriter flushes during cleanup).
/// Both outcomes are acceptable — the invariant is no error message.
///
/// `extra_args` lets callers pass subcommand-specific flags
/// (e.g. `&["--force"]` for the `check` subcommand).
///
/// `path_override` replaces the default `fixtures/` directory with a
/// custom target path (e.g. project root for `cogent run`).
#[cfg(unix)]
fn assert_cogent_piped_to_head_no_broken_pipe(
    subcommand: &str,
    format: &str,
    extra_args: &[&str],
    path_override: Option<&Path>,
) {
    use std::io::Read as _;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command as StdCommand, Stdio};

    let target = path_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(fixture_path);
    let mut cogent = StdCommand::new(env!("CARGO_BIN_EXE_cogent"));
    cogent
        .arg(subcommand)
        .arg(target.to_str().unwrap())
        .args(extra_args)
        .arg("--format")
        .arg(format)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut cogent_child = cogent
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn cogent {}: {}", subcommand, e));

    // Pipe cogent stdout into `head -1` — head closes after 1 line,
    // which can trigger SIGPIPE on cogent's next stdout write.
    let mut head = StdCommand::new("head")
        .arg("-1")
        .stdin(cogent_child.stdout.take().unwrap())
        .stdout(Stdio::null())
        .spawn()
        .expect("failed to spawn head");

    head.wait().expect("head should exit cleanly");

    let status = cogent_child
        .wait()
        .unwrap_or_else(|e| panic!("cogent {} should exit after pipe closes: {}", subcommand, e));

    // Drain stderr after process exits — pipe is closed so this returns
    // immediately with all buffered data.
    let mut stderr_buf = Vec::new();
    if let Some(ref mut stderr) = cogent_child.stderr {
        let _ = stderr.read_to_end(&mut stderr_buf);
    }
    let stderr = String::from_utf8_lossy(&stderr_buf);

    // Core assertion: no "broken pipe" error should leak to stderr.
    assert!(
        !stderr.to_lowercase().contains("brokenpipe")
            && !stderr.to_lowercase().contains("broken pipe"),
        "cogent {} --format {} should not print a broken pipe error when piped to head.\nstderr:\n{}",
        subcommand, format, stderr
    );

    // Process should terminate — either killed by SIGPIPE (signal 13) or
    // exit normally.  Both are acceptable; the key invariant is no error.
    let terminated = status.code().is_some() || status.signal().is_some();
    assert!(
        terminated,
        "process should terminate (exit or signal), got: {:?}",
        status
    );
}

// ── `cogent check` SIGPIPE tests ────────────────────────────────────────

#[test]
#[cfg(unix)]
fn test_e2e_check_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("check", "text", &["--force"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_check_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("check", "json", &["--force"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_check_sarif_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("check", "sarif", &["--force"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_check_ndjson_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("check", "ndjson", &["--force"], None);
}

// ── `cogent audit` SIGPIPE tests ────────────────────────────────────────

#[test]
#[cfg(unix)]
fn test_e2e_audit_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("audit", "text", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_audit_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("audit", "json", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_audit_sarif_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("audit", "sarif", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_audit_agent_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("audit", "agent", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_audit_ndjson_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("audit", "ndjson", &[], None);
}

// ── `cogent run` SIGPIPE tests ──────────────────────────────────────────

#[test]
#[cfg(unix)]
fn test_e2e_run_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("run", "json", &[], Some(&project_root()));
}

#[test]
#[cfg(unix)]
fn test_e2e_run_sarif_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("run", "sarif", &[], Some(&project_root()));
}

// ── Individual tool SIGPIPE tests ──────────────────────────────────────

#[test]
#[cfg(unix)]
fn test_e2e_crap_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("crap", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_debt_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("debt", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_secrets_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("secrets", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_doccov_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("doccov", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_complexity_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("complexity", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_dupfind_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("dupfind", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_halstead_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("halstead", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_taint_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("taint", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_sast_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("sast", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_crypto_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("crypto", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_riskmap_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("riskmap", "text", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_coupling_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("coupling", "json", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_vulnscan_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("vulnscan", "text", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_errhandle_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("errhandle", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_deadcode_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("deadcode", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_cohesion_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("cohesion", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_comments_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("comments", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_linelen_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("linelen", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_propcov_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("propcov", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_fuzz_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("fuzz", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_licenses_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("licenses", "text", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_access_control_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("access-control", "json", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_supply_chain_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("supply-chain", "text", &[], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_sbom_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("sbom", "json", &[], Some(&project_root()));
}

#[test]
#[cfg(unix)]
fn test_e2e_typecov_text_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("typecov", "text", &["--recursive"], None);
}

#[test]
#[cfg(unix)]
fn test_e2e_mutate_json_piped_to_head_exits_cleanly() {
    assert_cogent_piped_to_head_no_broken_pipe("mutate", "json", &[], Some(&project_root()));
}
