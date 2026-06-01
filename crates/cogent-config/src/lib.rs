// ═══════════════════════════════════════════
// PROJECT DETECTION
// ═══════════════════════════════════════════

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
        // Prefer vitest if vitest.config exists, otherwise fall back to jest/npm test
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

// ═══════════════════════════════════════════
// CONFIG GENERATION
// ═══════════════════════════════════════════

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
    std::fs::write(output, config).expect("Failed to write config");
}

// ═══════════════════════════════════════════
// THRESHOLD TYPES
// ═══════════════════════════════════════════

/// Global threshold tuple: 23 fields matching all check thresholds.
/// Order: max_crap, min_doc, max_debt, max_complexity, max_duplication,
///   max_taint, max_risk, max_coupling, min_propcov, max_fuzz_risk,
///   max_linelen, max_halstead_bugs, max_secrets, max_deadcode,
///   max_cohesion, min_comment_ratio, max_errhandle, min_typecov,
///   max_vuln_critical, max_vuln_high, max_sast, max_crypto, max_license_violations
pub type Thresholds = (
    f64,
    f64,
    usize,
    usize,
    f64,
    usize,
    f64,
    usize,
    f64,
    usize,
    usize,
    f64,
    usize,
    usize,
    usize,
    f64,
    usize,
    f64,
    usize,
    usize,
    usize,
    usize,
    usize,
);

/// Per-path override storage: path glob → (threshold_key → value).
/// Keys match the TOML keys: "max_avg", "min_pct", "max_markers", etc.
pub type PathOverrides = std::collections::HashMap<String, std::collections::HashMap<String, f64>>;

// ═══════════════════════════════════════════
// CONFIG SECTION (intermediate parsed state)
// ═══════════════════════════════════════════

#[derive(Default)]
pub struct ConfigSection {
    pub max_crap: Option<f64>,
    pub min_doc: Option<f64>,
    pub max_debt: Option<usize>,
    pub max_complexity: Option<usize>,
    pub max_duplication: Option<f64>,
    pub max_taint: Option<usize>,
    pub max_risk: Option<f64>,
    pub max_coupling: Option<usize>,
    pub min_propcov: Option<f64>,
    pub max_fuzz_risk: Option<usize>,
    pub max_linelen: Option<usize>,
    pub max_halstead_bugs: Option<f64>,
    pub max_secrets: Option<usize>,
    pub max_deadcode: Option<usize>,
    pub max_cohesion: Option<usize>,
    pub min_comment_ratio: Option<f64>,
    pub max_errhandle: Option<usize>,
    pub min_typecov: Option<f64>,
    pub max_vuln_critical: Option<usize>,
    pub max_vuln_high: Option<usize>,
    pub max_sast: Option<usize>,
    pub max_crypto: Option<usize>,
    pub max_license_violations: Option<usize>,
}

impl ConfigSection {
    pub fn to_thresholds(&self, defaults: Thresholds) -> Thresholds {
        (
            self.max_crap.unwrap_or(defaults.0),
            self.min_doc.unwrap_or(defaults.1),
            self.max_debt.unwrap_or(defaults.2),
            self.max_complexity.unwrap_or(defaults.3),
            self.max_duplication.unwrap_or(defaults.4),
            self.max_taint.unwrap_or(defaults.5),
            self.max_risk.unwrap_or(defaults.6),
            self.max_coupling.unwrap_or(defaults.7),
            self.min_propcov.unwrap_or(defaults.8),
            self.max_fuzz_risk.unwrap_or(defaults.9),
            self.max_linelen.unwrap_or(defaults.10),
            self.max_halstead_bugs.unwrap_or(defaults.11),
            self.max_secrets.unwrap_or(defaults.12),
            self.max_deadcode.unwrap_or(defaults.13),
            self.max_cohesion.unwrap_or(defaults.14),
            self.min_comment_ratio.unwrap_or(defaults.15),
            self.max_errhandle.unwrap_or(defaults.16),
            self.min_typecov.unwrap_or(defaults.17),
            self.max_vuln_critical.unwrap_or(defaults.18),
            self.max_vuln_high.unwrap_or(defaults.19),
            self.max_sast.unwrap_or(defaults.20),
            self.max_crypto.unwrap_or(defaults.21),
            self.max_license_violations.unwrap_or(defaults.22),
        )
    }

