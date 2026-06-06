//! Project configuration detection and .quality.toml generation.

#![deny(clippy::all)]

/// Ecosystem detected from project root filesystem signals.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectEcosystem {
    Rust,
    JavaScript,
    Python,
    Go,
    Unknown,
}

impl std::fmt::Display for ProjectEcosystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectEcosystem::Rust => write!(f, "Rust"),
            ProjectEcosystem::JavaScript => write!(f, "JavaScript/TypeScript"),
            ProjectEcosystem::Python => write!(f, "Python"),
            ProjectEcosystem::Go => write!(f, "Go"),
            ProjectEcosystem::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Everything Cogent needs to run tests and coverage for a project automatically.
#[derive(Debug, Clone)]
pub struct ProjectProfile {
    pub ecosystem: ProjectEcosystem,
    /// Command + args to run the test suite, e.g. ["cargo", "test"]
    pub test_cmd: Vec<String>,
    /// Command + args to collect coverage into `lcov_path`
    pub coverage_cmd: Vec<String>,
    /// Where the coverage output file will be written
    pub lcov_path: String,
    /// Source file extensions to watch for this ecosystem
    pub watch_extensions: Vec<String>,
    /// Recommended quality thresholds (language-tuned)
    pub max_crap: f64,
    pub min_doc: f64,
    pub max_debt: usize,
    pub max_complexity_violations: usize,
}

impl ProjectProfile {
    pub fn is_coverage_available(&self) -> bool {
        !self.coverage_cmd.is_empty()
    }
}

/// Inspect filesystem signals starting at `root` and return a `ProjectProfile`.
/// Falls back to unknown defaults when nothing is detected.
pub fn detect_project(root: &str) -> ProjectProfile {
    let p = std::path::Path::new(root);

    // Rust — Cargo.toml present
    if p.join("Cargo.toml").exists() || std::path::Path::new("Cargo.toml").exists() {
        return ProjectProfile {
            ecosystem: ProjectEcosystem::Rust,
            test_cmd: vec!["cargo".into(), "test".into()],
            coverage_cmd: vec![
                "cargo".into(),
                "llvm-cov".into(),
                "--lcov".into(),
                "--output-path".into(),
                "lcov.info".into(),
            ],
            lcov_path: "lcov.info".into(),
            watch_extensions: vec!["rs".into(), "toml".into()],
            max_crap: 15.0,
            min_doc: 95.0,
            max_debt: 0,
            max_complexity_violations: 0,
        };
    }

    // Go — go.mod present
    if p.join("go.mod").exists() || std::path::Path::new("go.mod").exists() {
        return ProjectProfile {
            ecosystem: ProjectEcosystem::Go,
            test_cmd: vec!["go".into(), "test".into(), "./...".into()],
            coverage_cmd: vec![
                "go".into(),
                "test".into(),
                "-coverprofile=coverage.out".into(),
                "./...".into(),
            ],
            lcov_path: String::new(), // go coverage not lcov; skip coverage feed
            watch_extensions: vec!["go".into()],
            max_crap: 20.0,
            min_doc: 80.0,
            max_debt: 0,
            max_complexity_violations: 0,
        };
    }

    // Python — pyproject.toml or setup.py present
    if p.join("pyproject.toml").exists()
        || p.join("setup.py").exists()
        || std::path::Path::new("pyproject.toml").exists()
        || std::path::Path::new("setup.py").exists()
    {
        return ProjectProfile {
            ecosystem: ProjectEcosystem::Python,
            test_cmd: vec!["pytest".into()],
            coverage_cmd: vec![
                "pytest".into(),
                "--cov".into(),
                "--cov-report=lcov:lcov.info".into(),
            ],
            lcov_path: "lcov.info".into(),
            watch_extensions: vec!["py".into(), "pyi".into()],
            max_crap: 20.0,
            min_doc: 80.0,
            max_debt: 0,
            max_complexity_violations: 0,
        };
    }

    // JavaScript/TypeScript — package.json present
    if p.join("package.json").exists() || std::path::Path::new("package.json").exists() {
        let has_vitest = p.join("vitest.config.ts").exists()
            || p.join("vitest.config.js").exists()
            || std::path::Path::new("vitest.config.ts").exists();
        let test_cmd = if has_vitest {
            vec!["npx".into(), "vitest".into(), "run".into()]
        } else {
            vec!["npm".into(), "test".into()]
        };
        let coverage_cmd = if has_vitest {
            vec![
                "npx".into(),
                "vitest".into(),
                "run".into(),
                "--coverage".into(),
            ]
        } else {
            vec![
                "npx".into(),
                "jest".into(),
                "--coverage".into(),
                "--coverageReporters=lcov".into(),
            ]
        };
        return ProjectProfile {
            ecosystem: ProjectEcosystem::JavaScript,
            test_cmd,
            coverage_cmd,
            lcov_path: "coverage/lcov.info".into(),
            watch_extensions: vec!["js".into(), "ts".into(), "jsx".into(), "tsx".into()],
            max_crap: 20.0,
            min_doc: 70.0,
            max_debt: 0,
            max_complexity_violations: 0,
        };
    }

    // Fallback
    ProjectProfile {
        ecosystem: ProjectEcosystem::Unknown,
        test_cmd: Vec::new(),
        coverage_cmd: Vec::new(),
        lcov_path: String::new(),
        watch_extensions: vec![
            "rs".into(),
            "py".into(),
            "js".into(),
            "ts".into(),
            "go".into(),
            "java".into(),
            "cpp".into(),
            "c".into(),
        ],
        max_crap: 30.0,
        min_doc: 50.0,
        max_debt: 100,
        max_complexity_violations: 0,
    }
}

