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
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
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

fn parse_toml_f64(line: &str, key: &str) -> Option<f64> {
    let prefix = format!("{} =", key);
    let prefix2 = format!("{}=", key);
    let rest = line
        .strip_prefix(&prefix)
        .or_else(|| line.strip_prefix(&prefix2))?;
    rest.split_whitespace().next()?.parse().ok()
}

fn parse_toml_usize(line: &str, key: &str) -> Option<usize> {
    parse_toml_f64(line, key).map(|v| v as usize)
}