    pub fn from_thresholds(t: Thresholds) -> Self {
        Self {
            max_crap: Some(t.0),
            min_doc: Some(t.1),
            max_debt: Some(t.2),
            max_complexity: Some(t.3),
            max_duplication: Some(t.4),
            max_taint: Some(t.5),
            max_risk: Some(t.6),
            max_coupling: Some(t.7),
            min_propcov: Some(t.8),
            max_fuzz_risk: Some(t.9),
            max_linelen: Some(t.10),
            max_halstead_bugs: Some(t.11),
            max_secrets: Some(t.12),
            max_deadcode: Some(t.13),
            max_cohesion: Some(t.14),
            min_comment_ratio: Some(t.15),
            max_errhandle: Some(t.16),
            min_typecov: Some(t.17),
            max_vuln_critical: Some(t.18),
            max_vuln_high: Some(t.19),
            max_sast: Some(t.20),
            max_crypto: Some(t.21),
            max_license_violations: Some(t.22),
        }
    }
}

// ═══════════════════════════════════════════
// TOML PARSING HELPERS
// ═══════════════════════════════════════════

pub fn parse_line_into(line: &str, section: &mut ConfigSection) {
    if let Some(v) = parse_toml_f64(line, "max_avg") {
        section.max_crap = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "min_pct") {
        section.min_doc = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_markers") {
        section.max_debt = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_violations") {
        section.max_complexity = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "max_duplicates") {
        section.max_duplication = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_taint") {
        section.max_taint = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "max_risk") {
        section.max_risk = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_coupling") {
        section.max_coupling = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "min_propcov") {
        section.min_propcov = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_fuzz_risk") {
        section.max_fuzz_risk = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_linelen") {
        section.max_linelen = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "max_halstead_bugs") {
        section.max_halstead_bugs = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_secrets") {
        section.max_secrets = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_deadcode") {
        section.max_deadcode = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_cohesion") {
        section.max_cohesion = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "min_comment_ratio") {
        section.min_comment_ratio = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_errhandle") {
        section.max_errhandle = Some(v);
    }
    if let Some(v) = parse_toml_f64(line, "min_typecov") {
        section.min_typecov = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_vuln_critical") {
        section.max_vuln_critical = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_vuln_high") {
        section.max_vuln_high = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_sast") {
        section.max_sast = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_crypto") {
        section.max_crypto = Some(v);
    }
    if let Some(v) = parse_toml_usize(line, "max_license_violations") {
        section.max_license_violations = Some(v);
    }
}

pub fn parse_toml_f64(line: &str, key: &str) -> Option<f64> {
    let prefix = format!("{} =", key);
    let prefix2 = format!("{}=", key);
    let rest = if let Some(r) = line.strip_prefix(&prefix) {
        r
    } else {
        line.strip_prefix(&prefix2)?
    };
    rest.split_whitespace().next()?.parse().ok()
}

pub fn parse_toml_usize(line: &str, key: &str) -> Option<usize> {
    parse_toml_f64(line, key).map(|v| v as usize)
}

// ═══════════════════════════════════════════
// CONFIG LOADING
// ═══════════════════════════════════════════