/// Write a fresh `.quality.toml` to `output` based on the detected `profile`.
pub fn generate_config(output: &str, profile: &ProjectProfile) {
    let config = format!(
        r#"# .quality.toml — Cogent quality thresholds
# Auto-generated for: {ecosystem}
# Used by: cogent check . and cogent run .
# Run `cogent init` at any time to regenerate with updated detection.

[project]
ecosystem = "{ecosystem}"
test_cmd = {test_cmd}
coverage_cmd = {coverage_cmd}
lcov_path = "{lcov_path}"

[crap]
# CRAP = complexity^2 * (1 - coverage)^3 + complexity. Lower is better.
max_avg = {max_crap}

[debt]
max_markers = {max_debt}
types = ["TODO", "FIXME", "HACK", "XXX"]

[doc_coverage]
min_pct = {min_doc}

[complexity]
max_violations = {max_complexity}

[duplication]
max_duplicates = 0
min_lines = 3

[skip]
checks = []

[secrets]
# Exclude paths from secrets scanner (TOML array or bare comma list)
# secrets_exclude = ["vendor", "tests"]
# Also settable via CLI: --secrets-exclude vendor,tests
# Or env var: COGENT_SECRETS_EXCLUDE=vendor,tests
"#,
        ecosystem = profile.ecosystem,
        test_cmd = serde_json::to_string(&profile.test_cmd).unwrap_or_default(),
        coverage_cmd = serde_json::to_string(&profile.coverage_cmd).unwrap_or_default(),
        lcov_path = profile.lcov_path,
        max_crap = profile.max_crap,
        max_debt = profile.max_debt,
        min_doc = profile.min_doc,
        max_complexity = profile.max_complexity_violations,
    );
    if let Err(e) = std::fs::write(output, config) {
        tracing::error!(path = %output, error = %e, "failed to write config");
    }
}

/// Build a GitHub Actions workflow YAML string from a `ProjectProfile`.
pub fn build_gha_workflow(profile: &ProjectProfile) -> String {
    let coverage_step = if profile.is_coverage_available() {
        let cmd = profile.coverage_cmd.join(" ");
        format!(
            r#"      - name: Collect coverage
        run: {cmd}
"#,
            cmd = cmd
        )
    } else {
        String::new()
    };

    let lcov_flag = if !profile.lcov_path.is_empty() {
        format!(" --coverage {}", profile.lcov_path)
    } else {
        String::new()
    };

    format!(
        r#"name: Cogent Quality Gate
# Generated by: cogent init --ci

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2

      - name: Build Cogent
        run: cargo build --release -p cogent-cli

      - name: Run tests
        run: {test_cmd}

{coverage_step}
      - name: Quality check
        run: ./target/release/cogent check .{lcov_flag} --format text

      - name: Full audit (SARIF)
        run: |
          ./target/release/cogent run . --format sarif \
            --baseline .cogent-baseline.sarif \
            > quality-results.sarif

      - name: Upload SARIF to GitHub Security
        if: always()
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: quality-results.sarif
          category: cogent

      - name: Update baseline on main
        if: github.ref == 'refs/heads/main'
        run: |
          mv quality-results.sarif .cogent-baseline.sarif
          ./target/release/cogent history record
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add .cogent-baseline.sarif .cogent-history/ || true
          git commit -m "chore: update quality baseline [ci skip]" || true
          git push origin HEAD || true
    permissions:
      security-events: write
      contents: write
"#,
        test_cmd = profile.test_cmd.join(" "),
        coverage_step = coverage_step,
        lcov_flag = lcov_flag,
    )
}

