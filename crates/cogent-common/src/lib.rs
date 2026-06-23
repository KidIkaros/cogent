#![deny(clippy::all)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthChar;

pub mod error;
pub mod memory;
pub mod types;

pub use error::CogentError;
pub use types::*;

// ═══════════════════════════════════════════
// AUDIT OPINION MODEL
// ═══════════════════════════════════════════

/// Audit opinion — mirrors professional audit firm language.
/// - **UnqualifiedPass**: all gate killers pass, weighted score ≥ 80
/// - **QualifiedPass**: all gate killers pass, weighted score 60–79
/// - **Adverse**: one or more gate killers failed
/// - **Disclaimer**: too many tools unavailable (5+ skipped)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditOpinion {
    UnqualifiedPass,
    QualifiedPass,
    Adverse,
    Disclaimer,
}

impl std::fmt::Display for AuditOpinion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnqualifiedPass => write!(f, "UNQUALIFIED PASS"),
            Self::QualifiedPass => write!(f, "QUALIFIED PASS"),
            Self::Adverse => write!(f, "ADVERSE"),
            Self::Disclaimer => write!(f, "DISCLAIMER"),
        }
    }
}

/// Per-category weighted score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScore {
    pub name: String,
    pub weight: u32,
    pub score: f64,
    pub checks_passed: usize,
    pub checks_total: usize,
}

/// Full audit result: opinion, weighted score, gate killers, category breakdown, margin risks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub opinion: AuditOpinion,
    pub overall_score: u32,
    pub grade: char,
    pub gate_killers_passed: bool,
    pub gate_killer_names: Vec<String>,
    pub gate_killer_passed_names: Vec<String>,
    /// Gate killers that were not present in the check results (skipped/not run).
    /// Missing gate killers cause `gate_killers_passed` to be false, which
    /// triggers an Adverse opinion. This prevents projects that skip security
    /// checks from receiving an Unqualified Pass.
    #[serde(default)]
    pub missing_gate_killers: Vec<String>,
    pub categories: Vec<CategoryScore>,
    pub margin_risks: Vec<(String, f64)>,
    pub unavailable_count: usize,
}

/// Compute the full audit opinion from check results.
///
/// Tier 1 — Gate Killers (binary pass/fail):
///   secrets, vulnscan, sast, taint — any failure → Adverse
///
/// Tier 2 — Weighted Category Scores (0–100 each):
///   Security 5×, Compliance 3×, Quality 2×, Hygiene 1×, Operations 1×
///
/// Tier 3 — Margin-to-threshold (closest to failing)
pub fn compute_audit(checks: &[CheckResult]) -> AuditResult {
    // ── Gate Killers ──
    let gate_killer_names: Vec<String> = ["secrets", "vulnscan", "sast", "taint"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut gate_killer_passed_names = Vec::new();
    let mut missing_gate_killers = Vec::new();
    for gk in &gate_killer_names {
        if let Some(c) = checks.iter().find(|c| &c.name == gk) {
            if c.passed {
                gate_killer_passed_names.push(gk.clone());
            }
            // If c.passed is false, it's a failure — don't add to passed list
        } else {
            // Gate killer not run — track as missing (not implicitly passing).
            // This ensures a project that skips `secrets` doesn't get an
            // UNQUALIFIED PASS. Missing gate killers cause gate_killers_passed=false,
            // which triggers AuditOpinion::Adverse.
            missing_gate_killers.push(gk.clone());
        }
    }
    let gate_killers_passed = gate_killer_passed_names.len() == gate_killer_names.len();

    // ── Category definitions ──
    let security_tools = [
        "secrets",
        "sast",
        "crypto",
        "taint",
        "vulnscan",
        "access-control",
        "errhandle",
    ];
    let compliance_tools = ["licenses", "sbom", "supply-chain", "outdated"];
    let quality_tools = [
        "crap",
        "complexity",
        "deadcode",
        "coupling",
        "dupfind",
        "duplication",
        "riskmap",
        "halstead",
        "cohesion",
        "fuzz",
        "propcov",
    ];
    let hygiene_tools = [
        "debt",
        "comments",
        "linelen",
        "doccov",
        "doc_coverage",
        "typecov",
    ];
    let operations_tools = [
        "observability",
        "test-quality",
        "design-docs",
        "debuggability",
    ];

    let cat_defs: &[(&str, u32, &[&str])] = &[
        ("Security", 5, &security_tools),
        ("Compliance", 3, &compliance_tools),
        ("Quality", 2, &quality_tools),
        ("Hygiene", 1, &hygiene_tools),
        ("Operations", 1, &operations_tools),
    ];

    // ── Category scores ──
    let mut categories = Vec::new();
    for &(name, weight, tools) in cat_defs {
        let cat_checks: Vec<&CheckResult> = checks
            .iter()
            .filter(|c| tools.contains(&c.name.as_str()))
            .collect();
        if cat_checks.is_empty() {
            continue;
        }
        let passed = cat_checks.iter().filter(|c| c.passed).count();
        let total = cat_checks.len();
        let score = if total > 0 {
            passed as f64 / total as f64 * 100.0
        } else {
            100.0
        };
        categories.push(CategoryScore {
            name: name.to_string(),
            weight,
            score,
            checks_passed: passed,
            checks_total: total,
        });
    }

    // ── Weighted overall score ──
    let mut weighted_sum = 0.0f64;
    let mut weight_total = 0u32;
    for cat in &categories {
        weighted_sum += cat.score * cat.weight as f64;
        weight_total += cat.weight;
    }
    let overall_score = if weight_total > 0 {
        (weighted_sum / weight_total as f64) as u32
    } else {
        100
    };
    let grade = match overall_score {
        90..=100 => 'A',
        80..=89 => 'B',
        65..=79 => 'C',
        50..=64 => 'D',
        _ => 'F',
    };

    // ── Unavailable count ──
    let unavailable_count = checks
        .iter()
        .filter(|c| c.message.starts_with("Skipped"))
        .count();

    // ── Opinion ──
    // Empty checks → vacuous truth: no failures = UnqualifiedPass.
    // Missing gate killers only trigger Adverse when other checks exist
    // (meaning the user deliberately ran checks but skipped gate killers).
    let has_any_checks = !checks.is_empty();
    let gate_killer_failure = has_any_checks && !gate_killers_passed;
    let opinion = if unavailable_count >= 5 {
        AuditOpinion::Disclaimer
    } else if gate_killer_failure {
        AuditOpinion::Adverse
    } else if overall_score >= 80 {
        AuditOpinion::UnqualifiedPass
    } else {
        AuditOpinion::QualifiedPass
    };

    // ── Margin risks (top 3 closest to failing) ──
    let mut margin_risks: Vec<(String, f64)> = checks
        .iter()
        .filter(|c| c.passed)
        .filter_map(|c| {
            let score = c.score?;
            let threshold = c.threshold?;
            if threshold == 0.0 {
                return None;
            }
            let inverted = matches!(
                c.name.as_str(),
                "doc_coverage" | "doccov" | "propcov" | "typecov"
            );
            let margin = if inverted {
                ((score - threshold) / threshold * 100.0).clamp(0.0, 100.0)
            } else {
                ((threshold - score) / threshold * 100.0).clamp(0.0, 100.0)
            };
            if margin < 25.0 {
                Some((c.name.clone(), margin))
            } else {
                None
            }
        })
        .collect();
    margin_risks.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    margin_risks.truncate(3);

    AuditResult {
        opinion,
        overall_score,
        grade,
        gate_killers_passed,
        gate_killer_names,
        gate_killer_passed_names,
        missing_gate_killers,
        categories,
        margin_risks,
        unavailable_count,
    }
}

/// Compute a weighted health score 0–100 and letter grade.
/// Security failures penalise harder (×3), compliance (×2), quality (×1).
///
/// This uses a simpler two-tier model than [`compute_audit`], which has a full
/// five-category breakdown (Security, Compliance, Quality, Hygiene, Operations)
/// with gate killers and opinion semantics. `health_score` is retained for
/// backward compatibility and lightweight summary displays.
pub fn health_score(checks: &[CheckResult]) -> (u32, char) {
    let security = [
        "secrets",
        "vulnscan",
        "taint",
        "errhandle",
        "sast",
        "crypto",
    ];
    let compliance = ["licenses", "sbom"];
    if checks.is_empty() {
        return (100, 'A');
    }
    let mut weighted_pass = 0u32;
    let mut weighted_total = 0u32;
    for c in checks {
        let w = if security.contains(&c.name.as_str()) {
            3
        } else if compliance.contains(&c.name.as_str()) {
            2
        } else {
            1
        };
        weighted_total += w;
        if c.passed {
            weighted_pass += w;
        }
    }
    let score = weighted_pass.checked_mul(100).unwrap_or(0) / weighted_total.max(1);
    let grade = match score {
        90..=100 => 'A',
        80..=89 => 'B',
        65..=79 => 'C',
        50..=64 => 'D',
        _ => 'F',
    };
    (score, grade)
}

/// Combined report from running multiple tools in one batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedReport {
    pub run_id: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub tools: Vec<ToolResult>,
    pub summary: ReportSummary,
}