/// Load thresholds from `.quality.toml` if present, falling back to `defaults`.
/// Parsing is intentionally line-by-line to avoid pulling in a TOML dependency.
/// Now section-aware: supports `[crap]`, `[errhandle]`, and `[override."path/glob"]`.
pub fn load_config_with_overrides(
    config_path: &str,
    defaults: Thresholds,
) -> (Thresholds, PathOverrides) {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return (defaults, std::collections::HashMap::new());
    };

    let mut global = ConfigSection::default();
    let mut overrides: PathOverrides = std::collections::HashMap::new();
    let mut current_override: Option<(String, ConfigSection)> = None;

    let mut flush_override = |ov: &mut Option<(String, ConfigSection)>| {
        if let Some((pat, sec)) = ov.take() {
            let mut map = std::collections::HashMap::new();
            if let Some(v) = sec.max_crap {
                map.insert("max_avg".into(), v);
            }
            if let Some(v) = sec.min_doc {
                map.insert("min_pct".into(), v);
            }
            if let Some(v) = sec.max_debt {
                map.insert("max_markers".into(), v as f64);
            }
            if let Some(v) = sec.max_complexity {
                map.insert("max_violations".into(), v as f64);
            }
            if let Some(v) = sec.max_duplication {
                map.insert("max_duplicates".into(), v);
            }
            if let Some(v) = sec.max_taint {
                map.insert("max_taint".into(), v as f64);
            }
            if let Some(v) = sec.max_risk {
                map.insert("max_risk".into(), v);
            }
            if let Some(v) = sec.max_coupling {
                map.insert("max_coupling".into(), v as f64);
            }
            if let Some(v) = sec.min_propcov {
                map.insert("min_propcov".into(), v);
            }
            if let Some(v) = sec.max_fuzz_risk {
                map.insert("max_fuzz_risk".into(), v as f64);
            }
            if let Some(v) = sec.max_linelen {
                map.insert("max_linelen".into(), v as f64);
            }
            if let Some(v) = sec.max_halstead_bugs {
                map.insert("max_halstead_bugs".into(), v);
            }
            if let Some(v) = sec.max_secrets {
                map.insert("max_secrets".into(), v as f64);
            }
            if let Some(v) = sec.max_deadcode {
                map.insert("max_deadcode".into(), v as f64);
            }
            if let Some(v) = sec.max_cohesion {
                map.insert("max_cohesion".into(), v as f64);
            }
            if let Some(v) = sec.min_comment_ratio {
                map.insert("min_comment_ratio".into(), v);
            }
            if let Some(v) = sec.max_errhandle {
                map.insert("max_errhandle".into(), v as f64);
            }
            if let Some(v) = sec.min_typecov {
                map.insert("min_typecov".into(), v);
            }
            if let Some(v) = sec.max_vuln_critical {
                map.insert("max_vuln_critical".into(), v as f64);
            }
            if let Some(v) = sec.max_vuln_high {
                map.insert("max_vuln_high".into(), v as f64);
            }
            if let Some(v) = sec.max_sast {
                map.insert("max_sast".into(), v as f64);
            }
            if let Some(v) = sec.max_crypto {
                map.insert("max_crypto".into(), v as f64);
            }
            if let Some(v) = sec.max_license_violations {
                map.insert("max_license_violations".into(), v as f64);
            }
            overrides.insert(pat, map);
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        // Section header: [crap]  or  [override."crates/*/tests/**"]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let sec = &trimmed[1..trimmed.len() - 1];
            if sec.starts_with("override.") {
                flush_override(&mut current_override);
                let pat = sec["override.".len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string();
                current_override = Some((pat, ConfigSection::default()));
            } else {
                flush_override(&mut current_override);
            }
            continue;
        }

        if let Some((_, ref mut section)) = current_override {
            parse_line_into(trimmed, section);
        } else {
            parse_line_into(trimmed, &mut global);
        }
    }

    flush_override(&mut current_override);

    let thresholds = global.to_thresholds(defaults);
    (thresholds, overrides)
}

pub fn load_config_thresholds(config_path: &str, defaults: Thresholds) -> Thresholds {
    load_config_with_overrides(config_path, defaults).0
}

// ═══════════════════════════════════════════
// THRESHOLD RESOLUTION (with glob matching)
// ═══════════════════════════════════════════