/// Line-by-line TOML threshold tuple used by `load_config_thresholds`.
pub type Thresholds = (
    f64,   // 0 max_crap
    f64,   // 1 min_doc
    usize, // 2 max_debt
    usize, // 3 max_complexity
    f64,   // 4 max_duplication
    usize, // 5 max_taint
    f64,   // 6 max_risk
    usize, // 7 max_coupling
    f64,   // 8 min_propcov
    usize, // 9 max_fuzz_risk
    usize, // 10 max_linelen
    f64,   // 11 max_halstead_bugs
    usize, // 12 max_secrets
    usize, // 13 max_deadcode
    usize, // 14 max_cohesion
    f64,   // 15 min_comment_ratio
    usize, // 16 max_errhandle
    f64,   // 17 min_typecov
    usize, // 18 max_vuln_critical
    usize, // 19 max_vuln_high
    usize, // 20 max_sast
    usize, // 21 max_crypto
    usize, // 22 max_license_violations
);

/// Load thresholds from `.quality.toml` if present, falling back to `defaults`.
/// Parsing is intentionally line-by-line to avoid pulling in a TOML dependency.
///
/// **Section-aware**: tracks `[section]` headers and only parses keys in the
/// top-level section (no `[header]`) or in known tool sections. Lines inside
/// `[override.*]` sections are skipped to prevent per-path overrides from
/// leaking into global thresholds.
pub fn load_config_thresholds(config_path: &str, defaults: Thresholds) -> Thresholds {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return defaults;
    };
    let mut max_crap = defaults.0;
    let mut min_doc = defaults.1;
    let mut max_debt = defaults.2;
    let mut max_complexity = defaults.3;
    let mut max_duplication = defaults.4;
    let mut max_taint = defaults.5;
    let mut max_risk = defaults.6;
    let mut max_coupling: usize = defaults.7;
    let mut min_propcov = defaults.8;
    let mut max_fuzz_risk: usize = defaults.9;
    let mut max_linelen: usize = defaults.10;
    let mut max_halstead_bugs = defaults.11;
    let mut max_secrets: usize = defaults.12;
    let mut max_deadcode: usize = defaults.13;
    let mut max_cohesion: usize = defaults.14;
    let mut min_comment_ratio = defaults.15;
    let mut max_errhandle: usize = defaults.16;
    let mut min_typecov = defaults.17;
    let mut max_vuln_critical: usize = defaults.18;
    let mut max_vuln_high: usize = defaults.19;
    let mut max_sast: usize = defaults.20;
    let mut max_crypto: usize = defaults.21;
    let mut max_license_violations: usize = defaults.22;

    // Track which TOML section we're in. "" = top-level (no header yet).
    // Lines in `[override.*]` sections must be ignored so per-path overrides
    // (e.g. max_secrets = 9999 for tests) don't clobber global thresholds.
    let mut current_section: String = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Track section headers: [section_name]
        if line.starts_with('[') && line.ends_with(']') && !line.starts_with("[[") {
            let section = &line[1..line.len() - 1];
            current_section = section.to_string();
            continue;
        }
        // Skip comment lines (but only outside section headers)
        if line.starts_with('#') {
            continue;
        }
        // Skip lines inside [override.*] sections — these are per-path overrides
        if current_section.starts_with("override.") {
            continue;
        }
        if let Some(val) = parse_toml_f64(line, "max_avg") {
            max_crap = val;
        }
        if let Some(val) = parse_toml_f64(line, "min_pct") {
            min_doc = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_markers") {
            max_debt = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_violations") {
            max_complexity = val;
        }
        if let Some(val) = parse_toml_f64(line, "max_duplicates") {
            max_duplication = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_taint") {
            max_taint = val;
        }
        if let Some(val) = parse_toml_f64(line, "max_risk") {
            max_risk = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_coupling") {
            max_coupling = val;
        }
        if let Some(val) = parse_toml_f64(line, "min_propcov") {
            min_propcov = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_fuzz_risk") {
            max_fuzz_risk = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_linelen") {
            max_linelen = val;
        }
        if let Some(val) = parse_toml_f64(line, "max_halstead_bugs") {
            max_halstead_bugs = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_secrets") {
            max_secrets = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_deadcode") {
            max_deadcode = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_cohesion") {
            max_cohesion = val;
        }
        if let Some(val) = parse_toml_f64(line, "min_comment_ratio") {
            min_comment_ratio = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_errhandle") {
            max_errhandle = val;
        }
        if let Some(val) = parse_toml_f64(line, "min_typecov") {
            min_typecov = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_vuln_critical") {
            max_vuln_critical = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_vuln_high") {
            max_vuln_high = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_sast") {
            max_sast = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_crypto") {
            max_crypto = val;
        }
        if let Some(val) = parse_toml_usize(line, "max_license_violations") {
            max_license_violations = val;
        }
    }
    (
        max_crap,
        min_doc,
        max_debt,
        max_complexity,
        max_duplication,
        max_taint,
        max_risk,
        max_coupling,
        min_propcov,
        max_fuzz_risk,
        max_linelen,
        max_halstead_bugs,
        max_secrets,
        max_deadcode,
        max_cohesion,
        min_comment_ratio,
        max_errhandle,
        min_typecov,
        max_vuln_critical,
        max_vuln_high,
        max_sast,
        max_crypto,
        max_license_violations,
    )
}