/// Summary of a batch run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_tools: usize,
    pub passed: usize,
    pub failed: usize,
    pub languages_detected: Vec<String>,
}

/// Convenience: wrap raw tool data into a ToolResponse envelope.
pub fn wrap_tool_response(
    tool: &str,
    version: &str,
    success: bool,
    duration_ms: u64,
    data: serde_json::Value,
    summary: Option<serde_json::Value>,
    error: Option<String>,
) -> ToolResponse {
    ToolResponse {
        tool: tool.to_string(),
        version: version.to_string(),
        success,
        duration_ms,
        data,
        summary,
        error,
        suggested_fix: None,
        auto_fix_available: None,
    }
}

/// Convenience: create a new UnifiedReport with a generated run_id.
pub fn new_unified_report(started_at: String) -> UnifiedReport {
    UnifiedReport {
        run_id: format!("run-{}", uuid::Uuid::new_v4()),
        started_at,
        duration_ms: 0,
        tools: Vec::new(),
        summary: ReportSummary {
            total_tools: 0,
            passed: 0,
            failed: 0,
            languages_detected: Vec::new(),
        },
    }
}

// ═══════════════════════════════════════════
// COGENT INFRASTRUCTURE SKIP LIST
// ═══════════════════════════════════════════

/// Tool crates whose names are distinctive enough to avoid false positives in
/// real user projects. Each pattern is surrounded by `/` to avoid substring
/// collisions.
const DISTINCTIVE_INFRA_PATTERNS: &[&str] = &[
    "/sast/",
    "/crypto-check/",
    "/mutation-test/",
    "/risk-map/",
    "/fuzz-surface/",
    "/access-control/",
    "/taint-scan/",
    "/vuln-scan/",
    "/debt-scan/",
    "/ast-parse-ts/",
];

/// Generic tool-crate names that could collide with user directories — scoped to
/// `/crates/<name>/` so only workspace crate paths match.
const GENERIC_INFRA_CRATE_PATTERNS: &[&str] = &[
    "/crates/secrets/",
    "/crates/dead-code/",
    "/crates/duplication/",
    "/crates/comment-ratio/",
    "/crates/coupling/",
    "/crates/cohesion/",
    "/crates/halstead/",
    "/crates/prop-cov/",
    "/crates/error-handling/",
    "/crates/type-coverage/",
    "/crates/line-length/",
    "/crates/doc-coverage/",
    "/crates/crap-metric/",
    "/crates/licenses/",
    "/crates/supply-chain/",
    "/crates/sbom/",
];

/// Returns `true` if `path` belongs to Cogent tool infrastructure and should be
/// excluded from self-scanning to avoid false positives.
///
/// Individual tools (sast, crypto-check, etc.) call this instead of maintaining
/// their own ad-hoc skip lists, so newly-added crates are covered automatically.
pub fn is_cogent_infra_path(path: &str) -> bool {
    // All cogent-* prefixed crates (cli, common, config, report, engine, server, fix)
    if path.contains("/cogent-") {
        return true;
    }
    DISTINCTIVE_INFRA_PATTERNS.iter().any(|p| path.contains(p))
        || GENERIC_INFRA_CRATE_PATTERNS
            .iter()
            .any(|p| path.contains(p))
}

// ═══════════════════════════════════════════
// FILE DISCOVERY
// ═══════════════════════════════════════════

/// Find all Rust source files at a path (file or directory).
pub fn find_rust_files(path: &str, recursive: bool) -> Vec<String> {
    let path = Path::new(path);
    let mut files = Vec::new();

    if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
        files.push(path.to_string_lossy().to_string());
    } else if path.is_dir() {
        scan_dir(path, recursive, &["rs"], &mut files);
    }

    files.sort();
    files
}

/// Find source files with any of the given extensions.
pub fn find_source_files(path: &str, recursive: bool, extensions: &[&str]) -> Vec<String> {
    let path = Path::new(path);
    let mut files = Vec::new();

    if path.is_file() {
        if let Some(ext) = path.extension() {
            if extensions.contains(&ext.to_string_lossy().as_ref()) {
                files.push(path.to_string_lossy().to_string());
            }
        }
    } else if path.is_dir() {
        scan_dir(path, recursive, extensions, &mut files);
    }

    files.sort();
    files
}

/// Check whether a file path has one of the given extensions.
fn should_include_file(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .is_some_and(|ext| extensions.contains(&ext.to_string_lossy().as_ref()))
}

/// Check whether a directory should be traversed (not a skipped/hidden dir).
fn should_scan_dir(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    !matches!(
        name.as_ref(),
        "target" | ".git" | "node_modules" | "fixtures"
    ) && !name.starts_with('.')
}

/// Recursively scan a directory for files with given extensions.
/// Skips target/, .git/, node_modules/, fixtures/, and hidden directories.
pub fn scan_dir(dir: &Path, recursive: bool, extensions: &[&str], files: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && should_include_file(&path, extensions) {
            files.push(path.to_string_lossy().to_string());
        } else if recursive && path.is_dir() && should_scan_dir(&path) {
            scan_dir(&path, recursive, extensions, files);
        }
    }
}

// ═══════════════════════════════════════════
// SHARED PARSING UTILITIES
// ═══════════════════════════════════════════

/// Parse a TOML value (`key = <value>`) from a line of `.quality.toml` content.
/// Returns the numeric value as `f64`, or `None` if the key is not found.
///
/// Does NOT track sections — callers that need section-aware parsing should
/// maintain their own `current_section` state and skip lines in `[override.*]`
/// sections before calling this function. See `parse_toml_f64_aware` for a
/// ready-made section-tracking variant.
pub fn parse_toml_f64(line: &str, key: &str) -> Option<f64> {
    let prefix = format!("{} = ", key);
    let prefix2 = format!("{}= ", key);
    let prefix3 = format!("{} =", key);
    let prefix4 = format!("{}=", key);
    let rest = line
        .strip_prefix(&prefix)
        .or_else(|| line.strip_prefix(&prefix2))
        .or_else(|| line.strip_prefix(&prefix3))
        .or_else(|| line.strip_prefix(&prefix4))?;
    let val_str = rest.split_whitespace().next()?;
    // Strip trailing comment
    let val_str = val_str.split('#').next().unwrap_or(val_str).trim();
    val_str.parse::<f64>().ok()
}

/// Parse a TOML numeric key from content, tracking sections and skipping
/// `[override.*]` sections. This is the recommended way to read a single
/// threshold value from `.quality.toml`.
///
/// ```text
/// max_secrets = 135       // found, returns Some(135.0)
///
/// [override."crates/*/tests/**"]
/// max_secrets = 9999      // skipped (inside override section)
/// ```
pub fn parse_toml_f64_aware(content: &str, key: &str) -> Option<f64> {
    let mut in_override = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        // Track section headers: [section_name]
        if line.starts_with('[') && line.ends_with(']') && !line.starts_with("[[") {
            let section = &line[1..line.len() - 1];
            in_override = section.starts_with("override.");
            continue;
        }
        if in_override {
            continue;
        }
        if let Some(val) = parse_toml_f64(line, key) {
            return Some(val);
        }
    }
    None
}