/// Simple glob matcher supporting `*` (within a single path component) and `**` (any depth).
/// Does NOT use an external crate — handles the patterns that matter in practice.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    // Normalise separators
    let pat = pattern
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    let tgt = path.replace('\\', "/");
    let tgt = tgt.trim_start_matches("./");
    glob_match_inner(&pat, tgt)
}

fn glob_match_inner(pat: &str, tgt: &str) -> bool {
    if pat.is_empty() {
        return tgt.is_empty();
    }
    if pat == "**" {
        return true;
    }
    if let Some(rest_pat) = pat.strip_prefix("**/") {
        // `**` matches zero or more path components
        if glob_match_inner(rest_pat, tgt) {
            return true;
        }
        // Try consuming one component of tgt
        if let Some(slash) = tgt.find('/') {
            return glob_match_inner(pat, &tgt[slash + 1..]);
        }
        return false;
    }
    // Split on first '/'
    let (pat_seg, pat_rest) = match pat.find('/') {
        Some(i) => (&pat[..i], &pat[i + 1..]),
        None => (pat, ""),
    };
    let (tgt_seg, tgt_rest) = match tgt.find('/') {
        Some(i) => (&tgt[..i], &tgt[i + 1..]),
        None => (tgt, ""),
    };
    if segment_matches(pat_seg, tgt_seg) {
        if pat_rest.is_empty() && tgt_rest.is_empty() {
            return true;
        }
        if !pat_rest.is_empty() && !tgt_rest.is_empty() {
            return glob_match_inner(pat_rest, tgt_rest);
        }
    }
    false
}

fn segment_matches(pat: &str, tgt: &str) -> bool {
    if pat == "*" {
        return true;
    }
    if !pat.contains('*') {
        return pat == tgt;
    }
    // Simple wildcard within a segment
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !tgt.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if let Some(found) = tgt[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }
    true
}

/// Resolve the effective threshold for `check_name` on `file_path`.
/// Checks per-path overrides first (via glob match), falls back to global.
pub fn resolve_threshold(
    global: Thresholds,
    overrides: &PathOverrides,
    file: &str,
    _check: &str,
) -> Thresholds {
    let mut effective = global;
    for (pat, vals) in overrides {
        if glob_matches(pat, file) {
            let mut section = ConfigSection::from_thresholds(effective);
            for (key, val) in vals {
                match key.as_str() {
                    "max_avg" => section.max_crap = Some(*val),
                    "min_pct" => section.min_doc = Some(*val),
                    "max_markers" => section.max_debt = Some(*val as usize),
                    "max_violations" => section.max_complexity = Some(*val as usize),
                    "max_duplicates" => section.max_duplication = Some(*val),
                    "max_taint" => section.max_taint = Some(*val as usize),
                    "max_risk" => section.max_risk = Some(*val),
                    "max_coupling" => section.max_coupling = Some(*val as usize),
                    "min_propcov" => section.min_propcov = Some(*val),
                    "max_fuzz_risk" => section.max_fuzz_risk = Some(*val as usize),
                    "max_linelen" => section.max_linelen = Some(*val as usize),
                    "max_halstead_bugs" => section.max_halstead_bugs = Some(*val),
                    "max_secrets" => section.max_secrets = Some(*val as usize),
                    "max_deadcode" => section.max_deadcode = Some(*val as usize),
                    "max_cohesion" => section.max_cohesion = Some(*val as usize),
                    "min_comment_ratio" => section.min_comment_ratio = Some(*val),
                    "max_errhandle" => section.max_errhandle = Some(*val as usize),
                    "min_typecov" => section.min_typecov = Some(*val),
                    "max_vuln_critical" => section.max_vuln_critical = Some(*val as usize),
                    "max_vuln_high" => section.max_vuln_high = Some(*val as usize),
                    "max_sast" => section.max_sast = Some(*val as usize),
                    "max_crypto" => section.max_crypto = Some(*val as usize),
                    "max_license_violations" => {
                        section.max_license_violations = Some(*val as usize)
                    }
                    _ => {}
                }
            }
            effective = section.to_thresholds(effective);
        }
    }
    effective
}