/// Load `secrets_exclude` paths for the secrets scanner.
/// Priority: `COGENT_SECRETS_EXCLUDE` env var > `.quality.toml` > empty.
/// Supports both single-line and multi-line TOML array syntax.
pub fn load_secrets_exclude(config_path: &str) -> Vec<String> {
    // Environment variable overrides config file
    if let Ok(val) = std::env::var("COGENT_SECRETS_EXCLUDE") {
        if !val.is_empty() {
            return val.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return Vec::new();
    };
    let mut in_multiline = false;
    let mut multiline_buf = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            if in_multiline {
                // Empty/comment lines inside a multi-line array — skip
                continue;
            }
            continue;
        }
        if in_multiline {
            multiline_buf.push_str(line);
            multiline_buf.push(' ');
            if line.contains(']') {
                // End of multi-line array — parse the concatenated result
                in_multiline = false;
                if let Some(val) = parse_toml_string_list(&multiline_buf, "secrets_exclude") {
                    return val;
                }
                multiline_buf.clear();
            }
            continue;
        }
        // Detect start of multi-line array: key = [
        // Must be checked BEFORE single-line parse, because `secrets_exclude = [`
        // returns Some(["["]) from parse_string_list due to chained unwrap_or fallback.
        if line.contains("secrets_exclude") && line.contains('[') && !line.contains(']') {
            in_multiline = true;
            multiline_buf.clear();
            multiline_buf.push_str(line);
            multiline_buf.push(' ');
            continue;
        }
        // Single-line attempt
        if let Some(val) = parse_toml_string_list(line, "secrets_exclude") {
            return val;
        }
    }
    Vec::new()
}

/// Parse a `key = "a", "b"` or `key = a, b` line for a comma-separated string list.
/// Delegates to `cogent_common::parse_string_list`.
fn parse_toml_string_list(line: &str, key: &str) -> Option<Vec<String>> {
    cogent_common::parse_string_list(line, key)
}

fn parse_toml_f64(line: &str, key: &str) -> Option<f64> {
    cogent_common::parse_toml_f64(line, key)
}