/// Parse a TOML-style `key = "a", "b"` or `key = a, b` line for a comma-separated string list.
/// Handles TOML array syntax `["a", "b"]`, bare commas `a, b`, and single-quoted strings.
/// Inline comments (everything after an unquoted `#`) are stripped before parsing.
/// Returns `None` if the key is missing or the result is empty after filtering.
/// Empty strings and whitespace-only items are filtered out.
pub fn parse_string_list(line: &str, key: &str) -> Option<Vec<String>> {
    let prefix = format!("{} = ", key);
    let prefix2 = format!("{}= ", key);
    let prefix3 = format!("{} =", key);
    let prefix4 = format!("{}=", key);
    let rest = line
        .strip_prefix(&prefix)
        .or_else(|| line.strip_prefix(&prefix2))
        .or_else(|| line.strip_prefix(&prefix3))
        .or_else(|| line.strip_prefix(&prefix4))?;
    let rest = rest.trim();
    if rest == "[]" {
        return None;
    }
    // Strip inline comments FIRST (before bracket stripping), because comments
    // may appear after the closing bracket: `["a"]  # comment`.
    // Only strip `#` outside double quotes. TOML uses `"` for strings.
    let value = {
        let mut in_double = false;
        let mut comment_pos = None;
        for (i, ch) in rest.char_indices() {
            if ch == '"' {
                in_double = !in_double;
            } else if ch == '#' && !in_double {
                comment_pos = Some(i);
                break;
            }
        }
        match comment_pos {
            Some(pos) => rest[..pos].trim(),
            None => rest,
        }
    };
    // Strip optional brackets: `["a", "b"]` -> inner without brackets.
    // Uses explicit match to avoid chained unwrap_or(rest) bug where
    // strip_prefix succeeds but strip_suffix fails and unwrap_or falls back
    // to the ORIGINAL rest instead of the intermediate result.
    let inner = match value.strip_prefix('[') {
        Some(stripped) => stripped.strip_suffix(']').unwrap_or(stripped),
        None => value,
    };
    let items: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

// ═══════════════════════════════════════════
// STRING UTILITIES
// ═══════════════════════════════════════════

/// Truncate a string to `max` **display columns**, adding "…" prefix if truncated.
/// Keeps the RIGHT side (end) of the string.
/// Uses unicode visual width so multi-byte icons (✓ △ ✗ …) count correctly.
pub fn truncate(s: &str, max: usize) -> String {
    // Fast path: the string already fits
    let visual_len: usize = s
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if visual_len <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    // "…" occupies 1 column; budget = max - 1 columns for actual content
    let budget = max.saturating_sub(1);
    // Collect chars from the right until we fill the budget
    let mut cols = 0usize;
    let mut keep_from = s.len(); // byte offset
    for c in s.chars().rev() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if cols + w > budget {
            break;
        }
        cols += w;
        keep_from -= c.len_utf8();
    }
    format!("…{}", &s[keep_from..])
}

/// Truncate a string to `max` **display columns**, adding "…" suffix if truncated.
/// Keeps the LEFT side (start) of the string.
/// Uses unicode visual width so multi-byte icons (✓ △ ✗ …) count correctly.
pub fn truncate_left(s: &str, max: usize) -> String {
    let visual_len: usize = s
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if visual_len <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    // "…" occupies 1 column; budget = max - 1 columns for actual content
    let budget = max.saturating_sub(1);
    let mut cols = 0usize;
    let mut keep_to = 0usize; // byte offset
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if cols + w > budget {
            break;
        }
        cols += w;
        keep_to += c.len_utf8();
    }
    format!("{}…", &s[..keep_to])
}

// ═══════════════════════════════════════════
// LINE NUMBER ESTIMATION
// ═══════════════════════════════════════════

/// Estimate the line number of a pattern in source code.
pub fn estimate_line(source: &str, pattern: &str) -> usize {
    for (i, line) in source.lines().enumerate() {
        if line.contains(pattern) {
            return i + 1;
        }
    }
    1
}

/// Estimate line number of a function definition.
pub fn estimate_fn_line(source: &str, fn_name: &str) -> usize {
    estimate_line(source, &format!("fn {}", fn_name))
}

// ═══════════════════════════════════════════
// OUTPUT FORMATTING HELPERS
// ═══════════════════════════════════════════

/// Print a standard separator line.
pub fn separator(width: usize) -> String {
    "─".repeat(width)
}

/// Print a section header.
pub fn section_header(title: &str) {
    println!();
    println!("{}", title);
    println!("{}", separator(title.len().max(40)));
}

// ═══════════════════════════════════════════
// TABLE FORMATTING
// ═══════════════════════════════════════════

/// A column in a table output.
pub struct Column {
    pub header: &'static str,
    pub width: usize,
    pub align_right: bool,
}

impl Column {
    /// Create a left-aligned column.
    pub fn left(header: &'static str, width: usize) -> Self {
        Self {
            header,
            width,
            align_right: false,
        }
    }
    /// Create a right-aligned column.
    pub fn right(header: &'static str, width: usize) -> Self {
        Self {
            header,
            width,
            align_right: true,
        }
    }
}

/// Print a table header row.
pub fn print_table_header(columns: &[Column]) {
    let mut line = String::new();
    for col in columns {
        if col.align_right {
            line.push_str(&format!("{:>width$} ", col.header, width = col.width));
        } else {
            line.push_str(&format!("{:<width$} ", col.header, width = col.width));
        }
    }
    println!("{}", line.trim_end());
    let total_width: usize = columns.iter().map(|c| c.width + 1).sum();
    println!("{}", separator(total_width));
}

/// Print a table row with values.
pub fn print_table_row(columns: &[Column], values: &[&str]) {
    let mut line = String::new();
    for (col, val) in columns.iter().zip(values.iter()) {
        let truncated = truncate(val, col.width);
        if col.align_right {
            line.push_str(&format!("{:>width$} ", truncated, width = col.width));
        } else {
            line.push_str(&format!("{:<width$} ", truncated, width = col.width));
        }
    }
    println!("{}", line.trim_end());
}

/// Print a summary section with key-value pairs.
pub fn print_summary(items: &[(&str, String)]) {
    println!();
    for (key, value) in items {
        println!("  {:<25} {}", key, value);
    }
}

/// Print a verdict line with icon.
pub fn print_verdict(score: f64, good_threshold: f64, label_good: &str, label_bad: &str) {
    if score <= good_threshold {
        println!("\n  ✓ {:.1} — {}", score, label_good);
    } else {
        println!("\n  ✗ {:.1} — {}", score, label_bad);
    }
}

// ═══════════════════════════════════════════
// GIT INTEGRATION
// ═══════════════════════════════════════════

