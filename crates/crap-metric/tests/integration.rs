use assert_cmd::Command;
use predicates::prelude::*;

const TEST_PROJECT: &str = env!("CARGO_MANIFEST_DIR");

fn crap_cmd() -> Command {
    Command::cargo_bin("crap").expect("crap binary not found")
}

#[test]
fn test_basic_analysis() {
    let src = format!("{}/src", TEST_PROJECT);
    crap_cmd()
        .arg(&src)
        .arg("--recursive")
        .assert()
        .success()
        .stdout(predicate::str::contains("FUNCTION"))
        .stdout(predicate::str::contains("Functions analyzed"));
}

#[test]
fn test_json_output() {
    let src = format!("{}/src", TEST_PROJECT);
    crap_cmd()
        .arg(&src)
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"functions\""))
        .stdout(predicate::str::contains("\"summary\""));
}

#[test]
fn test_with_coverage() {
    // Create a minimal valid lcov file
    let lcov_path = std::env::temp_dir().join("test-integration-crap.info");
    std::fs::write(&lcov_path, "TN:\nSF:/fake.rs\nLF:10\nLH:5\nend_of_record\n").unwrap();

    let src = format!("{}/src", TEST_PROJECT);
    crap_cmd()
        .arg(&src)
        .arg("--coverage")
        .arg(&lcov_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Functions analyzed"));

    std::fs::remove_file(&lcov_path).ok();
}

#[test]
fn test_min_score_filter() {
    let src = format!("{}/src", TEST_PROJECT);
    crap_cmd()
        .arg(&src)
        .arg("--min-score")
        .arg("100")
        .assert()
        .success();
}

#[test]
fn test_single_file() {
    let lib = format!("{}/src/main.rs", TEST_PROJECT);
    crap_cmd()
        .arg(&lib)
        .assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
fn test_nonexistent_path() {
    crap_cmd()
        .arg("/tmp/nonexistent-path-12345")
        .assert()
        .failure() // exits 1 when no files found
        .stderr(predicate::str::contains("No source files found"));
}

#[test]
fn test_categories_displayed() {
    let src = format!("{}/src", TEST_PROJECT);
    crap_cmd()
        .arg(&src)
        .arg("--recursive")
        .assert()
        .success()
        .stdout(predicate::str::contains("excellent"))
        .stdout(predicate::str::contains("good"));
}

#[test]
fn test_output_columns_all_present() {
    // Verify all expected column headers appear in the table output
    let src = format!("{}/src", TEST_PROJECT);
    let output = crap_cmd().arg(&src).arg("--recursive").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("FUNCTION"), "Missing FUNCTION column");
    assert!(stdout.contains("FILE"), "Missing FILE column");
    assert!(stdout.contains("LINE"), "Missing LINE column");
    assert!(stdout.contains("COMP"), "Missing COMP (complexity) column");
    assert!(stdout.contains("LINES"), "Missing LINES column");
    assert!(stdout.contains("CRAP"), "Missing CRAP column");
    assert!(stdout.contains("CATEGORY"), "Missing CATEGORY column");
}

#[test]
fn test_category_icons_in_output() {
    // Verify category icons (checkmark, circle, triangle, cross) appear for different categories
    let src = format!("{}/src", TEST_PROJECT);
    let output = crap_cmd().arg(&src).arg("--recursive").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // At least one category icon should be present
    let has_excellent_icon = stdout.contains("\u{2713} excellent"); // ✓ excellent
    let has_good_icon = stdout.contains("\u{25CB} good"); // ○ good
    let has_acceptable_icon = stdout.contains("\u{25B3} acceptable"); // △ acceptable
    let has_crappy_icon = stdout.contains("\u{2717} crappy"); // ✗ crappy

    // At least one category type should be shown with its icon
    assert!(
        has_excellent_icon || has_good_icon || has_acceptable_icon || has_crappy_icon,
        "No category icons found in output"
    );
}

#[test]
fn test_summary_fields() {
    // Verify the summary section contains all expected fields
    let src = format!("{}/src", TEST_PROJECT);
    let output = crap_cmd().arg(&src).arg("--recursive").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Functions analyzed"),
        "Missing 'Functions analyzed' in summary"
    );
    assert!(
        stdout.contains("Total complexity"),
        "Missing 'Total complexity' in summary"
    );
    assert!(
        stdout.contains("Avg complexity"),
        "Missing 'Avg complexity' in summary"
    );
    assert!(
        stdout.contains("Avg CRAP score"),
        "Missing 'Avg CRAP score' in summary"
    );
}

#[test]
fn test_category_breakdown_counts() {
    // Verify the summary shows counts for each category
    let src = format!("{}/src", TEST_PROJECT);
    let output = crap_cmd().arg(&src).arg("--recursive").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The category breakdown line shows counts
    assert!(
        stdout.contains("excellent"),
        "Missing 'excellent' in category breakdown"
    );
    assert!(
        stdout.contains("good"),
        "Missing 'good' in category breakdown"
    );
    assert!(
        stdout.contains("acceptable"),
        "Missing 'acceptable' in category breakdown"
    );
    assert!(
        stdout.contains("crappy"),
        "Missing 'crappy' in category breakdown"
    );
}