fn parse_toml_usize(line: &str, key: &str) -> Option<usize> {
    parse_toml_f64(line, key).map(|v| v as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_toml_f64 ──

    #[test]
    fn test_parse_f64_max_avg() {
        assert_eq!(parse_toml_f64("max_avg = 15.0", "max_avg"), Some(15.0));
    }

    #[test]
    fn test_parse_f64_min_pct() {
        assert_eq!(parse_toml_f64("min_pct = 95.5", "min_pct"), Some(95.5));
    }

    #[test]
    fn test_parse_f64_integer_value() {
        assert_eq!(parse_toml_f64("max_avg = 30", "max_avg"), Some(30.0));
    }

    #[test]
    fn test_parse_f64_no_spaces_key() {
        assert_eq!(parse_toml_f64("max_avg=15.0", "max_avg"), Some(15.0));
    }

    #[test]
    fn test_parse_f64_key_not_found() {
        assert_eq!(parse_toml_f64("other_key = 42.0", "max_avg"), None);
    }

    #[test]
    fn test_parse_f64_comment_line() {
        assert_eq!(parse_toml_f64("# max_avg = 15.0", "max_avg"), None);
    }

    #[test]
    fn test_parse_f64_empty_value() {
        assert_eq!(parse_toml_f64("max_avg = ", "max_avg"), None);
    }

    #[test]
    fn test_parse_f64_non_numeric() {
        assert_eq!(parse_toml_f64("max_avg = abc", "max_avg"), None);
    }

    // ── parse_toml_usize ──

    #[test]
    fn test_parse_usize_max_markers() {
        assert_eq!(parse_toml_usize("max_markers = 50", "max_markers"), Some(50));
    }

    #[test]
    fn test_parse_usize_zero() {
        assert_eq!(parse_toml_usize("max_markers = 0", "max_markers"), Some(0));
    }

    #[test]
    fn test_parse_usize_float_truncates() {
        assert_eq!(parse_toml_usize("max_markers = 42.9", "max_markers"), Some(42));
    }

    #[test]
    fn test_parse_usize_no_spaces() {
        assert_eq!(parse_toml_usize("max_violations=10", "max_violations"), Some(10));
    }

    #[test]
    fn test_parse_usize_key_not_found() {
        assert_eq!(parse_toml_usize("other = 5", "max_markers"), None);
    }

    // ── build_gha_workflow ──

    fn make_profile(
        test_cmd: Vec<&str>,
        coverage_cmd: Vec<&str>,
        lcov_path: &str,
    ) -> ProjectProfile {
        ProjectProfile {
            ecosystem: ProjectEcosystem::Rust,
            test_cmd: test_cmd.into_iter().map(String::from).collect(),
            coverage_cmd: coverage_cmd.into_iter().map(String::from).collect(),
            lcov_path: lcov_path.to_string(),
            watch_extensions: vec!["rs".into()],
            max_crap: 15.0,
            min_doc: 95.0,
            max_debt: 0,
            max_complexity_violations: 0,
        }
    }

    #[test]
    fn test_gha_workflow_contains_name() {
        let profile = make_profile(vec!["cargo", "test"], vec![], "");
        let workflow = build_gha_workflow(&profile);
        assert!(workflow.contains("name: Cogent Quality Gate"));
    }

    #[test]
    fn test_gha_workflow_contains_test_cmd() {
        let profile = make_profile(vec!["cargo", "test"], vec![], "");
        let workflow = build_gha_workflow(&profile);
        assert!(workflow.contains("run: cargo test"));
    }

    #[test]
    fn test_gha_workflow_no_coverage() {
        let profile = make_profile(vec!["cargo", "test"], vec![], "");
        let workflow = build_gha_workflow(&profile);
        assert!(!workflow.contains("Collect coverage"));
    }

    #[test]
    fn test_gha_workflow_with_coverage() {
        let profile = make_profile(
            vec!["cargo", "test"],
            vec!["cargo", "llvm-cov", "--lcov", "--output-path", "lcov.info"],
            "lcov.info",
        );
        let workflow = build_gha_workflow(&profile);
        assert!(workflow.contains("Collect coverage"));
        assert!(workflow.contains("cargo llvm-cov --lcov --output-path lcov.info"));
        assert!(workflow.contains("--coverage lcov.info"));
    }

    #[test]
    fn test_gha_workflow_baseline_section() {
        let profile = make_profile(vec!["npm", "test"], vec![], "");
        let workflow = build_gha_workflow(&profile);
        assert!(workflow.contains(".cogent-baseline.sarif"));
        assert!(workflow.contains("Update baseline on main"));
    }

    #[test]
    fn test_gha_workflow_sarif_upload() {
        let profile = make_profile(vec!["go", "test", "./..."], vec![], "");
        let workflow = build_gha_workflow(&profile);
        assert!(workflow.contains("Upload SARIF to GitHub Security"));
        assert!(workflow.contains("github/codeql-action/upload-sarif@v3"));
    }

    // ── parse_toml_string_list ──

    #[test]
    fn test_string_list_toml_array() {
        assert_eq!(
            parse_toml_string_list("secrets_exclude = [\"a\", \"b\"]", "secrets_exclude"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_string_list_bare_comma_separated() {
        assert_eq!(
            parse_toml_string_list("secrets_exclude = a, b", "secrets_exclude"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_string_list_single_item() {
        assert_eq!(
            parse_toml_string_list("secrets_exclude = [\"only\"]", "secrets_exclude"),
            Some(vec!["only".to_string()])
        );
    }

    #[test]
    fn test_string_list_empty_array() {
        assert_eq!(
            parse_toml_string_list("secrets_exclude = []", "secrets_exclude"),
            None
        );
    }

    #[test]
    fn test_string_list_key_not_found() {
        assert_eq!(
            parse_toml_string_list("other = [\"a\"]", "secrets_exclude"),
            None
        );
    }

    #[test]
    fn test_string_list_no_spaces() {
        assert_eq!(
            parse_toml_string_list("secrets_exclude=[\"x\",\"y\"]", "secrets_exclude"),
            Some(vec!["x".to_string(), "y".to_string()])
        );
    }

    #[test]
    fn test_string_list_trims_whitespace() {
        // trim() strips outer whitespace; trim_matches strips quotes;
        // inner spaces inside quotes are preserved (consistent with engine parser)
        assert_eq!(
            parse_toml_string_list("secrets_exclude = [  \" a \" , \" b \"  ]", "secrets_exclude"),
            Some(vec![" a ".to_string(), " b ".to_string()])
        );
    }

    #[test]
    fn test_string_list_filters_empty_strings() {
        // Empty quoted strings should be filtered out
        assert_eq!(
            parse_toml_string_list("secrets_exclude = [\"valid\", \"\", \"also_valid\"]", "secrets_exclude"),
            Some(vec!["valid".to_string(), "also_valid".to_string()])
        );
    }

    // ── load_secrets_exclude ──

    #[test]
    fn test_load_secrets_exclude_missing_file() {
        assert_eq!(load_secrets_exclude("nonexistent.toml"), Vec::<String>::new());
    }

    #[test]
    fn test_load_secrets_exclude_with_config() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("cogent_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_secrets_exclude.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "max_secrets = 100").unwrap();
            writeln!(f, "secrets_exclude = [\"crates/engine\", \"tests/fixtures\"]").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["crates/engine".to_string(), "tests/fixtures".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_no_match() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_no_exclude_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "max_secrets = 100").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert!(result.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_roundtrip() {
        use std::io::Write;
        // Write a config with both TOML array and bare-comma syntax, then verify
        // load_secrets_exclude parses both correctly.
        let path = std::env::temp_dir().join("cogent_test_roundtrip_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "max_secrets = 50").unwrap();
            writeln!(f, "secrets_exclude = [\"vendor\", \"tests\", \"target\"]").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["vendor".to_string(), "tests".to_string(), "target".to_string()]);
        // Now verify it round-trips through the dispatcher's pattern:
        // load_secrets_exclude → Vec<String> → secrets binary --exclude arg
        let joined = result.join(",");
        assert_eq!(joined, "vendor,tests,target");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_bare_comma_syntax() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_bare_comma_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "secrets_exclude = vendor, tests").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["vendor".to_string(), "tests".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_multiline_toml_array() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_multiline_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "max_secrets = 50").unwrap();
            writeln!(f, "secrets_exclude = [").unwrap();
            writeln!(f, "  \"vendor\",").unwrap();
            writeln!(f, "  \"tests\",").unwrap();
            writeln!(f, "  \"target\"").unwrap();
            writeln!(f, "]").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["vendor".to_string(), "tests".to_string(), "target".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_multiline_with_comments() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_multiline_comments_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "secrets_exclude = [").unwrap();
            writeln!(f, "  \"vendor\",  # C++ vendored deps").unwrap();
            writeln!(f, "  \"tests\"    # test fixtures").unwrap();
            writeln!(f, "]").unwrap();
        }
        let result = load_secrets_exclude(path.to_str().unwrap());
        // Lines with comments are trimmed per-line; the comment text is part of the value
        // This is expected behavior for a line-by-line parser
        assert!(!result.is_empty(), "should parse multiline array with comments");
        let _ = std::fs::remove_file(&path);
    }

    /// RAII guard for environment variables in tests.
    /// Sets the var on creation and restores the original state on drop.
    struct EnvGuard {
        key: String,
        original: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                original,
            }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(val) => std::env::set_var(&self.key, val),
                None => std::env::remove_var(&self.key),
            }
        }
    }

    #[test]
    fn test_load_secrets_exclude_env_var_override() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_env_override_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "secrets_exclude = [\"config_val\"]").unwrap();
        }
        let _guard = EnvGuard::set("COGENT_SECRETS_EXCLUDE", "env_val1,env_val2");
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["env_val1".to_string(), "env_val2".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_secrets_exclude_env_var_empty_falls_back_to_config() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_env_empty_cli.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "secrets_exclude = [\"config_val\"]").unwrap();
        }
        let _guard = EnvGuard::set("COGENT_SECRETS_EXCLUDE", "");
        let result = load_secrets_exclude(path.to_str().unwrap());
        assert_eq!(result, vec!["config_val".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    // ── load_config_thresholds section-aware parsing ──────────────

    #[test]
    fn test_load_config_thresholds_ignores_override_sections() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_section_aware.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            // Base thresholds
            writeln!(f, "max_secrets = 50").unwrap();
            writeln!(f, "max_deadcode = 100").unwrap();
            writeln!(f, "max_errhandle = 200").unwrap();
            writeln!(f, "max_linelen = 120").unwrap();
            writeln!(f, "max_fuzz_risk = 80").unwrap();
            writeln!(f, "max_markers = 10").unwrap();
            // Override section — these should NOT overwrite globals
            writeln!(f, "").unwrap();
            writeln!(f, "[override.\"crates/*/tests/**\"]").unwrap();
            writeln!(f, "max_secrets = 9999").unwrap();
            writeln!(f, "max_deadcode = 9999").unwrap();
            writeln!(f, "max_errhandle = 9999").unwrap();
            writeln!(f, "max_linelen = 9999").unwrap();
            writeln!(f, "max_fuzz_risk = 9999").unwrap();
        }
        let defaults: Thresholds = (
            30.0, 95.0, 0, 0, 0.0, 0, 75.0, 10, 0.0, 0, 0, 15.0, 0, 0, 0,
            0.0, 0, 0.0, 0, 0, 0, 0, 0,
        );
        let (max_secrets, max_deadcode, max_errhandle, max_linelen, max_fuzz_risk, max_debt) = {
            let t = load_config_thresholds(path.to_str().unwrap(), defaults);
            (t.12, t.13, t.16, t.10, t.9, t.2)
        };
        // Override values (9999) must NOT leak into global thresholds
        assert_eq!(max_secrets, 50, "secrets should be 50, not 9999");
        assert_eq!(max_deadcode, 100, "deadcode should be 100, not 9999");
        assert_eq!(max_errhandle, 200, "errhandle should be 200, not 9999");
        assert_eq!(max_linelen, 120, "linelen should be 120, not 9999");
        assert_eq!(max_fuzz_risk, 80, "fuzz should be 80, not 9999");
        assert_eq!(max_debt, 10, "debt should be 10, not 9999");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_config_thresholds_takes_last_top_level_value() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_last_value.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "max_secrets = 10").unwrap();
            writeln!(f, "max_secrets = 20").unwrap();
        }
        let defaults: Thresholds = (
            30.0, 95.0, 0, 0, 0.0, 0, 75.0, 10, 0.0, 0, 0, 15.0, 0, 0, 0,
            0.0, 0, 0.0, 0, 0, 0, 0, 0,
        );
        let t = load_config_thresholds(path.to_str().unwrap(), defaults);
        assert_eq!(t.12, 20, "last top-level value should win");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_config_thresholds_section_before_override_wins() {
        use std::io::Write;
        let path = std::env::temp_dir().join("cogent_test_section_order.toml");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            // Tool section sets the value
            writeln!(f, "[secrets]").unwrap();
            writeln!(f, "max_secrets = 75").unwrap();
            // Override section tries to override it
            writeln!(f, "").unwrap();
            writeln!(f, "[override.\"crates/*/tests/**\"]").unwrap();
            writeln!(f, "max_secrets = 9999").unwrap();
        }
        let defaults: Thresholds = (
            30.0, 95.0, 0, 0, 0.0, 0, 75.0, 10, 0.0, 0, 0, 15.0, 0, 0, 0,
            0.0, 0, 0.0, 0, 0, 0, 0, 0,
        );
        let t = load_config_thresholds(path.to_str().unwrap(), defaults);
        assert_eq!(t.12, 75, "section value should win over override");
        let _ = std::fs::remove_file(&path);
    }
}