/// Get git churn data: file -> number of commits since a date.
pub fn get_git_churn(repo_root: &Path, since: &str) -> std::collections::HashMap<String, u32> {
    use std::collections::HashMap;
    use std::process::Command;

    let output = Command::new("git")
        .args(["log", "--since", since, "--name-only", "--pretty=format:"])
        .current_dir(repo_root)
        .output();

    let mut churn: HashMap<String, u32> = HashMap::new();

    if let Ok(output) = output {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let file = line.trim();
                if !file.is_empty() && !file.starts_with('.') {
                    *churn.entry(file.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    churn
}

/// Get git blame info for a specific line.
pub fn get_git_blame(file_path: &str, line: usize) -> (Option<String>, Option<String>) {
    use std::process::Command;

    let output = Command::new("git")
        .args([
            "blame",
            "-L",
            &format!("{},{}", line, line),
            "--porcelain",
            file_path,
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut author = None;
            let mut date = None;

            for line in text.lines() {
                if let Some(name) = line.strip_prefix("author ") {
                    author = Some(name.to_string());
                }
                if let Some(d) = line.strip_prefix("author-time ") {
                    if let Ok(ts) = d.parse::<i64>() {
                        date = Some(format_timestamp(ts));
                    }
                }
            }

            (author, date)
        }
        _ => (None, None),
    }
}

/// Get git blame info for multiple lines in a file efficiently.
/// Returns a HashMap mapping line number to (author, date).
pub fn get_git_blame_batch(
    file_path: &str,
    lines: &[usize],
) -> std::collections::HashMap<usize, (Option<String>, Option<String>)> {
    use std::collections::HashMap;
    use std::process::Command;

    if lines.is_empty() {
        return HashMap::new();
    }

    // Sort and deduplicate lines
    let mut sorted_lines = lines.to_vec();
    sorted_lines.sort_unstable();
    sorted_lines.dedup();

    // Build line ranges to minimize git blame calls
    // Group consecutive lines into ranges
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut range_start = sorted_lines[0];
    let mut prev_line = sorted_lines[0];

    for &line in &sorted_lines[1..] {
        if line == prev_line + 1 {
            // Consecutive, extend current range
            prev_line = line;
        } else {
            // Gap, close current range and start new one
            ranges.push((range_start, prev_line));
            range_start = line;
            prev_line = line;
        }
    }
    ranges.push((range_start, prev_line));

    // Call git blame for each range and collect results
    let mut results: HashMap<usize, (Option<String>, Option<String>)> = HashMap::new();

    for (start, end) in ranges {
        let output = Command::new("git")
            .args([
                "blame",
                "-L",
                &format!("{},{}", start, end),
                "--porcelain",
                file_path,
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let mut current_line: Option<usize> = None;
                let mut current_author: Option<String> = None;
                let mut current_date: Option<String> = None;

                for line_text in text.lines() {
                    // Parse the header line which contains the original line number
                    // Format: <sha1> <original_line> <final_line> <line_count>
                    if line_text.starts_with('\t') {
                        // Content line - associate collected data with current line
                        if let Some(line_num) = current_line {
                            results
                                .insert(line_num, (current_author.clone(), current_date.clone()));
                        }
                    } else if let Some(author) = line_text.strip_prefix("author ") {
                        current_author = Some(author.to_string());
                    } else if let Some(time_str) = line_text.strip_prefix("author-time ") {
                        if let Ok(ts) = time_str.parse::<i64>() {
                            current_date = Some(format_timestamp(ts));
                        }
                    } else if line_text.len() >= 40
                        && !line_text.starts_with('\t')
                        && !line_text.starts_with("author")
                    {
                        // Header line: extract the original line number
                        // Format: <40-char-sha> <original-line> <final-line> <line-count>
                        let parts: Vec<&str> = line_text.split_whitespace().collect();
                        if parts.len() >= 3 {
                            if let Ok(orig_line) = parts[1].parse::<usize>() {
                                current_line = Some(orig_line);
                            }
                        }
                    }
                }
                // Don't forget the last entry
                if let Some(line_num) = current_line {
                    results.insert(line_num, (current_author.clone(), current_date.clone()));
                }
            }
        }
    }

    results
}

fn format_timestamp(ts: i64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    let remaining = days % 365;
    let month = remaining / 30 + 1;
    let day = remaining % 30 + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_cogent_infra_path ──

    #[test]
    fn test_is_cogent_infra_path_cogent_prefix() {
        assert!(is_cogent_infra_path("./crates/cogent-cli/src/main.rs"));
        assert!(is_cogent_infra_path("./crates/cogent-common/src/lib.rs"));
        assert!(is_cogent_infra_path("./crates/cogent-fix/src/main.rs"));
    }

    #[test]
    fn test_is_cogent_infra_path_distinctive() {
        assert!(is_cogent_infra_path("./crates/sast/src/main.rs"));
        assert!(is_cogent_infra_path("./crates/crypto-check/src/main.rs"));
        assert!(is_cogent_infra_path("./crates/mutation-test/src/main.rs"));
    }

    #[test]
    fn test_is_cogent_infra_path_generic_scoped() {
        assert!(is_cogent_infra_path("./crates/secrets/src/main.rs"));
        assert!(is_cogent_infra_path("./crates/licenses/src/main.rs"));
        assert!(is_cogent_infra_path("./crates/duplication/src/main.rs"));
    }

    #[test]
    fn test_is_cogent_infra_path_user_project_not_skipped() {
        assert!(!is_cogent_infra_path("./src/main.rs"));
        assert!(!is_cogent_infra_path("./my-app/secrets/manager.rs"));
        assert!(!is_cogent_infra_path("./lib/duplication/utils.rs"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "…world");
        assert_eq!(truncate("hi", 1), "…");
        assert_eq!(truncate("hi", 0), "");
        // Unicode icons: "✓ ok" — '✓' is 1 col wide in most terminals
        assert_eq!(truncate("✓ ok", 10), "✓ ok");
        // Multi-char: truncate to 3 cols → "…ok" (3 cols)
        let r = truncate("✓ hello", 3);
        assert_eq!(
            r.len(),
            "…ok".len(),
            "truncated string byte length unexpected"
        );
        assert!(r.starts_with('…'));
    }

    #[test]
    fn test_truncate_left() {
        assert_eq!(truncate_left("hello", 10), "hello");
        assert_eq!(truncate_left("hello world", 6), "hello…");
        assert_eq!(truncate_left("hi", 0), "");
        // Unicode icon keeps left side
        assert_eq!(truncate_left("✓ ok", 10), "✓ ok");
        let r = truncate_left("hello ✓", 4);
        assert!(r.ends_with('…'), "should end with ellipsis, got: {r}");
    }

    #[test]
    fn test_estimate_line() {
        let source = "fn main() {\n    let x = 1;\n    println!(\"hi\");\n}";
        assert_eq!(estimate_line(source, "fn main"), 1);
        assert_eq!(estimate_line(source, "println"), 3);
        assert_eq!(estimate_line(source, "missing"), 1);
    }

    #[test]
    fn test_estimate_fn_line() {
        let source = "fn foo() {}\n\nfn bar() {\n    x\n}";
        assert_eq!(estimate_fn_line(source, "foo"), 1);
        assert_eq!(estimate_fn_line(source, "bar"), 3);
    }

    // ── parse_toml_f64 / parse_toml_f64_aware ──

    #[test]
    fn test_parse_toml_f64_basic() {
        assert_eq!(parse_toml_f64("max_avg = 15.0", "max_avg"), Some(15.0));
    }

    #[test]
    fn test_parse_toml_f64_integer() {
        assert_eq!(
            parse_toml_f64("max_secrets = 30", "max_secrets"),
            Some(30.0)
        );
    }

    #[test]
    fn test_parse_toml_f64_no_space() {
        assert_eq!(parse_toml_f64("max_avg=15.0", "max_avg"), Some(15.0));
    }

    #[test]
    fn test_parse_toml_f64_missing_key() {
        assert_eq!(parse_toml_f64("other_key = 42.0", "max_avg"), None);
    }

    #[test]
    fn test_parse_toml_f64_comment() {
        assert_eq!(parse_toml_f64("# max_avg = 15.0", "max_avg"), None);
    }

    #[test]
    fn test_parse_toml_f64_trailing_comment() {
        assert_eq!(
            parse_toml_f64("max_avg = 15.0 # threshold", "max_avg"),
            Some(15.0)
        );
    }

    #[test]
    fn test_parse_toml_f64_aware_skips_override_sections() {
        let content = r#"max_secrets = 135

[override."crates/*/tests/**"]
max_secrets = 9999"#;
        assert_eq!(parse_toml_f64_aware(content, "max_secrets"), Some(135.0));
    }

    #[test]
    fn test_parse_toml_f64_aware_section_before_override_wins() {
        let content = r#"[secrets]
max_secrets = 75

[override."crates/*/tests/**"]
max_secrets = 9999"#;
        assert_eq!(parse_toml_f64_aware(content, "max_secrets"), Some(75.0));
    }

    #[test]
    fn test_parse_toml_f64_aware_no_sections() {
        let content = "max_avg = 63.0\nmax_markers = 18\n";
        assert_eq!(parse_toml_f64_aware(content, "max_avg"), Some(63.0));
        assert_eq!(parse_toml_f64_aware(content, "max_markers"), Some(18.0));
    }

    #[test]
    fn test_parse_toml_f64_aware_returns_none_for_missing() {
        let content = "max_avg = 63.0\n";
        assert_eq!(parse_toml_f64_aware(content, "nonexistent"), None);
    }

    // ── compute_audit ─────────────────────────────────────────────────────

    fn make_audit_check(name: &str, passed: bool) -> CheckResult {
        CheckResult {
            name: name.into(),
            passed,
            score: None,
            threshold: None,
            message: String::new(),
            details: serde_json::Value::Null,
            severity: None,
            help: None,
            rule_id: None,
            findings: vec![],
        }
    }

    #[test]
    fn test_compute_audit_all_pass() {
        let checks = vec![
            make_audit_check("secrets", true),
            make_audit_check("vulnscan", true),
            make_audit_check("sast", true),
            make_audit_check("taint", true),
            make_audit_check("crap", true),
            make_audit_check("debt", true),
        ];
        let audit = compute_audit(&checks);
        assert!(audit.gate_killers_passed);
        assert_eq!(audit.opinion, AuditOpinion::UnqualifiedPass);
        assert!(
            audit.overall_score >= 80,
            "score should be >= 80, got {}",
            audit.overall_score
        );
    }

    #[test]
    fn test_compute_audit_gate_killer_fail_is_adverse() {
        let checks = vec![
            make_audit_check("secrets", false),
            make_audit_check("vulnscan", true),
            make_audit_check("sast", true),
            make_audit_check("taint", true),
            make_audit_check("crap", true),
        ];
        let audit = compute_audit(&checks);
        assert!(!audit.gate_killers_passed);
        assert_eq!(audit.opinion, AuditOpinion::Adverse);
        assert!(audit.gate_killer_names.contains(&"secrets".to_string()));
    }

    #[test]
    fn test_compute_audit_disclaimer_when_many_unavailable() {
        let mut checks: Vec<CheckResult> = Vec::new();
        for i in 0..6 {
            let mut c = make_audit_check(&format!("tool_{}", i), true);
            c.message = "Skipped: tool not available".into();
            checks.push(c);
        }
        let audit = compute_audit(&checks);
        assert_eq!(audit.opinion, AuditOpinion::Disclaimer);
        assert!(
            audit.unavailable_count >= 5,
            "should detect 5+ unavailable, got {}",
            audit.unavailable_count
        );
    }

    #[test]
    fn test_compute_audit_qualified_pass() {
        // All 4 gate killers present and passing, but weighted score is 60-79
        let checks = vec![
            make_audit_check("secrets", true),
            make_audit_check("vulnscan", true),
            make_audit_check("sast", true),
            make_audit_check("taint", true),
            make_audit_check("crap", false),
            make_audit_check("complexity", false),
            make_audit_check("deadcode", false),
            make_audit_check("coupling", false),
            make_audit_check("dupfind", true),
            make_audit_check("debt", false),
            make_audit_check("comments", false),
            make_audit_check("linelen", false),
            make_audit_check("doccov", true),
        ];
        let audit = compute_audit(&checks);
        assert!(audit.gate_killers_passed);
        assert_eq!(
            audit.opinion,
            AuditOpinion::QualifiedPass,
            "score {} should trigger QualifiedPass",
            audit.overall_score
        );
    }

    #[test]
    fn test_compute_audit_empty_checks() {
        let audit = compute_audit(&[]);
        // All 4 gate killers are missing → gate_killers_passed=false
        assert!(!audit.gate_killers_passed);
        assert_eq!(audit.missing_gate_killers.len(), 4);
        // But with no checks at all, vacuous truth: no failures = UnqualifiedPass
        assert_eq!(audit.opinion, AuditOpinion::UnqualifiedPass);
    }

    #[test]
    fn test_compute_audit_categories_populated() {
        let checks = vec![
            make_audit_check("secrets", true),
            make_audit_check("sast", true),
            make_audit_check("licenses", true),
            make_audit_check("crap", true),
            make_audit_check("debt", true),
            make_audit_check("observability", true),
        ];
        let audit = compute_audit(&checks);
        let cat_names: Vec<&str> = audit.categories.iter().map(|c| c.name.as_str()).collect();
        assert!(cat_names.contains(&"Security"));
        assert!(cat_names.contains(&"Compliance"));
        assert!(cat_names.contains(&"Quality"));
        assert!(cat_names.contains(&"Hygiene"));
        assert!(cat_names.contains(&"Operations"));
    }

    #[test]
    fn test_compute_audit_gate_killer_not_run_tracked_as_missing() {
        let checks = vec![
            make_audit_check("secrets", true),
            make_audit_check("crap", true),
        ];
        let audit = compute_audit(&checks);
        // Only secrets is present and passed; vulnscan, sast, taint are missing
        // Missing gate killers cause gate_killers_passed=false because not ALL 4 passed
        assert!(
            !audit.gate_killers_passed,
            "missing gate killers should not pass"
        );
        assert_eq!(
            audit.gate_killer_passed_names.len(),
            1,
            "only secrets is present and passed"
        );
        assert_eq!(
            audit.missing_gate_killers.len(),
            3,
            "3 gate killers not run: vulnscan, sast, taint"
        );
        assert!(audit.missing_gate_killers.contains(&"vulnscan".to_string()));
        assert!(audit.missing_gate_killers.contains(&"sast".to_string()));
        assert!(audit.missing_gate_killers.contains(&"taint".to_string()));
    }

    // ── health_score (existing tests) ─────────────────────────────────────

    #[test]
    fn test_health_score_all_pass() {
        let checks = vec![
            CheckResult {
                name: "crap".into(),
                passed: true,
                score: None,
                threshold: None,
                message: "".into(),
                details: serde_json::Value::Null,
                severity: None,
                help: None,
                rule_id: None,
                findings: vec![],
            },
            CheckResult {
                name: "secrets".into(),
                passed: true,
                score: None,
                threshold: None,
                message: "".into(),
                details: serde_json::Value::Null,
                severity: None,
                help: None,
                rule_id: None,
                findings: vec![],
            },
        ];
        let (score, grade) = health_score(&checks);
        assert_eq!(score, 100);
        assert_eq!(grade, 'A');
    }

    #[test]
    fn test_health_score_security_fail_weights_heavier() {
        let checks = vec![
            CheckResult {
                name: "crap".into(),
                passed: true,
                score: None,
                threshold: None,
                message: "".into(),
                details: serde_json::Value::Null,
                severity: None,
                help: None,
                rule_id: None,
                findings: vec![],
            },
            CheckResult {
                name: "secrets".into(),
                passed: false,
                score: None,
                threshold: None,
                message: "".into(),
                details: serde_json::Value::Null,
                severity: None,
                help: None,
                rule_id: None,
                findings: vec![],
            },
        ];
        // security weights 3, quality weights 1 → pass=1, total=4 → 25%
        let (score, grade) = health_score(&checks);
        assert_eq!(score, 25);
        assert_eq!(grade, 'F');
    }

    #[test]
    fn test_health_score_empty() {
        let (score, grade) = health_score(&[]);
        assert_eq!(score, 100);
        assert_eq!(grade, 'A');
    }

    #[test]
    fn test_checkresult_roundtrip_json() {
        let cr = CheckResult {
            name: "debt".into(),
            passed: false,
            score: Some(12.0),
            threshold: Some(10.0),
            message: "found markers".into(),
            details: serde_json::json!({"items": [{"file": "a.rs", "line": 1}]}),
            severity: Some("medium".into()),
            help: Some("remove todos".into()),
            rule_id: Some("debt-marker".into()),
            findings: vec![],
        };
        let json = serde_json::to_string(&cr).unwrap();
        assert!(json.contains("debt"));
        assert!(json.contains("found markers"));
        let back: CheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "debt");
        assert!(!back.passed);
        assert_eq!(back.score, Some(12.0));
    }

    // ── separator ──

    #[test]
    fn test_separator_default() {
        let s = separator(10);
        assert_eq!(s.chars().count(), 10);
        assert!(s.chars().all(|c| c == '─'));
    }

    #[test]
    fn test_separator_zero() {
        assert_eq!(separator(0), "");
    }

    // ── Column builders ──

    #[test]
    fn test_column_left() {
        let c = Column::left("Name", 20);
        assert_eq!(c.header, "Name");
        assert_eq!(c.width, 20);
        assert!(!c.align_right);
    }

    #[test]
    fn test_column_right() {
        let c = Column::right("Count", 6);
        assert_eq!(c.header, "Count");
        assert_eq!(c.width, 6);
        assert!(c.align_right);
    }

    // ── wrap_tool_response ──

    #[test]
    fn test_wrap_tool_response_basic() {
        let resp = wrap_tool_response(
            "test-tool",
            "1.0",
            true,
            42,
            serde_json::json!({"key": "val"}),
            None,
            None,
        );
        assert_eq!(resp.tool, "test-tool");
        assert_eq!(resp.version, "1.0");
        assert!(resp.success);
        assert_eq!(resp.duration_ms, 42);
        assert_eq!(resp.data["key"], "val");
        assert!(resp.summary.is_none());
        assert!(resp.error.is_none());
        assert!(resp.suggested_fix.is_none());
    }

    #[test]
    fn test_wrap_tool_response_with_summary_and_error() {
        let resp = wrap_tool_response(
            "x",
            "2.0",
            false,
            100,
            serde_json::Value::Null,
            Some(serde_json::json!({})),
            Some("error msg".into()),
        );
        assert!(!resp.success);
        assert_eq!(resp.error, Some("error msg".into()));
        assert_eq!(resp.summary, Some(serde_json::json!({})));
    }

    // ── new_unified_report ──

    #[test]
    fn test_new_unified_report() {
        let report = new_unified_report("2024-01-01".into());
        assert!(report.run_id.starts_with("run-"));
        assert_eq!(report.started_at, "2024-01-01");
        assert_eq!(report.duration_ms, 0);
        assert!(report.tools.is_empty());
        assert_eq!(report.summary.total_tools, 0);
    }

    // ── function_coverage ──

    #[test]
    fn test_function_coverage_found_with_hits() {
        let records = vec![
            CoverageRecord {
                function: "foo".into(),
                line: 10,
                hits: 5,
            },
            CoverageRecord {
                function: "bar".into(),
                line: 20,
                hits: 0,
            },
        ];
        assert_eq!(function_coverage(&records, "foo"), 1.0);
    }

    #[test]
    fn test_function_coverage_found_no_hits() {
        let records = vec![CoverageRecord {
            function: "foo".into(),
            line: 10,
            hits: 0,
        }];
        assert_eq!(function_coverage(&records, "foo"), 0.0);
    }

    #[test]
    fn test_function_coverage_not_found() {
        let records = vec![CoverageRecord {
            function: "foo".into(),
            line: 10,
            hits: 5,
        }];
        assert_eq!(function_coverage(&records, "nonexistent"), 0.0);
    }

    #[test]
    fn test_function_coverage_empty_records() {
        let records: Vec<CoverageRecord> = vec![];
        assert_eq!(function_coverage(&records, "foo"), 0.0);
    }

    // ── crap_score ──

    #[test]
    fn test_crap_score_zero_complexity() {
        let score = crap_score(0, 0.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_crap_score_no_coverage() {
        let score = crap_score(5, 0.0);
        // 5^2 * (1-0)^3 + 5 = 25 + 5 = 30
        assert!((score - 30.0).abs() < 1e-9);
    }

    #[test]
    fn test_crap_score_full_coverage() {
        let score = crap_score(5, 1.0);
        // 5^2 * (1-1)^3 + 5 = 0 + 5 = 5
        assert!((score - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_crap_score_partial_coverage() {
        let score = crap_score(10, 0.5);
        // 10^2 * (1-0.5)^3 + 10 = 100 * 0.125 + 10 = 22.5
        assert!((score - 22.5).abs() < 1e-9);
    }

    #[test]
    fn test_crap_score_clamps_cov_above_1() {
        let score = crap_score(3, 2.0);
        // clamped to 1.0: 3^2 * 0 + 3 = 3
        assert!((score - 3.0).abs() < 1e-9);
    }

    // ── crap_category ──

    #[test]
    fn test_crap_category_excellent() {
        assert_eq!(crap_category(0.0), "excellent");
        assert_eq!(crap_category(10.0), "excellent");
    }

    #[test]
    fn test_crap_category_good() {
        assert_eq!(crap_category(15.0), "good");
        assert_eq!(crap_category(20.0), "good");
    }

    #[test]
    fn test_crap_category_acceptable() {
        assert_eq!(crap_category(25.0), "acceptable");
        assert_eq!(crap_category(30.0), "acceptable");
    }

    #[test]
    fn test_crap_category_crappy() {
        assert_eq!(crap_category(31.0), "crappy");
        assert_eq!(crap_category(100.0), "crappy");
    }

    // ── sarif_level ──

    #[test]
    fn test_sarif_level_error() {
        assert_eq!(sarif_level("error"), "error");
        assert_eq!(sarif_level("critical"), "error");
        assert_eq!(sarif_level("high"), "error");
    }

    #[test]
    fn test_sarif_level_warning() {
        assert_eq!(sarif_level("warning"), "warning");
        assert_eq!(sarif_level("medium"), "warning");
    }

    #[test]
    fn test_sarif_level_note() {
        assert_eq!(sarif_level("note"), "note");
        assert_eq!(sarif_level("info"), "note");
        assert_eq!(sarif_level("low"), "note");
    }

    #[test]
    fn test_sarif_level_unknown_defaults_to_warning() {
        assert_eq!(sarif_level("unknown"), "warning");
        assert_eq!(sarif_level(""), "warning");
    }

    // ── get_rule_details ──

    #[test]
    fn test_get_rule_details_known() {
        let (short, full, help) = get_rule_details("crap-error");
        assert_eq!(short, "CRAP Score Too High");
        assert!(full.contains("CRAP"));
        assert!(help.contains("Reduce complexity"));
    }

    #[test]
    fn test_get_rule_details_default() {
        let (short, full, help) = get_rule_details("unknown-rule");
        assert_eq!(short, "Rule unknown-rule");
        assert_eq!(full, "Details for rule unknown-rule");
    }

    // ── format_timestamp ──

    #[test]
    fn test_format_timestamp_epoch() {
        assert_eq!(format_timestamp(0), "1970-01-01");
    }

    #[test]
    fn test_format_timestamp_later() {
        // Approx 2024-06-15: days = 19889
        let ts = 19889i64 * 86400;
        let s = format_timestamp(ts);
        assert!(s.starts_with("202"));
        assert_eq!(s.len(), 10);
    }

    // ── demangle_rust_v0 ──

    #[test]
    fn test_demangle_non_mangled() {
        assert_eq!(demangle_rust_v0("my_function"), "my_function");
    }

    #[test]
    fn test_demangle_v0_extracts_last_ident() {
        // _R (v0 prefix) Nv (fn nesting) CsXXX (crate) 4crate 17my_function_name
        let result = demangle_rust_v0("_RNvCs1234_4crate16my_function_name");
        assert_eq!(result, "my_function_name");
    }

    #[test]
    fn test_demangle_v0_short_sym() {
        let result = demangle_rust_v0("_R");
        // No parseable identifiers, returns the original
        assert_eq!(result, "_R");
    }

    // ── parse_lcov ──

    #[test]
    fn test_parse_lcov_empty() {
        assert!(parse_lcov("").is_empty());
    }

    #[test]
    fn test_parse_lcov_single_fn() {
        let lcov = "FN:10,my_func\nFNDA:5,my_func\n";
        let records = parse_lcov(lcov);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].function, "my_func");
        assert_eq!(records[0].line, 10);
        assert_eq!(records[0].hits, 5);
    }

    #[test]
    fn test_parse_lcov_multiple_fns() {
        let lcov = "FN:5,fn_a\nFNDA:3,fn_a\nFN:20,fn_b\nFNDA:7,fn_b\n";
        let records = parse_lcov(lcov);
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn test_parse_lcov_fnda_before_fn() {
        // FNDA without prior FN should insert a record with line=0
        let lcov = "FNDA:42,orphan\n";
        let records = parse_lcov(lcov);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].function, "orphan");
        assert_eq!(records[0].line, 0);
        assert_eq!(records[0].hits, 42);
    }

    // ── diff_results ──

    fn make_sarif_result(rule_id: &str, uri: &str, line: usize) -> SarifResult {
        SarifResult {
            rule_id: rule_id.to_string(),
            rule_index: None,
            level: "warning".to_string(),
            message: SarifMessage {
                text: "msg".to_string(),
            },
            locations: vec![SarifLocation {
                physical_location: SarifPhysicalLocation {
                    artifact_location: Some(SarifArtifactLocation {
                        uri: uri.to_string(),
                    }),
                    region: Some(SarifRegion {
                        start_line: Some(line),
                        start_column: None,
                        end_line: None,
                        end_column: None,
                    }),
                },
            }],
        }
    }

    #[test]
    fn test_diff_results_no_new() {
        let current = vec![make_sarif_result("R1", "a.rs", 1)];
        let baseline = vec![make_sarif_result("R1", "a.rs", 1)];
        assert!(diff_results(&current, &baseline).is_empty());
    }

    #[test]
    fn test_diff_results_new_finding() {
        let current = vec![
            make_sarif_result("R1", "a.rs", 1),
            make_sarif_result("R2", "b.rs", 5),
        ];
        let baseline = vec![make_sarif_result("R1", "a.rs", 1)];
        let new = diff_results(&current, &baseline);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].rule_id, "R2");
    }

    #[test]
    fn test_diff_results_all_new() {
        let current = vec![make_sarif_result("R3", "c.rs", 10)];
        let baseline = vec![make_sarif_result("R1", "a.rs", 1)];
        assert_eq!(diff_results(&current, &baseline).len(), 1);
    }

    // ── find_lcov_file ──

    #[test]
    fn test_find_lcov_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_lcov_file(dir.path()).is_none());
    }

    #[test]
    fn test_find_lcov_file_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lcov.info"), "").unwrap();
        assert!(find_lcov_file(dir.path()).is_some());
    }

    // ── find_coverage ──

    #[test]
    fn test_find_coverage_no_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_coverage(dir.path()).is_none());
    }

    #[test]
    fn test_find_coverage_with_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lcov.info"), "FN:1,foo\nFNDA:3,foo\n").unwrap();
        let cov = find_coverage(dir.path()).unwrap();
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].function, "foo");
        assert_eq!(cov[0].hits, 3);
    }

    // ── print_verdict ──

    #[test]
    fn test_print_verdict_passes_when_equal() {
        // Just verify it doesn't panic
        print_verdict(5.0, 5.0, "good", "bad");
    }

    #[test]
    fn test_print_verdict_passes_below() {
        print_verdict(3.0, 5.0, "good", "bad");
    }

    #[test]
    fn test_print_verdict_fails_above() {
        print_verdict(10.0, 5.0, "good", "bad");
    }

    // ── parse_string_list (canonical tests for shared parser) ──

    #[test]
    fn test_parse_string_list_toml_array() {
        assert_eq!(
            parse_string_list("secrets_exclude = [\"a\", \"b\"]", "secrets_exclude"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_bare_comma() {
        assert_eq!(
            parse_string_list("secrets_exclude = a, b", "secrets_exclude"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_single_quoted() {
        assert_eq!(
            parse_string_list("secrets_exclude = ['vendor', 'test']", "secrets_exclude"),
            Some(vec!["vendor".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_empty_array() {
        assert_eq!(
            parse_string_list("secrets_exclude = []", "secrets_exclude"),
            None
        );
    }

    #[test]
    fn test_parse_string_list_filters_empty_strings() {
        assert_eq!(
            parse_string_list(
                "secrets_exclude = [\"valid\", \"\", \"also_valid\"]",
                "secrets_exclude"
            ),
            Some(vec!["valid".to_string(), "also_valid".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_no_key() {
        assert_eq!(
            parse_string_list("other = [\"a\"]", "secrets_exclude"),
            None
        );
    }

    #[test]
    fn test_parse_string_list_no_space_after_eq() {
        assert_eq!(
            parse_string_list("secrets_exclude=[\"x\"]", "secrets_exclude"),
            Some(vec!["x".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_trailing_comma() {
        assert_eq!(
            parse_string_list("secrets_exclude = [\"a\", \"b\", ]", "secrets_exclude"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_paths_with_slashes() {
        assert_eq!(
            parse_string_list(
                "secrets_exclude = [\"crates/engine\", \"tests/fixtures\"]",
                "secrets_exclude"
            ),
            Some(vec![
                "crates/engine".to_string(),
                "tests/fixtures".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_string_list_opening_bracket_only() {
        // Regression: `secrets_exclude = [` used to return Some(vec!["["])
        // due to chained unwrap_or(rest) fallback. Should return None.
        assert_eq!(
            parse_string_list("secrets_exclude = [", "secrets_exclude"),
            None
        );
    }

    #[test]
    fn test_parse_string_list_inline_comment_stripped() {
        // Inline comments after # should be stripped
        assert_eq!(
            parse_string_list(
                "secrets_exclude = [\"vendor\", \"tests\"]  # excluded paths",
                "secrets_exclude"
            ),
            Some(vec!["vendor".to_string(), "tests".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_inline_comment_in_array() {
        // Comment inside array brackets strips everything after #
        // (including subsequent values — this is expected TOML-like behavior)
        assert_eq!(
            parse_string_list(
                "secrets_exclude = [\"a\" # first, \"b\"]",
                "secrets_exclude"
            ),
            Some(vec!["a".to_string()])
        );
    }

    #[test]
    fn test_parse_string_list_comment_only_after_value() {
        // Single value with trailing comment
        assert_eq!(
            parse_string_list("secrets_exclude = vendor  # skip this", "secrets_exclude"),
            Some(vec!["vendor".to_string()])
        );
    }

    // ── proptest fuzz tests for parse_string_list ──

    proptest::proptest! {
        #[test]
        fn test_parse_string_list_never_panics(line in ".*", key in "[a-z_]{1,20}") {
            // The parser must never panic on arbitrary input.
            let _ = parse_string_list(&line, &key);
        }

        #[test]
        fn test_parse_string_list_result_invariants(line in ".*", key in "[a-z_]{1,20}") {
            if let Some(items) = parse_string_list(&line, &key) {
                // No empty strings in result
                assert!(items.iter().all(|s| !s.is_empty()),
                    "empty string in result for key='{}', line='{}'", key, line);
                // No duplicates of empty strings
                assert_eq!(items.len(), items.iter().collect::<std::collections::HashSet<_>>().len(),
                    "duplicate items for key='{}', line='{}'", key, line);
            }
        }

        #[test]
        fn test_parse_string_list_toml_array_never_returns_bare_strings(parts in proptest::collection::vec("[a-zA-Z0-9_/-]{1,30}", 0..5)) {
            // A well-formed TOML array should parse correctly
            let inner: Vec<String> = parts.iter().map(|s| format!("\"{}\"", s)).collect();
            let line = format!("secrets_exclude = [{}]", inner.join(", "));
            let result = parse_string_list(&line, "secrets_exclude");
            if parts.iter().any(|p| !p.is_empty()) {
                assert!(result.is_some(), "TOML array should parse for: {}", line);
                let items = result.unwrap();
                assert_eq!(items.len(), parts.len());
                for (item, expected) in items.iter().zip(parts.iter()) {
                    assert_eq!(item, expected);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════
// SARIF OUTPUT
// ═══════════════════════════════════════════

/// Minimal SARIF v2.1.0 structures for GitHub Security / VS Code ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

/// A single run (execution) inside a SARIF log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocations: Option<Vec<SarifInvocation>>,
    pub results: Vec<SarifResult>,
}

/// Tool information inside a SARIF run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

/// Tool driver (the actual scanning tool).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
    pub rules: Vec<SarifRule>,
}

/// A rule that a result can reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifRule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<SarifMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_description: Option<SarifMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<SarifMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_configuration: Option<SarifRuleConfig>,
}

/// Default severity configuration for a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifRuleConfig {
    pub level: String,
}

/// A human-readable message in SARIF output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifMessage {
    pub text: String,
}

/// Metadata about a single tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifInvocation {
    pub execution_successful: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_utc: Option<String>,
}

/// One finding / result produced by the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifResult {
    pub rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_index: Option<usize>,
    pub level: String,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
}

/// A location where a result was found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifLocation {
    pub physical_location: SarifPhysicalLocation,
}

/// Physical file location with optional region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifPhysicalLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_location: Option<SarifArtifactLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

/// URI reference to an artifact (source file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

/// A line/column region inside a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifRegion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

impl SarifLog {
    /// Build a minimal SARIF log from a tool name and findings.
    pub fn new(_tool_name: &str, _tool_version: &str) -> Self {
        SarifLog {
            schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json".to_string(),
            version: "2.1.0".to_string(),
            runs: Vec::new(),
        }
    }

    /// Append a run to this SARIF log.
    pub fn add_run(&mut self, run: SarifRun) {
        self.runs.push(run);
    }
}

/// Convenience builder for a single-tool SARIF run.
/// Get detailed information about a rule for SARIF output.
///
/// Returns a tuple of (short_description, full_description, help_text)
/// for a given rule ID. This is used to populate SARIF rule definitions.
///
/// # Arguments
/// * `rule_id` - The rule ID to look up
///
/// # Returns
/// Tuple of (short_desc, full_desc, help_text)
pub fn get_rule_details(rule_id: &str) -> (String, String, String) {
    match rule_id {
        "crap-error" => (
            "CRAP Score Too High".to_string(),
            "The CRAP (Change Risk Anti-Patterns) score combines cyclomatic complexity with test coverage. A high CRAP score indicates code that is risky to maintain and modify.".to_string(),
            "To fix: 1) Reduce complexity by splitting the function into smaller parts. 2) Increase test coverage for the function. Target: CRAP < 15, complexity < 5, coverage > 90%.".to_string(),
        ),
        "debt-error" => (
            "Technical Debt Markers Found".to_string(),
            "Technical debt markers (TODO, FIXME, HACK, XXX) indicate future work that hasn't been done. These should be tracked in issue trackers, not left in code.".to_string(),
            "To fix: 1) Create issues for each marker in your project tracker. 2) Remove the markers from code. 3) Follow the 'zero debt' principle - no markers in committed code.".to_string(),
        ),
        "doc-error" => (
            "Documentation Coverage Too Low".to_string(),
            "Public API documentation helps users understand how to use your code. Low documentation coverage indicates missing doc comments on public functions, structs, or modules.".to_string(),
            "To fix: 1) Add doc comments (/// or /*!) to all public items. 2) Run 'doccov' to check coverage. Target: > 95% for public APIs.".to_string(),
        ),
        "complexity-error" => (
            "Cyclomatic Complexity Too High".to_string(),
            "Cyclomatic complexity measures the number of decision points in code. High complexity indicates functions that are hard to understand, test, and maintain.".to_string(),
            "To fix: 1) Split complex functions into smaller, focused functions. 2) Reduce nesting depth. 3) Use early returns to reduce cognitive load. Target: complexity < 5 per function.".to_string(),
        ),
        "duplication-error" => (
            "Code Duplication Detected".to_string(),
            "Duplicated code increases maintenance burden and the risk of inconsistent fixes. It should be extracted into shared functions or modules.".to_string(),
            "To fix: 1) Extract duplicated code into a shared function. 2) Use abstraction to eliminate redundancy. Target: 0 duplicates > 3 lines.".to_string(),
        ),
        _ => (
            format!("Rule {}", rule_id),
            format!("Details for rule {}", rule_id),
            "Review the finding and apply appropriate fixes.".to_string(),
        ),
    }
}

fn build_sarif_rules(rule_ids: Vec<String>) -> Vec<SarifRule> {
    rule_ids
        .into_iter()
        .map(|id| {
            let (short_desc, full_desc, help_text) = get_rule_details(&id);
            SarifRule {
                id: id.clone(),
                name: Some(id.clone()),
                short_description: Some(SarifMessage { text: short_desc }),
                full_description: Some(SarifMessage { text: full_desc }),
                help: Some(SarifMessage { text: help_text }),
                default_configuration: Some(SarifRuleConfig {
                    level: "warning".to_string(),
                }),
            }
        })
        .collect()
}

/// Create a SARIF run structure for tool results.
///
/// Generates a complete SARIF run with tool information, rules, and results.
/// This is used to format tool output in SARIF format for GitHub Security and VS Code integration.
///
/// # Arguments
/// * `tool_name` - Name of the tool (e.g., "crap", "debt")
/// * `tool_version` - Version of the tool
/// * `results` - Vector of SarifResult structs containing the findings
/// * `exit_code` - Exit code from the tool execution
///
/// # Returns
/// A SarifRun struct ready for serialization to SARIF format
pub fn sarif_run(
    tool_name: &str,
    tool_version: &str,
    results: Vec<SarifResult>,
    exit_code: i32,
) -> SarifRun {
    let mut rule_ids: Vec<String> = results.iter().map(|r| r.rule_id.clone()).collect();
    rule_ids.sort();
    rule_ids.dedup();

    let rules = build_sarif_rules(rule_ids);

    SarifRun {
        tool: SarifTool {
            driver: SarifDriver {
                name: tool_name.to_string(),
                version: tool_version.to_string(),
                rules,
            },
        },
        invocations: Some(vec![SarifInvocation {
            execution_successful: exit_code == 0,
            exit_code: Some(exit_code),
            end_time_utc: Some(
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
        }]),
        results,
    }
}

/// Convert a quality-level string to SARIF level.
/// "error" | "warning" | "note" | "none"
pub fn sarif_level(level: &str) -> &'static str {
    match level.to_lowercase().as_str() {
        "error" | "critical" | "high" => "error",
        "warning" | "medium" => "warning",
        "note" | "info" | "low" => "note",
        _ => "warning",
    }
}

// ═══════════════════════════════════════════
// BASELINE DIFF
// ═══════════════════════════════════════════

/// Compare current SARIF results against a baseline and return only new/regressed.
pub fn diff_results(current: &[SarifResult], baseline: &[SarifResult]) -> Vec<SarifResult> {
    let baseline_keys: std::collections::HashSet<String> =
        baseline.iter().map(result_key).collect();
    current
        .iter()
        .filter(|r| !baseline_keys.contains(&result_key(r)))
        .cloned()
        .collect()
}

fn result_key(result: &SarifResult) -> String {
    let location = result
        .locations
        .first()
        .map(|l| {
            let uri = l
                .physical_location
                .artifact_location
                .as_ref()
                .map(|a| a.uri.clone())
                .unwrap_or_default();
            let line = l
                .physical_location
                .region
                .as_ref()
                .and_then(|r| r.start_line)
                .unwrap_or(0);
            format!("{}:{}:{}", uri, line, result.rule_id)
        })
        .unwrap_or(result.rule_id.clone());
    location
}

// ═══════════════════════════════════════════
// TEST RUNNER TRAIT
// ═══════════════════════════════════════════

/// Result of a test execution.
#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Trait for language-agnostic test execution.
pub trait TestRunner: Send + Sync {
    /// Run tests for the project at the given path.
    fn run_tests(&self, project_path: &Path, timeout_secs: u64) -> Result<TestRunResult, String>;
}

/// Rust test runner using `cargo test`.
pub struct CargoTestRunner;

impl TestRunner for CargoTestRunner {
    fn run_tests(&self, project_path: &Path, _timeout_secs: u64) -> Result<TestRunResult, String> {
        let output = std::process::Command::new("cargo")
            .args(["test", "--quiet"])
            .current_dir(project_path)
            .output()
            .map_err(|e| format!("Failed to run cargo test: {}", e))?;
        Ok(TestRunResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

// ═══════════════════════════════════════════
// COVERAGE / LCOV PARSING (shared across tools)
// ═══════════════════════════════════════════

/// Parsed coverage record per function.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageRecord {
    pub function: String,
    pub line: usize,
    pub hits: usize,
}

/// Parse an LCOV file into coverage records per function.
/// Lines look like: `FN:<line>,<name>` followed by `FNDA:<hits>,<name>`.
/// Attempt to extract a human-readable function name from a Rust v0 mangled symbol.
/// For `_RNvCsXXX_4crate16my_function_name` this returns `"my_function_name"`.
/// Falls back to returning the original symbol unchanged for non-mangled names.
fn demangle_rust_v0(sym: &str) -> String {
    // Only attempt demangling for Rust v0 mangled symbols
    if !sym.starts_with("_R") {
        return sym.to_string();
    }
    // Identifiers in the v0 scheme are encoded as <decimal-length><name>.
    // Walk the symbol and find every run of digits followed by ASCII identifier chars.
    // The last such run is the most-specific name (the function itself).
    let bytes = sym.as_bytes();
    let mut last_ident: Option<&str> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            // Read the length prefix
            let len_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if let Ok(len_str) = std::str::from_utf8(&bytes[len_start..i]) {
                if let Ok(len) = len_str.parse::<usize>() {
                    if i + len <= bytes.len() {
                        let candidate = &sym[i..i + len];
                        // Only keep if it looks like a valid Rust identifier
                        if candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            last_ident = Some(candidate);
                        }
                        i += len;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    last_ident
        .map(|s| s.to_string())
        .unwrap_or_else(|| sym.to_string())
}

pub fn parse_lcov(content: &str) -> Vec<CoverageRecord> {
    let mut records: std::collections::HashMap<String, CoverageRecord> =
        std::collections::HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("FN:") {
            if let Some((line_str, name)) = rest.split_once(',') {
                if let Ok(line_num) = line_str.parse::<usize>() {
                    // Index by both the raw symbol and the demangled name
                    let demangled = demangle_rust_v0(name);
                    records.entry(name.to_string()).or_insert(CoverageRecord {
                        function: demangled.clone(),
                        line: line_num,
                        hits: 0,
                    });
                    if demangled != name {
                        records.entry(demangled.clone()).or_insert(CoverageRecord {
                            function: demangled,
                            line: line_num,
                            hits: 0,
                        });
                    }
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("FNDA:") {
            if let Some((hits_str, name)) = rest.split_once(',') {
                if let Ok(hits) = hits_str.parse::<usize>() {
                    let demangled = demangle_rust_v0(name);
                    // Update both the mangled key and the demangled key
                    if let Some(rec) = records.get_mut(name) {
                        rec.hits += hits;
                    } else {
                        records.insert(
                            name.to_string(),
                            CoverageRecord {
                                function: demangled.clone(),
                                line: 0,
                                hits,
                            },
                        );
                    }
                    if demangled != name {
                        if let Some(rec) = records.get_mut(&demangled) {
                            rec.hits += hits;
                        } else {
                            records.insert(
                                demangled.clone(),
                                CoverageRecord {
                                    function: demangled,
                                    line: 0,
                                    hits,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    records.into_values().collect()
}

/// Find an LCOV coverage file in the project root (common names).
pub fn find_lcov_file(project_path: &Path) -> Option<PathBuf> {
    for name in [
        "target/lcov.info",
        "lcov.info",
        "coverage.lcov",
        "target/coverage/lcov.info",
        "coverage/lcov.info",
    ] {
        let path = project_path.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Try to find an LCOV file under the given project path.
pub fn find_coverage(project_path: &Path) -> Option<Vec<CoverageRecord>> {
    let lcov = find_lcov_file(project_path)?;
    let content = std::fs::read_to_string(&lcov).ok()?;
    Some(parse_lcov(&content))
}

// ═══════════════════════════════════════════
// CRAP SCORE UTILITIES (shared across tools)
// ═══════════════════════════════════════════

/// Look up a function in coverage records and return 1.0 if it has > 0 hits, else 0.0.
pub fn function_coverage(records: &[CoverageRecord], func_name: &str) -> f64 {
    records
        .iter()
        .find(|r| r.function == func_name)
        .map_or(0.0, |r| if r.hits > 0 { 1.0 } else { 0.0 })
}

/// Calculate CRAP score from complexity and test-coverage ratio.
/// `covered_ratio` is hits / total_runs (0.0–1.0).
pub fn crap_score(complexity: u32, covered_ratio: f64) -> f64 {
    let comp = complexity as f64;
    let cov = covered_ratio.clamp(0.0, 1.0);
    comp.powf(2.0) * (1.0 - cov).powf(3.0) + comp
}

/// Bucket a CRAP score into a category.
pub fn crap_category(score: f64) -> &'static str {
    if score > 30.0 {
        "crappy"
    } else if score > 20.0 {
        "acceptable"
    } else if score > 10.0 {
        "good"
    } else {
        "excellent"
    }
}
