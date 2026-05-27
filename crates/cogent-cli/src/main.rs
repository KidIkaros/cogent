#![deny(clippy::all)]

use clap::{Parser, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use ast_parse_ts::{parse_complexity_file, parse_doc_coverage_file, Language};
use cogent_common::memory::MemoryMonitor;
use cogent_common::{crap_score, parse_lcov, CoverageRecord};
use cogent_common::{find_source_files, ToolResult};
use cogent_common::{
    SarifArtifactLocation, SarifDriver, SarifInvocation, SarifLocation, SarifLog, SarifMessage,
    SarifPhysicalLocation, SarifRegion, SarifResult, SarifRule, SarifRuleConfig, SarifRun,
    SarifTool,
};

mod audit;

// ═══════════════════════════════════════════
// PROGRESS / SPINNER
// ═══════════════════════════════════════════

/// Detect whether stderr is a real TTY (not CI, not piped).
fn is_tty() -> bool {
    if std::env::var("CI").is_ok()
        || std::env::var("NO_COLOR").is_ok()
        || std::env::var("COGENT_NO_PROGRESS").is_ok()
    {
        return false;
    }
    // Check if stderr fd 2 is a terminal via isatty syscall
    #[cfg(unix)]
    {
        unsafe { libc_isatty(2) }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> bool {
    extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    isatty(fd) != 0
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// An overall progress bar for multi-step operations (run_batch).
struct Bar {
    total: usize,
    done: usize,
    start: Instant,
    tty: bool,
    last_len: usize,
    current_tool: String,
}

impl Bar {
    fn new(total: usize) -> Self {
        let tty = is_tty();
        Self {
            total,
            done: 0,
            start: Instant::now(),
            tty,
            last_len: 0,
            current_tool: String::new(),
        }
    }

    fn set_current(&mut self, tool: &str) {
        self.current_tool = tool.to_string();
        self.render();
    }

    fn advance(&mut self, tool: &str, passed: bool, duration_ms: u64) {
        self.done += 1;
        let icon = if passed {
            "  ✓".green().bold()
        } else {
            "  ✗".red().bold()
        };
        let name_col = if passed { tool.normal() } else { tool.red() };
        let dur_str = format_ms(duration_ms);
        if self.tty {
            // Clear spinner line, print result
            eprintln!("\r{:<width$}", "", width = self.last_len);
            eprintln!("\r{} {:<18}  {}", icon, name_col, dur_str.bright_black());
        } else {
            let ci_icon = if passed { "✓" } else { "✗" };
            eprintln!("  {} {:<18}  {}", ci_icon, tool, dur_str);
        }
        self.render();
    }

    fn render(&mut self) {
        if !self.tty {
            return;
        }
        let pct = self.done.checked_mul(100).unwrap_or(0) / self.total.max(1);
        let bar_width = 28usize;
        let filled = bar_width * self.done / self.total.max(1);
        let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
        let elapsed = self.start.elapsed();
        let eta_str = if self.done > 0 {
            let per_item = elapsed / self.done as u32;
            let remaining = per_item * (self.total - self.done) as u32;
            format!("ETA {}", format_duration(remaining))
        } else {
            "ETA --:--".to_string()
        };
        let frame = SPINNER_FRAMES[self.done % SPINNER_FRAMES.len()];
        let running = if self.current_tool.is_empty() {
            String::new()
        } else {
            format!(
                "  {} Running: {}  ({})",
                frame.cyan(),
                self.current_tool.bold(),
                format_elapsed(elapsed)
            )
        };
        let bar_line = format!(
            "  [{}/{}] {}  {}%   {}",
            self.done,
            self.total,
            bar.cyan(),
            pct,
            eta_str.bright_black()
        );
        eprint!(
            "\r{:<width$}",
            bar_line,
            width = self.last_len.max(bar_line.len())
        );
        self.last_len = bar_line.len();
        if !running.is_empty() {
            eprint!("\n{}", running);
            eprint!("\x1b[1A"); // move cursor up one line
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }

    fn finish(&self) {
        if self.tty {
            // Clear the bar line
            eprintln!("\r{:<80}", "");
        }
    }
}

fn format_elapsed(d: std::time::Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        format!("{:.1}s", total_ms as f64 / 1000.0)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

fn format_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Strip ANSI escape sequences to measure true visible character width.
fn visible_len(s: &str) -> usize {
    let plain = strip_ansi(s);
    // Count unicode scalar values (approximation; fine for ASCII + a few emoji)
    plain.chars().count()
}

/// Remove ANSI CSI escape sequences (ESC [ ... m) from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // consume until a letter
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Print a box row padding `content` to `inner_width` visible chars.
fn box_row(content: &str, inner_width: usize) {
    let vlen = visible_len(content);
    let padding = inner_width.saturating_sub(vlen);
    eprintln!("  ║  {}{}║", content, " ".repeat(padding));
}

/// Compute a weighted health score 0–100 and letter grade.
/// Security failures penalise harder (×3), compliance (×2), quality (×1).
fn health_score(checks: &[CheckResult]) -> (u32, char) {
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

/// Extract up to `limit` top offenders from a CheckResult's details JSON.
/// Returns (file, line, description) tuples.
fn extract_offenders(check: &CheckResult, limit: usize) -> Vec<(String, Option<u64>, String)> {
    let mut out = Vec::new();
    // Try common keys: items, functions, findings
    let arrays = [
        "items",
        "functions",
        "findings",
        "violations",
        "secrets",
        "duplicates",
    ];
    for key in &arrays {
        if let Some(arr) = check.details.get(key).and_then(|v| v.as_array()) {
            for item in arr.iter().take(limit) {
                let file = item
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let line = item.get("line").and_then(|v| v.as_u64());
                let desc = item
                    .get("context")
                    .or_else(|| item.get("kind"))
                    .or_else(|| item.get("name"))
                    .or_else(|| item.get("type"))
                    .or_else(|| item.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !file.is_empty() || !desc.is_empty() {
                    out.push((file, line, desc));
                }
            }
            if !out.is_empty() {
                break;
            }
        }
    }
    out
}

/// Print inline offenders under a check line (used by run_check! for failures).
fn print_offenders(check: &CheckResult) {
    let offenders = extract_offenders(check, 5);
    if offenders.is_empty() {
        return;
    }
    for (file, line, desc) in &offenders {
        let loc = match line {
            Some(l) => format!("{}:{}", file, l),
            None if file.is_empty() => String::new(),
            None => file.clone(),
        };
        if loc.is_empty() && desc.is_empty() {
            continue;
        }
        let truncated_desc = if desc.len() > 60 {
            format!("{}…", &desc[..60])
        } else {
            desc.clone()
        };
        if loc.is_empty() {
            eprintln!("      {}", truncated_desc.bright_black());
        } else {
            eprintln!("      {}  {}", loc.cyan(), truncated_desc.bright_black());
        }
    }
    // Count total items to show "… N more"
    let arrays = [
        "items",
        "functions",
        "findings",
        "violations",
        "secrets",
        "duplicates",
    ];
    for key in &arrays {
        if let Some(arr) = check.details.get(key).and_then(|v| v.as_array()) {
            if arr.len() > 5 {
                eprintln!(
                    "      {}",
                    format!("… {} more", arr.len() - 5).bright_black()
                );
            }
            break;
        }
    }
}

fn print_summary_box(
    kind: &str,
    passed: bool,
    path: &str,
    passed_count: usize,
    total: usize,
    elapsed: std::time::Duration,
    checks: &[CheckResult],
) {
    let (score, grade) = health_score(checks);
    let status_plain = if passed { "PASSED ✓" } else { "FAILED ✗" };
    let status = if passed {
        status_plain.green().bold().to_string()
    } else {
        status_plain.red().bold().to_string()
    };
    let grade_col = match grade {
        'A' => grade.to_string().green().bold().to_string(),
        'B' => grade.to_string().cyan().bold().to_string(),
        'C' => grade.to_string().yellow().bold().to_string(),
        _ => grade.to_string().red().bold().to_string(),
    };
    let score_str = format!("Score: {}/100  {}", score, grade_col);
    let checks_str = format!(
        "{}/{} checks passed  ·  {} total",
        passed_count,
        total,
        format_elapsed(elapsed)
    );
    let checks_col = if passed {
        checks_str.green().to_string()
    } else {
        checks_str.red().to_string()
    };
    let inner = 50usize;
    let border = "═".repeat(inner + 2);
    let title = format!("{}  ·  {}", kind, status);
    eprintln!();
    eprintln!("  ╔{}╗", border);
    box_row(&title, inner);
    eprintln!("  ╠{}╣", border);
    box_row(&checks_col, inner);
    box_row(&score_str, inner);
    box_row(&format!("Path: {}", path), inner);
    eprintln!("  ╚{}╝", border);
    eprintln!();
}

/// Print a compact "Quick Fixes" box after check failures.
fn print_fix_summary(checks: &[CheckResult]) {
    let failed: Vec<&CheckResult> = checks.iter().filter(|c| !c.passed).collect();
    if failed.is_empty() {
        return;
    }

    let mut rows: Vec<(String, String)> = Vec::new();
    for check in &failed {
        let fix = match check.name.as_str() {
            "crap" => "add tests for complex functions, refactor high-complexity code".to_string(),
            "debt" => "convert TODOs to issues, remove resolved markers".to_string(),
            "doc_coverage" | "doccov" => "add /// doc comments to public functions".to_string(),
            "complexity" => "extract nested logic into smaller helpers".to_string(),
            "taint" => "sanitize user input before security-sensitive operations".to_string(),
            "duplication" | "dup" | "dupfind" => "extract clones into shared functions".to_string(),
            "riskmap" | "risk" => "add tests for high-churn files, refactor hotspots".to_string(),
            "coupling" => "introduce traits/traits to decouple modules".to_string(),
            "secrets" => "rotate leaked secrets, use env vars or secret manager".to_string(),
            "vulnscan" => "cargo update affected crates, review advisories".to_string(),
            "sast" => "use safe APIs, validate/sanitize all inputs".to_string(),
            "crypto" => "replace weak algorithms with SHA-256/AES-GCM".to_string(),
            "licenses" => "review flagged licenses, replace if needed".to_string(),
            "deadcode" => "remove unused functions or mark with #[allow(dead_code)]".to_string(),
            "mutate" => "add boundary-value tests, use property-based testing".to_string(),
            "linelen" => "run rustfmt, break long expressions".to_string(),
            "halstead" => "split large files into focused modules".to_string(),
            "cohesion" => "split low-cohesion types into single-responsibility structs".to_string(),
            "comments" => "add 'why' comments for non-obvious logic".to_string(),
            "errhandle" => "replace unwrap/expect with ? or match".to_string(),
            "typecov" => "add type annotations, enable strict mode".to_string(),
            "propcov" => "add property-based tests (proptest, Hypothesis)".to_string(),
            "fuzz" => "add fuzz targets for parsers, sanitize inputs".to_string(),
            "access-control" => "add auth middleware, enforce default-deny".to_string(),
            "supply-chain" => "audit new deps, pin versions, use cargo-vet".to_string(),
            "outdated" => "cargo update outdated deps, enable Dependabot".to_string(),
            _ => check.message.clone(),
        };
        rows.push((check.name.clone(), fix));
    }

    let inner = 52usize;
    let border = "═".repeat(inner + 2);
    eprintln!("  ╔{}╗", border);
    box_row("Quick Fixes", inner);
    eprintln!("  ╠{}╣", border);
    for (name, fix) in rows {
        let line = format!("{} → {}", name.cyan(), fix);
        box_row(&line, inner);
    }
    eprintln!("  ╚{}╝", border);
    eprintln!();
}

/// Categorize failed checks by severity and print grouped summary.
fn print_severity_grouped(checks: &[CheckResult]) {
    let failed: Vec<&CheckResult> = checks.iter().filter(|c| !c.passed).collect();
    if failed.is_empty() {
        return;
    }

    let security = [
        "secrets",
        "vulnscan",
        "sast",
        "crypto",
        "taint",
        "access-control",
        "supply-chain",
    ];
    let compliance = ["licenses", "sbom"];
    // everything else is quality

    let mut sec = Vec::new();
    let mut qual = Vec::new();
    let mut comp = Vec::new();

    for check in &failed {
        let name = check.name.as_str();
        if security.contains(&name) {
            sec.push(name);
        } else if compliance.contains(&name) {
            comp.push(name);
        } else {
            qual.push(name);
        }
    }

    if sec.is_empty() && qual.is_empty() && comp.is_empty() {
        return;
    }

    eprintln!("  {}", "Failed by category:".bold());
    if !sec.is_empty() {
        eprintln!(
            "    {} {} — {}",
            "🔴".red(),
            "Security".red().bold(),
            sec.join(", ").red()
        );
    }
    if !qual.is_empty() {
        eprintln!(
            "    {} {} — {}",
            "🟡".yellow(),
            "Quality".yellow().bold(),
            qual.join(", ").yellow()
        );
    }
    if !comp.is_empty() {
        eprintln!(
            "    {} {} — {}",
            "🔵".cyan(),
            "Compliance".cyan().bold(),
            comp.join(", ").cyan()
        );
    }
    eprintln!();
}

/// Run `f` on the current thread while a spinner ticks on a background thread.
/// Returns the result of `f`. The spinner shows elapsed time in real-time.
fn run_with_spinner<T, F>(label: &str, f: F) -> T
where
    F: FnOnce() -> T,
    T: Send + 'static,
{
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let tty = is_tty();
    let label_str = label.to_string();
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let start = Instant::now();

    let ticker = std::thread::spawn(move || {
        if !tty {
            eprintln!("  … {}", label_str);
            return;
        }
        let mut frame = 0usize;
        let mut last_len = 0usize;
        loop {
            if done_clone.load(Ordering::Relaxed) {
                break;
            }
            let f = SPINNER_FRAMES[frame % SPINNER_FRAMES.len()];
            frame += 1;
            let elapsed = format_elapsed(start.elapsed());
            let line = format!("  {} {}  {}", f.cyan(), label_str, elapsed.bright_black());
            eprint!("\r{:<width$}", line, width = last_len.max(line.len()));
            last_len = line.len();
            let _ = std::io::Write::flush(&mut std::io::stderr());
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });

    let result = f();
    done.store(true, Ordering::Relaxed);
    let _ = ticker.join();
    result
}

// ═══════════════════════════════════════════
// PROJECT DETECTION
// ═══════════════════════════════════════════

/// Ecosystem detected from project root filesystem signals.
#[derive(Debug, Clone, PartialEq)]
enum ProjectEcosystem {
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
struct ProjectProfile {
    ecosystem: ProjectEcosystem,
    /// Command + args to run the test suite, e.g. ["cargo", "test"]
    test_cmd: Vec<String>,
    /// Command + args to collect coverage into `lcov_path`
    coverage_cmd: Vec<String>,
    /// Where the coverage output file will be written
    lcov_path: String,
    /// Source file extensions to watch for this ecosystem
    watch_extensions: Vec<String>,
    /// Recommended quality thresholds (language-tuned)
    max_crap: f64,
    min_doc: f64,
    max_debt: usize,
    max_complexity_violations: usize,
}

impl ProjectProfile {
    fn is_coverage_available(&self) -> bool {
        !self.coverage_cmd.is_empty()
    }
}

/// Inspect filesystem signals starting at `root` and return a `ProjectProfile`.
/// Falls back to unknown defaults when nothing is detected.
fn detect_project(root: &str) -> ProjectProfile {
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
// CLI DEFINITION
// ═══════════════════════════════════════════

#[derive(Parser)]
#[command(
    name = "cogent",
    about = "Unified code quality tool for Rust. Headless-first, JSON output, CI-ready.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum PolicyAction {
    /// Validate all .cogent-policies/*.yaml files
    Validate,
    /// Run checks defined in policies only
    Check {
        /// Path to analyze
        path: String,
        /// Output format: json (default), text
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Force run even without .quality.toml
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ExceptionAction {
    /// Propose a new exception
    Add {
        /// Finding ID to suppress
        #[arg(long)]
        finding_id: String,
        /// Rule ID that triggered the finding
        #[arg(long)]
        rule_id: String,
        /// File path containing the finding
        #[arg(long)]
        file: String,
        /// Reason for the exception
        #[arg(long)]
        reason: String,
        /// Reviewer who must approve
        #[arg(long)]
        reviewer: String,
    },
    /// List exceptions
    List {
        /// Filter by status: pending, approved, revoked
        #[arg(long)]
        status: Option<String>,
    },
    /// Approve a pending exception
    Approve {
        /// Exception ID
        id: String,
    },
    /// Revoke an approved exception
    Revoke {
        /// Exception ID
        id: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Run all Cogent checks, compute a 0-100 health score, and report pass/fail
    #[command(
        after_help = "Example: cogent check . --format text → runs 20+ checks with live progress.\nUse --ci for JSON output suitable for CI pipelines.\nRequires .quality.toml (run cogent init first) or use --force for defaults."
    )]
    Check {
        /// Path to analyze
        path: String,

        /// Recursive scan
        #[arg(short, long)]
        recursive: bool,

        /// Output format: json (default), text, sarif, junit, findings, ndjson, or markdown
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Path to lcov coverage file
        #[arg(long)]
        coverage: Option<String>,

        /// Max average CRAP score (fail if exceeded)
        #[arg(long, default_value = "30")]
        max_crap: f64,

        /// Min doc coverage percentage (fail if below)
        #[arg(long, default_value = "50")]
        min_doc: f64,

        /// Max technical debt markers (fail if exceeded)
        #[arg(long, default_value = "100")]
        max_debt: usize,

        /// Max number of functions with complexity >= 10 allowed before failing (default: 0 = strict)
        #[arg(long, default_value = "0")]
        max_complexity_violations: usize,

        /// Max taint violations (default: 0)
        #[arg(long, default_value = "0")]
        max_taint: usize,

        /// Max code duplication percentage (default: 5.0)
        #[arg(long, default_value = "5.0")]
        max_duplication: f64,

        /// Max allowed file risk score (default: 10.0)
        #[arg(long, default_value = "10.0")]
        max_risk: f64,

        /// Max allowed architectural coupling issues (default: 5)
        #[arg(long, default_value = "5")]
        max_coupling: usize,

        /// Min property test coverage percentage (default: 0.0)
        #[arg(long, default_value = "0.0")]
        min_propcov: f64,

        /// Max unprotected fuzzable endpoints (default: 0)
        #[arg(long, default_value = "0")]
        max_fuzz_risk: usize,

        /// Max functions/files exceeding line length limits (default: 0)
        #[arg(long, default_value = "0")]
        max_linelen: usize,

        /// Max estimated bugs from Halstead metrics per file (default: 2.0)
        #[arg(long, default_value = "2.0")]
        max_halstead_bugs: f64,

        /// Max hardcoded secret findings (default: 0)
        #[arg(long, default_value = "0")]
        max_secrets: usize,

        /// Max dead code findings (default: 10)
        #[arg(long, default_value = "10")]
        max_deadcode: usize,

        /// Max LCOM4 cohesion violations (default: 5)
        #[arg(long, default_value = "5")]
        max_cohesion: usize,

        /// Minimum comment ratio 0.0–1.0 (default: 0.05 = 5%)
        #[arg(long, default_value = "0.05")]
        min_comment_ratio: f64,

        /// Max error handling violations (unwrap/expect/panic/discard, default: 50)
        #[arg(long, default_value = "50")]
        max_errhandle: usize,

        /// Minimum type annotation coverage % for Python/JS/TS (default: 0 = off)
        #[arg(long, default_value = "0.0")]
        min_typecov: f64,

        /// Max critical CVEs from dependency scan (default: 0)
        #[arg(long, default_value = "0")]
        max_vuln_critical: usize,

        /// Max high CVEs from dependency scan (default: 0)
        #[arg(long, default_value = "0")]
        max_vuln_high: usize,

        /// Max SAST findings — SQL injection, XSS, path traversal, cmd injection (default: 0)
        #[arg(long, default_value = "0")]
        max_sast: usize,

        /// Max crypto findings — weak hash, insecure random, ECB, disabled TLS (default: 0)
        #[arg(long, default_value = "0")]
        max_crypto: usize,

        /// Max OSS license violations (default: 0)
        #[arg(long, default_value = "0")]
        max_license_violations: usize,

        /// Max direct dependencies that are a full major version behind latest (default: 0, requires cargo-outdated)
        #[arg(long, default_value = "0")]
        max_outdated: usize,

        /// Skip specific checks (comma-separated: crap,debt,doc,dup,complexity,taint,risk,coupling,propcov,fuzz,linelen,halstead,secrets,deadcode,cohesion,comments,errhandle,typecov,vulnscan,sast,crypto,licenses)
        #[arg(long)]
        skip: Option<String>,

        /// Run only these checks (comma-separated); takes precedence over --skip
        #[arg(long)]
        only: Option<String>,

        /// CI mode: JSON output, no TTY colors or progress (equivalent to --format json + COGENT_NO_PROGRESS=1)
        #[arg(long)]
        ci: bool,

        /// Show top offenders (file:line) for every check, not just failed ones
        #[arg(long)]
        verbose: bool,

        /// Emit a markdown snippet suitable for posting as a GitHub PR comment
        #[arg(long)]
        pr_comment: bool,

        /// Run checks even when .quality.toml is missing (uses hardcoded defaults)
        #[arg(long)]
        force: bool,
    },

    /// Verify environment dependencies (doctor)
    Setup,

    /// CRAP metric — measures change-risk by combining complexity and test coverage
    #[command(
        after_help = "Example: cogent crap ./src --format json → lists functions with high CRAP scores. High CRAP = complex + untested = dangerous to change."
    )]
    Crap {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long)]
        coverage: Option<String>,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Technical debt — finds TODO, FIXME, HACK, and XXX markers in source code
    #[command(
        after_help = "Example: cogent debt ./src --format json → finds all debt markers with file:line. Each marker is a known unaddressed issue."
    )]
    Debt {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long)]
        marker: Option<String>,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Documentation coverage — checks what percentage of public functions have doc comments
    #[command(
        after_help = "Example: cogent doccov ./src --format json → shows % coverage and which public items are missing docs."
    )]
    Doccov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Code duplication — finds identical or near-identical code blocks
    #[command(
        after_help = "Example: cogent dupfind ./src --format json → groups clones by similarity. Duplicated code doubles maintenance cost."
    )]
    Dupfind {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        min_lines: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cyclomatic complexity — counts independent paths through each function
    #[command(
        after_help = "Example: cogent complexity ./src --format json → lists functions with complexity >= 10. Higher complexity = harder to test and more bug-prone."
    )]
    Complexity {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        min_complexity: u32,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Generate default config file
    Init {
        /// Output path (default: .quality.toml)
        #[arg(long, default_value = ".quality.toml")]
        output: String,

        /// Full CI bootstrap: also writes GitHub Actions workflow, installs pre-commit hook, seeds baseline, records history
        #[arg(long)]
        ci: bool,
    },

    /// Explain what a Cogent tool measures, how to read its output, and how to fix common findings
    Explain {
        /// Tool name: crap, debt, doccov, complexity, taint, dup, risk, coupling, propcov, fuzz, linelen, halstead, secrets, deadcode, cohesion, comments, errhandle, typecov, vulnscan, sast, crypto, licenses, mutate, access-control, supply-chain, outdated
        tool: String,
    },

    /// Run all Cogent tools in batch mode using .quality.toml config
    Run {
        /// Path to the crate root (directory with Cargo.toml)
        path: String,

        /// Config file (default: .quality.toml)
        #[arg(long, default_value = ".quality.toml")]
        config: String,

        /// Output format (table, json, or sarif)
        #[arg(short, long, default_value = "table")]
        format: String,

        /// Baseline SARIF/JSON file: only emit new/regressed results
        #[arg(long)]
        baseline: Option<String>,

        /// Do not exit 1 on baseline regression (useful for seeding a new baseline)
        #[arg(long)]
        no_fail_on_regression: bool,
    },

    /// Record or display Cogent history
    History {
        /// Action: record (append current run to history) or show (print trend table)
        #[arg(default_value = "show")]
        action: String,

        /// History directory (default: .quality-history)
        #[arg(long, default_value = ".cogent-history")]
        dir: String,

        /// Number of recent runs to show
        #[arg(long, default_value = "10")]
        last: usize,

        /// Path to a JSON run report to record (default: stdin)
        #[arg(long)]
        report: Option<String>,

        /// Output format: text (default) or html
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Install a Cogent pre-commit git hook
    InstallHooks {
        /// Git repo root (default: current directory)
        #[arg(default_value = ".")]
        repo: String,

        /// Install a lightweight hook that skips test execution (metrics only)
        #[arg(long)]
        fast: bool,
    },

    /// Remove the Cogent pre-commit git hook
    UninstallHooks {
        /// Git repo root (default: current directory)
        #[arg(default_value = ".")]
        repo: String,
    },

    /// Watch for file changes and re-run relevant checks
    Watch {
        /// Path to watch
        #[arg(default_value = ".")]
        path: String,

        /// Which checks to run on change (comma-separated: crap,debt,doc,complexity)
        #[arg(long, default_value = "debt,doc,crap")]
        checks: String,

        /// Debounce delay in milliseconds
        #[arg(long, default_value = "500")]
        debounce_ms: u64,

        /// Skip running tests and coverage collection (metrics-only mode)
        #[arg(long)]
        no_tests: bool,

        /// Run all available checks every cycle (equivalent to cogent check)
        #[arg(long)]
        full: bool,
    },

    /// Discover available Cogent tools and their capabilities
    Discover {
        /// Output format: json (default) or text
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Generate a human-readable audit report (HTML or Markdown) from a check run
    Report {
        /// Path to audit (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Output format: html (default) or markdown
        #[arg(short, long, default_value = "html")]
        format: String,

        /// Output file (default: cogent-report.html or cogent-report.md)
        #[arg(short, long)]
        output: Option<String>,

        /// Project name shown in the report header
        #[arg(long)]
        project: Option<String>,

        /// Optional: path to existing JSON check output (skips re-running checks)
        #[arg(long)]
        from_json: Option<String>,

        /// Skip vulnscan check (faster; use when cargo audit is slow)
        #[arg(long)]
        skip: Option<String>,

        /// Open the report in the default browser after writing
        #[arg(long)]
        open: bool,
    },

    /// Compare two check JSON snapshots and show regressions or improvements
    Diff {
        /// Path to the older check JSON snapshot
        before: String,

        /// Path to the newer check JSON snapshot
        after: String,

        /// Output format: text (default) or html
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Taint analysis only
    Taint {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_taint: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Coupling analysis only
    Coupling {
        path: String,
        #[arg(long, default_value = "5")]
        max_coupling: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Risk map (churn x complexity) only
    Riskmap {
        path: String,
        #[arg(long, default_value = "10.0")]
        max_risk: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Mutation testing only
    Mutate {
        path: String,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Fuzz surface analysis only
    Fuzz {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_fuzz_risk: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Property test coverage only
    Propcov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.0")]
        min_propcov: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Line length check only
    Linelen {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Halstead complexity metrics only
    Halstead {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "2.0")]
        max_bugs: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Secret detection only
    Secrets {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Dead code detection only
    Deadcode {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "10")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cohesion (LCOM4) analysis only
    Cohesion {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Comment ratio analysis only
    Comments {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.05")]
        min_ratio: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Error handling checker only
    Errhandle {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "50")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Type annotation coverage only
    Typecov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.0")]
        min_pct: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Vulnerability scan only
    Vulnscan {
        path: String,
        #[arg(long, default_value = "0")]
        max_critical: usize,
        #[arg(long, default_value = "0")]
        max_high: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// SAST scanner only
    Sast {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cryptography checker only
    Crypto {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// License compliance only
    Licenses {
        path: String,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Outdated dependency checker only
    Outdated {
        path: String,
        #[arg(long, default_value = "0")]
        max_major_behind: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Access control checker only
    AccessControl {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Supply chain checker only
    SupplyChain {
        path: String,
        #[arg(long, default_value = "0")]
        max_risks: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Start a local HTTP server to browse reports
    Serve {
        /// Port to listen on (default: 8080)
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Directory containing historical reports (default: .cogent-history)
        #[arg(long, default_value = ".cogent-history")]
        history_dir: String,
    },

    /// Generate shell completion scripts for bash, zsh, fish, or powershell
    #[command(after_help = "Example: cogent completions bash > /etc/bash_completion.d/cogent")]
    Completions {
        /// Shell to generate completions for: bash, zsh, fish, or powershell
        shell: String,
    },

    /// Validate or run custom audit policies defined in .cogent-policies/
    #[command(
        after_help = "Example: cogent policy validate → checks all policy files for errors.
cogent policy check . → runs only the tools defined in policies."
    )]
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    /// Manage approved exceptions (false positive overrides)
    #[command(
        after_help = "Example: cogent exception add --finding-id SAST-001 --reason 'sanitized upstream' --reviewer alice"
    )]
    Exception {
        #[command(subcommand)]
        action: ExceptionAction,
    },

    /// Track remediation status of findings
    #[command(
        after_help = "Example: cogent remediate --verify → checks which previous findings are now fixed."
    )]
    Remediate {
        /// Verify closed findings against current scan
        #[arg(long)]
        verify: bool,
        /// Path to analyze
        path: String,
    },

    /// Query and verify the signed audit trail
    #[command(after_help = "Example: cogent audit-trail --verify → checks trail integrity.")]
    AuditTrail {
        /// Verify signatures (detect tampering)
        #[arg(long)]
        verify: bool,
        /// Filter by command
        #[arg(long)]
        command: Option<String>,
        /// Show entries since this ISO date
        #[arg(long)]
        since: Option<String>,
    },

    /// Full audit: runs all checks, enriches findings with fixes + compliance controls, writes audit trail
    #[command(
        after_help = "Example: cogent audit . --format agent | jq\ncogent audit . --format agent --evidence --framework soc2"
    )]
    Audit {
        /// Path to analyze
        path: String,

        /// Output format: agent (NDJSON, one finding per line), json (full report), markdown
        #[arg(short, long, default_value = "agent")]
        format: String,

        /// Only run a category: security, quality, compliance (default: all)
        #[arg(long)]
        only: Option<String>,

        /// Attach code snippets, file hashes, and git blame to each finding
        #[arg(long)]
        evidence: bool,

        /// Map findings to compliance framework controls: soc2, iso27001, both
        #[arg(long)]
        framework: Option<String>,

        /// Run only these checks (comma-separated)
        #[arg(long)]
        checks: Option<String>,

        /// Skip these checks (comma-separated)
        #[arg(long)]
        skip: Option<String>,

        /// CI mode: suppress TTY output, exit 1 on any finding
        #[arg(long)]
        ci: bool,

        /// After scanning, auto-close remediation entries whose findings are no longer present
        #[arg(long)]
        verify: bool,
    },
}

// ═══════════════════════════════════════════
// RESULT TYPES
// ═══════════════════════════════════════════

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Finding {
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u64>,
    severity: String,
    message: String,
    rule_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    fix_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<SuggestedFix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    controls: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Evidence {
    snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SuggestedFix {
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
    confidence: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct FileSummary {
    file: String,
    issue_count: usize,
    severity_score: usize,
    findings_by_severity: std::collections::HashMap<String, usize>,
}

#[derive(Serialize, Deserialize, Clone)]
struct CheckResult {
    name: String,
    passed: bool,
    score: Option<f64>,
    threshold: Option<f64>,
    message: String,
    details: serde_json::Value,
    severity: Option<String>,
    help: Option<String>,
    rule_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    findings: Vec<Finding>,
}

#[derive(Serialize, Deserialize)]
struct CheckReport {
    passed: bool,
    path: String,
    checks: Vec<CheckResult>,
    summary: CheckSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    file_summary: Vec<FileSummary>,
}

#[derive(Serialize, Deserialize)]
struct CheckSummary {
    total_checks: usize,
    passed_checks: usize,
    failed_checks: usize,
    functions_analyzed: usize,
    avg_complexity: f64,
    avg_crap: f64,
}

// ═══════════════════════════════════════════
// FINDING HELPERS
// ═══════════════════════════════════════════

/// Generic extraction of Finding structs from typical tool JSON output.
/// Tries common keys: "findings", "violations", "items", "functions", "files", "results".
fn extract_findings_from_details(
    details: &serde_json::Value,
    default_rule_id: &str,
    default_severity: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let arrays = [
        "findings",
        "violations",
        "items",
        "functions",
        "files",
        "results",
        "matches",
    ];
    for key in &arrays {
        if let Some(arr) = details.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                let file = item
                    .get("file")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("path").and_then(|v| v.as_str()))
                    .or_else(|| item.get("filename").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let line = item.get("line").and_then(|v| v.as_u64());
                let column = item.get("column").and_then(|v| v.as_u64());
                let severity = item
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_severity)
                    .to_string();
                let message = item
                    .get("message")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("description").and_then(|v| v.as_str()))
                    .or_else(|| item.get("name").and_then(|v| v.as_str()))
                    .or_else(|| item.get("type").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let rule_id = item
                    .get("rule_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("rule").and_then(|v| v.as_str()))
                    .unwrap_or(default_rule_id)
                    .to_string();
                let fix_hint = item
                    .get("fix_hint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // Skip empty entries that are just summary metadata
                if file.is_empty() && message.is_empty() {
                    continue;
                }
                findings.push(Finding {
                    file,
                    line,
                    column,
                    severity,
                    message,
                    rule_id,
                    fix_hint,
                    evidence: None,
                    suggested_fix: None,
                    controls: None,
                });
            }
        }
    }
    findings
}

/// Compute per-file aggregation from all check results.
fn aggregate_file_summary(checks: &[CheckResult]) -> Vec<FileSummary> {
    let mut map: std::collections::HashMap<
        String,
        (usize, usize, std::collections::HashMap<String, usize>),
    > = std::collections::HashMap::new();
    for check in checks {
        for finding in &check.findings {
            let sev_score = match finding.severity.as_str() {
                "critical" => 4,
                "high" | "error" => 3,
                "medium" | "warning" => 2,
                "low" => 1,
                _ => 0,
            };
            let entry = map
                .entry(finding.file.clone())
                .or_insert_with(|| (0, 0, std::collections::HashMap::new()));
            entry.0 += 1;
            entry.1 += sev_score;
            *entry.2.entry(finding.severity.clone()).or_insert(0) += 1;
        }
    }
    let mut summaries: Vec<FileSummary> = map
        .into_iter()
        .map(
            |(file, (issue_count, severity_score, findings_by_severity))| FileSummary {
                file,
                issue_count,
                severity_score,
                findings_by_severity,
            },
        )
        .collect();
    summaries.sort_by(|a, b| {
        b.severity_score
            .cmp(&a.severity_score)
            .then_with(|| b.issue_count.cmp(&a.issue_count))
    });
    summaries.truncate(20);
    summaries
}

#[derive(Serialize)]
struct ToolInfo {
    name: String,
    binary: String,
    description: String,
    supported_formats: Vec<String>,
    output_fields: Vec<String>,
    rule_ids: Vec<String>,
}

// ═══════════════════════════════════════════
// CHECKS
// ═══════════════════════════════════════════

/// Scan all source files under `path`, invoking `predicate` on each function.
/// Returns `(total_functions_count, collected_items)`.
fn scan_source_functions<T, F>(path: &str, recursive: bool, mut predicate: F) -> (usize, Vec<T>)
where
    F: FnMut(&ast_parse_ts::FunctionInfo) -> Option<T>,
{
    let files = find_source_files(
        path,
        recursive,
        &[
            "rs", "py", "js", "ts", "go", "java", "c", "cpp", "cs", "php", "rb", "swift",
        ],
    );
    let mut total = 0;
    let mut results = Vec::new();
    for file in files {
        let functions = parse_complexity_file(&file);
        total += functions.len();
        for func in &functions {
            if let Some(item) = predicate(func) {
                results.push(item);
            }
        }
    }
    (total, results)
}

fn function_coverage(coverage_records: &[CoverageRecord], func_name: &str) -> f64 {
    coverage_records
        .iter()
        .find(|r| r.function == func_name)
        .map_or(0.0, |r| if r.hits > 0 { 1.0 } else { 0.0 })
}

fn check_crap(
    path: &str,
    recursive: bool,
    coverage_path: &Option<String>,
    max_crap: f64,
) -> CheckResult {
    let coverage_data: Option<Vec<CoverageRecord>> = coverage_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|c| parse_lcov(&c));

    let (total, functions) = scan_source_functions(path, recursive, |func| {
        let cov_pct = if let Some(ref cov_data) = coverage_data {
            function_coverage(cov_data, &func.name)
        } else {
            0.0
        };
        let score = crap_score(func.complexity, cov_pct);
        Some((func.name.clone(), func.complexity, cov_pct, score))
    });
    let avg_crap = if total > 0 {
        functions.iter().map(|f| f.3).sum::<f64>() / total as f64
    } else {
        0.0
    };
    let crappy: Vec<_> = functions.iter().filter(|f| f.3 > 30.0).collect();

    let (severity, rule_id, help) = if avg_crap <= max_crap {
        (
            "info".to_string(),
            "crap-pass".to_string(),
            "CRAP score is within acceptable limits.".to_string(),
        )
    } else if avg_crap > max_crap * 1.5 {
        (
            "error".to_string(),
            "crap-error".to_string(),
            "Reduce function complexity or increase test coverage to lower CRAP score. Aim for CRAP < 30 per function.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "crap-warning".to_string(),
            "CRAP score is approaching threshold. Consider refactoring complex functions or adding tests.".to_string(),
        )
    };

    let details = serde_json::json!({
        "total_functions": total,
        "avg_crap": avg_crap,
        "crappy_count": crappy.len(),
        "excellent_count": functions.iter().filter(|f| f.3 <= 10.0).count(),
        "top_offenders": crappy.iter().take(5).map(|f| {
            serde_json::json!({
                "name": f.0, "complexity": f.1, "coverage": f.2, "crap": f.3
            })
        }).collect::<Vec<_>>(),
    });
    let findings: Vec<Finding> = crappy
        .iter()
        .take(20)
        .map(|f| Finding {
            file: f.0.clone(),
            line: None,
            column: None,
            evidence: None,
            severity: severity.clone(),
            message: format!(
                "{}: complexity={}, coverage={:.0}%, CRAP={:.1}",
                f.0,
                f.1,
                f.2 * 100.0,
                f.3
            ),
            rule_id: rule_id.clone(),
            fix_hint: "Reduce function complexity or increase test coverage to lower CRAP score."
                .to_string(),
            suggested_fix: None,
            controls: None,
        })
        .collect();

    CheckResult {
        name: "crap".to_string(),
        passed: avg_crap <= max_crap,
        score: Some(avg_crap),
        threshold: Some(max_crap),
        message: if avg_crap <= max_crap {
            format!("Average CRAP {:.1} <= {:.0}", avg_crap, max_crap)
        } else {
            format!(
                "Average CRAP {:.1} > {:.0} ({} functions above 30)",
                avg_crap,
                max_crap,
                crappy.len()
            )
        },
        details,
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

fn check_debt(path: &str, recursive: bool, max_debt: usize) -> CheckResult {
    let extensions = [
        "rs", "py", "js", "ts", "go", "c", "cpp", "h", "java", "cs", "php", "rb", "swift",
    ];
    let files = find_source_files(path, recursive, &extensions);

    let markers = ["TODO", "FIXME", "HACK", "XXX", "BUG"];
    let mut count = 0;
    let mut items = Vec::new();

    for file in &files {
        if let Ok(source) = std::fs::read_to_string(file) {
            for (line_num, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*')
                {
                    for marker in &markers {
                        if trimmed.contains(marker) {
                            count += 1;
                            items.push(serde_json::json!({
                                "file": file, "line": line_num + 1, "type": marker
                            }));
                        }
                    }
                }
            }
        }
    }

    let (severity, rule_id, help) = if count <= max_debt {
        (
            "info".to_string(),
            "debt-pass".to_string(),
            "Technical debt is within acceptable limits.".to_string(),
        )
    } else if count > max_debt * 2 {
        (
            "error".to_string(),
            "debt-high".to_string(),
            "Excessive technical debt. Address TODO/FIXME/HACK markers to improve code maintainability.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "debt-moderate".to_string(),
            "Moderate technical debt. Consider addressing high-priority markers first.".to_string(),
        )
    };

    let findings: Vec<Finding> = items
        .iter()
        .take(50)
        .map(|item| {
            let file = item
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = item.get("line").and_then(|v| v.as_u64());
            let marker = item.get("type").and_then(|v| v.as_str()).unwrap_or("TODO");
            Finding {
                file: file.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("{} marker found in {}", marker, file),
                rule_id: format!("debt-{}", marker.to_lowercase()),
                fix_hint: format!("Address or remove the {} marker.", marker),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    CheckResult {
        name: "debt".to_string(),
        passed: count <= max_debt,
        score: Some(count as f64),
        threshold: Some(max_debt as f64),
        message: if count <= max_debt {
            format!("{} debt markers <= {}", count, max_debt)
        } else {
            format!("{} debt markers > {}", count, max_debt)
        },
        details: serde_json::json!({
            "total_markers": count,
            "items": items.iter().take(20).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

use syn::visit::Visit;
use syn::{ImplItemFn, ItemEnum, ItemFn, ItemStruct, ItemTrait, Visibility};

struct DocCounter {
    total: usize,
    documented: usize,
}
impl<'a> Visit<'a> for DocCounter {
    fn visit_item_fn(&mut self, node: &'a ItemFn) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_struct(&mut self, node: &'a ItemStruct) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_enum(&mut self, node: &'a ItemEnum) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_item_trait(&mut self, node: &'a ItemTrait) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
    fn visit_impl_item_fn(&mut self, node: &'a ImplItemFn) {
        if matches!(node.vis, Visibility::Public(_)) {
            self.total += 1;
            if node.attrs.iter().any(|a| a.path().is_ident("doc")) {
                self.documented += 1;
            }
        }
    }
}

fn check_doc_coverage(path: &str, recursive: bool, min_doc: f64) -> CheckResult {
    let mut total = 0usize;
    let mut documented = 0usize;
    let mut langs_seen: std::collections::HashSet<String> = Default::default();

    // Rust files via syn (high-fidelity)
    let rust_files = find_source_files(path, recursive, &["rs"]);
    if !rust_files.is_empty() {
        langs_seen.insert("rust".to_string());
    }
    let mut counter = DocCounter {
        total: 0,
        documented: 0,
    };
    for file in &rust_files {
        if let Ok(source) = std::fs::read_to_string(file) {
            if let Ok(ast) = syn::parse_file(&source) {
                counter.visit_file(&ast);
            }
        }
    }
    total += counter.total;
    documented += counter.documented;

    // Non-Rust files via tree-sitter
    let all_exts = ["py", "pyi", "js", "mjs", "ts", "tsx", "go"];
    let other_files: Vec<String> = find_source_files(path, recursive, &all_exts)
        .into_iter()
        .filter(|f| !f.ends_with(".rs"))
        .collect();
    for file in &other_files {
        let lang = Language::from_extension(file);
        let stats = parse_doc_coverage_file(file);
        if stats.total_public > 0 {
            langs_seen.insert(lang.to_string());
        }
        total += stats.total_public;
        documented += stats.documented;
    }

    let pct = if total > 0 {
        documented as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    let mut langs_vec: Vec<String> = langs_seen.into_iter().collect();
    langs_vec.sort();

    let (severity, rule_id, help) = if pct >= min_doc {
        (
            "info".to_string(),
            "doccov-pass".to_string(),
            "Documentation coverage is within acceptable limits.".to_string(),
        )
    } else if pct < min_doc * 0.5 {
        (
            "error".to_string(),
            "doccov-low".to_string(),
            "Very low documentation coverage. Add documentation to public APIs to improve maintainability.".to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "doccov-moderate".to_string(),
            "Moderate documentation coverage. Add documentation to remaining public APIs."
                .to_string(),
        )
    };

    CheckResult {
        name: "doc_coverage".to_string(),
        passed: pct >= min_doc,
        findings: Vec::new(),
        score: Some(pct),
        threshold: Some(min_doc),
        message: if pct >= min_doc {
            format!(
                "Doc coverage {:.0}% >= {:.0}% (langs: {})",
                pct,
                min_doc,
                langs_vec.join(", ")
            )
        } else {
            format!(
                "Doc coverage {:.0}% < {:.0}% (langs: {})",
                pct,
                min_doc,
                langs_vec.join(", ")
            )
        },
        details: serde_json::json!({
            "total_public": total,
            "documented": documented,
            "coverage_pct": pct,
            "languages": langs_vec,
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
    }
}

fn check_complexity(
    path: &str,
    recursive: bool,
    min_complexity: u32,
    max_violations: usize,
) -> CheckResult {
    let all_exts = [
        "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp", "cc",
        "cxx", "hpp", "cs", "java", "php", "rb", "swift",
    ];
    let files = find_source_files(path, recursive, &all_exts);

    let mut total = 0usize;
    let mut complex_funcs: Vec<serde_json::Value> = Vec::new();
    let mut langs_seen: std::collections::HashSet<String> = Default::default();

    for file in &files {
        let lang = Language::from_extension(file);
        langs_seen.insert(lang.to_string());
        let funcs = parse_complexity_file(file);
        for func in funcs {
            total += 1;
            if func.complexity >= min_complexity {
                complex_funcs.push(serde_json::json!({
                    "name": func.name,
                    "file": func.file,
                    "line": func.line,
                    "complexity": func.complexity,
                    "language": func.language.to_string(),
                }));
            }
        }
    }

    let mut langs_vec: Vec<String> = langs_seen.into_iter().collect();
    langs_vec.sort();

    let passed = complex_funcs.len() <= max_violations;

    let (severity, rule_id, help) = if passed && complex_funcs.is_empty() {
        (
            "info".to_string(),
            "complexity-pass".to_string(),
            "No functions with excessive complexity.".to_string(),
        )
    } else if passed {
        (
            "info".to_string(),
            "complexity-pass".to_string(),
            format!(
                "Complexity violations within allowed limit (<= {}).",
                max_violations
            ),
        )
    } else if complex_funcs.len() > 10 {
        (
            "error".to_string(),
            "complexity-high".to_string(),
            "Multiple functions with high complexity. Refactor to reduce decision points."
                .to_string(),
        )
    } else {
        (
            "warning".to_string(),
            "complexity-moderate".to_string(),
            "Some functions with high complexity. Consider refactoring.".to_string(),
        )
    };

    let findings: Vec<Finding> = complex_funcs
        .iter()
        .take(20)
        .map(|f| {
            let file = f
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = f.get("line").and_then(|v| v.as_u64());
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let complexity = f.get("complexity").and_then(|v| v.as_u64()).unwrap_or(0);
            Finding {
                file: file.clone(),
                line,
                column: None,
                severity: severity.clone(),
                message: format!("{}: cyclomatic complexity = {}", name, complexity),
                rule_id: rule_id.clone(),
                fix_hint: "Refactor to reduce decision points (if/loops/matches).".to_string(),
                evidence: None,
                suggested_fix: None,
                controls: None,
            }
        })
        .collect();

    CheckResult {
        name: "complexity".to_string(),
        passed,
        score: Some(complex_funcs.len() as f64),
        threshold: Some(max_violations as f64),
        message: if passed && complex_funcs.is_empty() {
            format!(
                "No functions above complexity threshold (languages: {})",
                langs_vec.join(", ")
            )
        } else if passed {
            format!(
                "{} complex functions <= allowed {} (languages: {})",
                complex_funcs.len(),
                max_violations,
                langs_vec.join(", ")
            )
        } else {
            format!(
                "{} functions with complexity >= {} > allowed {} (languages: {})",
                complex_funcs.len(),
                min_complexity,
                max_violations,
                langs_vec.join(", ")
            )
        },
        details: serde_json::json!({
            "total_functions": total,
            "complex_count": complex_funcs.len(),
            "max_violations_allowed": max_violations,
            "languages": langs_vec,
            "functions": complex_funcs.iter().take(10).collect::<Vec<_>>(),
        }),
        severity: Some(severity),
        help: Some(help),
        rule_id: Some(rule_id),
        findings,
    }
}

// ═══════════════════════════════════════════
// OUTPUT FORMATTERS
// ═══════════════════════════════════════════

fn output_json(report: &CheckReport) {
    println!("{}", serde_json::to_string_pretty(report).unwrap());
}

// ═══════════════════════════════════════════
// CONFIG
// ═══════════════════════════════════════════

fn generate_config(output: &str, profile: &ProjectProfile) {
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

/// Load thresholds from `.quality.toml` if present, falling back to `defaults`.
/// Parsing is intentionally line-by-line to avoid pulling in a TOML dependency.
type Thresholds = (
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

fn load_config_thresholds(config_path: &str, defaults: Thresholds) -> Thresholds {
    // defaults: (max_crap, min_doc, max_debt, max_complexity, max_duplication,
    //            max_taint, max_risk, max_coupling, min_propcov, max_fuzz_risk,
    //            max_linelen, max_halstead_bugs, max_secrets, max_deadcode,
    //            max_cohesion, min_comment_ratio, max_errhandle,
    //            min_typecov, max_vuln_critical, max_vuln_high,
    //            max_sast, max_crypto, max_license_violations)
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
    let rest = if let Some(r) = line.strip_prefix(&prefix) {
        r
    } else {
        line.strip_prefix(&prefix2)?
    };
    rest.split_whitespace().next()?.parse().ok()
}

fn parse_toml_usize(line: &str, key: &str) -> Option<usize> {
    parse_toml_f64(line, key).map(|v| v as usize)
}

/// `cogent init --ci`: detect project, write config, install hook, write GHA workflow,
/// seed baseline, record first history entry.
fn init_ci(config_path: &str, profile: &ProjectProfile) -> i32 {
    let mut ok = true;

    // 1. Write .quality.toml
    {
        let t = Instant::now();
        generate_config(config_path, profile);
        eprintln!(
            "  {} Wrote {}  ({})",
            "✓".green().bold(),
            config_path.cyan(),
            format_elapsed(t.elapsed()).bright_black()
        );
    }

    // 2. Install pre-commit hook
    {
        let t = Instant::now();
        let hook_result = install_hooks_impl(".", false, profile);
        if hook_result == 0 {
            eprintln!(
                "  {} Installed pre-commit hook  ({})",
                "✓".green().bold(),
                format_elapsed(t.elapsed()).bright_black()
            );
        } else {
            eprintln!(
                "  {} Could not install pre-commit hook (not a git repo?)",
                "!".yellow().bold()
            );
        }
    }

    // 3. Write GitHub Actions workflow
    {
        let t = Instant::now();
        let gha_dir = ".github/workflows";
        if let Err(e) = std::fs::create_dir_all(gha_dir) {
            eprintln!(
                "  {} Could not create {}: {}",
                "!".yellow().bold(),
                gha_dir,
                e
            );
            ok = false;
        } else {
            let workflow_path = format!("{}/cogent.yml", gha_dir);
            let workflow = build_gha_workflow(profile);
            match std::fs::write(&workflow_path, workflow) {
                Ok(_) => eprintln!(
                    "  {} Wrote {}  ({})",
                    "✓".green().bold(),
                    workflow_path.cyan(),
                    format_elapsed(t.elapsed()).bright_black()
                ),
                Err(e) => {
                    eprintln!(
                        "  {} Could not write {}: {}",
                        "!".yellow().bold(),
                        workflow_path,
                        e
                    );
                    ok = false;
                }
            }
        }
    }

    // 4. Seed baseline: run `cogent run . --format sarif --no-fail-on-regression`
    let seed = run_with_spinner("seeding quality baseline (this runs all tools)", || {
        std::process::Command::new(std::env::current_exe().unwrap_or("cogent".into()))
            .args(["run", ".", "--format", "sarif", "--no-fail-on-regression"])
            .output()
    });
    match seed {
        Ok(out) => {
            let sarif = String::from_utf8_lossy(&out.stdout);
            match std::fs::write(".cogent-baseline.sarif", sarif.as_bytes()) {
                Ok(_) => eprintln!("  {} Wrote .cogent-baseline.sarif", "✓".green().bold()),
                Err(e) => {
                    eprintln!("  {} Could not write baseline: {}", "!".yellow().bold(), e);
                    ok = false;
                }
            }
        }
        Err(e) => {
            eprintln!(
                "  {} Baseline seeding skipped (cogent not on PATH yet): {}",
                "!".yellow().bold(),
                e
            );
        }
    }

    // 5. Record initial history entry
    let history_dir = ".cogent-history";
    if std::fs::create_dir_all(history_dir).is_ok() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry = serde_json::json!({
            "ts": ts,
            "event": "init",
            "ecosystem": profile.ecosystem.to_string(),
            "passed": 0u64,
            "failed": 0u64,
        });
        let path = format!("{}/{}.jsonl", history_dir, ts);
        let _ = std::fs::write(&path, format!("{}\n", entry));
        println!("  {} Recorded initial history entry", "[✓]".green().bold());
    }

    eprintln!();
    if ok {
        eprintln!("  {} Setup complete — CI is ready.", "✓".green().bold());
    } else {
        eprintln!(
            "  {} Setup completed with warnings. Check messages above.",
            "!".yellow().bold()
        );
    }
    eprintln!();
    eprintln!("  {} Next steps:", "▶".cyan().bold());
    eprintln!(
        "    1. {} cogent check .          {}",
        "$".bright_black(),
        "— verify everything passes locally".bright_black()
    );
    eprintln!(
        "    2. {} cogent report .         {}",
        "$".bright_black(),
        "— open the HTML audit report in your browser".bright_black()
    );
    eprintln!(
        "    3. {} git push                     {}",
        "$".bright_black(),
        "— CI runs automatically on next push".bright_black()
    );
    eprintln!(
        "    4. {} cogent watch . --full   {}",
        "$".bright_black(),
        "— live feedback during development".bright_black()
    );
    eprintln!();
    0
}

/// Print per-tool documentation for `cogent explain <tool>`.
fn explain_command(tool: &str) {
    let tool = tool.to_lowercase();
    let (title, description, threshold, how_to_read, fixes, see_also) = match tool.as_str() {
        "crap" => (
            "CRAP (Change Risk Anti-Pattern)",
            "Measures how risky a function is to change. Combines cyclomatic complexity and test coverage. A high-CRAP function is complex AND untested — the most dangerous kind.",
            "max_avg = 15.0  (average CRAP score across all functions)",
            "Score 0-30 is good. Above 30 means many complex functions lack tests. Each function gets a CRAP score; the average is compared to the threshold.",
            vec![
                "Add unit tests for complex functions (coverage drives CRAP down faster than simplification).",
                "Refactor functions with cyclomatic complexity > 10 into smaller helpers.",
                "Focus on the top 5 offenders shown in the output.",
            ],
            "docs/tools/crap.md",
        ),
        "debt" => (
            "Technical Debt Markers",
            "Scans source code for TODO, FIXME, HACK, and XXX markers. Each marker is a known unaddressed issue that accumulates over time.",
            "max_markers = 0  (zero-tolerance by default for Rust)",
            "Output lists each marker with file:line and text. The count is compared to max_markers.",
            vec![
                "Convert TODOs into tracked GitHub issues and remove the marker.",
                "Schedule a 'debt sprint' to resolve FIXMEs before they rot.",
                "Use cogent watch . to catch new markers during development.",
            ],
            "docs/tools/debt.md",
        ),
        "doccov" | "doc" | "doc_coverage" => (
            "Documentation Coverage",
            "Measures the percentage of public functions that have doc comments (/// in Rust, /** in JS, etc.).",
            "min_pct = 95%  (Rust), 80% (Go), 70% (JS)",
            "Output shows the percentage and which public functions are missing docs. Missing docs make APIs harder to consume and maintain.",
            vec![
                "Add /// doc comments to every public function and struct.",
                "Use rustdoc --test to ensure doc examples compile.",
                "Enable #![warn(missing_docs)] in lib.rs to catch omissions at compile time.",
            ],
            "docs/tools/doccov.md",
        ),
        "complexity" | "cyclomatic" => (
            "Cyclomatic Complexity",
            "Counts the number of independent paths through a function (branches, loops, match arms). Higher complexity means harder testing and higher bug risk.",
            "max_violations = 0  (no functions should exceed complexity 10)",
            "Output lists every function with complexity >= 10. The count of such functions is compared to max_violations.",
            vec![
                "Extract nested conditionals into named helper functions.",
                "Replace deep match/if chains with strategy enums or lookup tables.",
                "Add unit tests for each extracted branch to preserve coverage.",
            ],
            "docs/tools/complexity.md",
        ),
        "taint" => (
            "Taint Analysis",
            "Tracks untrusted input flows through your code to find paths that reach security-sensitive operations (SQL, shell, file writes) without sanitization.",
            "max_taint = 0  (zero-tolerance)",
            "Each finding shows the full flow from source (user input) to sink (dangerous call). Red paths need sanitization or validation.",
            vec![
                "Add input validation at the API boundary (length, charset, regex).",
                "Use parameterized queries / prepared statements for all SQL.",
                "Never pass user input directly to std::process::Command or eval().",
            ],
            "",
        ),
        "dup" | "dupfind" | "duplication" => (
            "Code Duplication",
            "Finds blocks of identical or near-identical code. Duplicated code doubles maintenance cost and bug probability.",
            "max_duplicates = 0  (zero-tolerance for blocks >= 3 lines)",
            "Output groups clones by similarity. Each group lists the files and line ranges where the clone appears.",
            vec![
                "Extract the duplicated block into a shared helper function.",
                "If the clones differ slightly, use a function parameter to capture the variation.",
                "For boilerplate, consider macros or code generation.",
            ],
            "",
        ),
        "risk" | "riskmap" => (
            "Risk Map",
            "Identifies files that are both complex and frequently changed (high churn). These are bug hotspots.",
            "max_risk = 10.0  (risk score per file)",
            "Score combines git churn (commit count) and cyclomatic complexity. Red files need refactoring or extra tests.",
            vec![
                "Add integration tests for the riskiest files.",
                "Refactor high-churn files to reduce coupling and complexity.",
                "Consider freezing volatile files behind stable interfaces.",
            ],
            "",
        ),
        "coupling" => (
            "Architectural Coupling",
            "Measures how tightly modules depend on each other. High coupling makes refactoring and testing difficult.",
            "max_coupling = 5  (allowed cross-module dependency issues)",
            "Output shows module pairs with excessive imports. Each arrow is a dependency that may violate your architecture.",
            vec![
                "Introduce dependency inversion: modules depend on traits, not concrete types.",
                "Move shared code to a common crate or module.",
                "Use visibility modifiers (pub(crate)) to hide internal details.",
            ],
            "",
        ),
        "secrets" => (
            "Secret Detection",
            "Scans for hardcoded API keys, tokens, passwords, and private keys committed to source control.",
            "max_secrets = 0  (zero-tolerance)",
            "Each finding shows the file, line, and secret type. Even test keys can leak real credentials through copy-paste.",
            vec![
                "Remove the secret from code immediately and rotate it in the service.",
                "Use environment variables or a secret manager (AWS Secrets Manager, 1Password, etc.).",
                "Add the pattern to .gitignore or a pre-commit hook (try cogent install-hooks).",
            ],
            "",
        ),
        "vulnscan" => (
            "Vulnerability Scan",
            "Checks dependencies against known CVE databases using cargo audit (Rust) or equivalent.",
            "max_vuln_critical = 0, max_vuln_high = 0  (zero-tolerance)",
            "Output lists each CVE with severity, affected crate/version, and advisory URL. Critical/High must be fixed immediately.",
            vec![
                "Run cargo update <crate> to pull patched versions.",
                "If no patch exists, consider replacing the dependency or adding a compensating control.",
                "Enable Dependabot or Renovate for automatic update PRs.",
            ],
            "",
        ),
        "sast" => (
            "Static Application Security Testing (SAST)",
            "Finds common security bugs: SQL injection, XSS, path traversal, command injection, insecure deserialization.",
            "max_sast = 0  (zero-tolerance)",
            "Each finding includes the vulnerability type, file:line, and a description of the unsafe pattern.",
            vec![
                "Use safe APIs: parameterized queries, HTML sanitizers, Path::join instead of string concat.",
                "Never eval() or deserialize untrusted data without schema validation.",
                "Run cogent sast --format json and feed findings into your issue tracker.",
            ],
            "",
        ),
        "crypto" => (
            "Cryptography Checker",
            "Detects weak cryptographic practices: MD5/SHA1, ECB mode, insecure random, disabled certificate validation.",
            "max_crypto = 0  (zero-tolerance)",
            "Findings list the insecure algorithm or pattern and where it is used.",
            vec![
                "Replace MD5/SHA1 with SHA-256 or Blake3.",
                "Use AEAD ciphers (ChaCha20-Poly1305, AES-GCM) instead of ECB.",
                "Use OsRng or getrandom for cryptographic randomness, not Math.random.",
            ],
            "",
        ),
        "licenses" => (
            "License Compliance",
            "Checks that all dependencies use permissive or approved open-source licenses.",
            "max_license_violations = 0  (zero-tolerance)",
            "Output lists each dependency with its license. Flagged licenses may conflict with your project's distribution terms.",
            vec![
                "Review flagged dependencies and seek legal approval if needed.",
                "Replace copyleft dependencies with permissive alternatives.",
                "Add an allow-list to .quality.toml under [licenses].",
            ],
            "",
        ),
        "deadcode" => (
            "Dead Code Detection",
            "Finds functions, methods, and modules that are never called. Dead code increases compile time and cognitive load.",
            "max_deadcode = 10  (allowed unused items)",
            "Output lists each unused item with file and name. Some may be false positives (public API, test helpers, feature-gated code).",
            vec![
                "Remove confirmed dead code.",
                "For public API items, add #[allow(dead_code)] with a comment explaining why.",
                "For test-only code, move it to a test module or cfg(test).",
            ],
            "",
        ),
        "mutate" => (
            "Mutation Testing",
            "Introduces small bugs (mutations) into your code and checks if tests catch them. Measures true test effectiveness.",
            "min_kill_rate = 0%  (no minimum by default)",
            "Kill rate = (mutants killed / total mutants) × 100. Higher is better. A low rate means tests are not exercising edge cases.",
            vec![
                "Add boundary-value tests (zero, empty, max, null).",
                "Use property-based testing to catch unexpected edge cases.",
                "Review surviving mutants manually — they often reveal real gaps.",
            ],
            "",
        ),
        "linelen" => (
            "Line Length",
            "Counts lines exceeding a character limit (default 100). Long lines reduce readability and cause horizontal scrolling.",
            "max_linelen = 0  (zero lines should exceed the limit)",
            "Output lists each offending line with file and line number.",
            vec![
                "Break long expressions after operators (Rustfmt does this automatically).",
                "Extract nested calls into intermediate variables.",
                "Configure your editor to show a vertical ruler at 100 characters.",
            ],
            "",
        ),
        "halstead" => (
            "Halstead Metrics",
            "Estimates program difficulty, effort, and predicted bugs based on operator/operand counts.",
            "max_halstead_bugs = 2.0  (estimated bugs per file)",
            "Output shows estimated bugs, volume, and difficulty per file. High values suggest dense, hard-to-review code.",
            vec![
                "Split large files into focused modules.",
                "Reduce the number of distinct operators by using higher-level abstractions.",
                "Add extra review for files with high predicted bug counts.",
            ],
            "",
        ),
        "cohesion" => (
            "LCOM4 Cohesion",
            "Measures how tightly related the methods of a class/struct are. Low cohesion means the type has too many responsibilities.",
            "max_cohesion = 5  (allowed low-cohesion types)",
            "Output lists types with LCOM4 > 1. A score of 1 is perfectly cohesive; higher means unrelated methods share state.",
            vec![
                "Split the type into smaller, single-responsibility types.",
                "Move unrelated methods to dedicated helper structs.",
                "Use composition instead of inheritance to share behavior.",
            ],
            "",
        ),
        "comments" => (
            "Comment Ratio",
            "Measures the ratio of comment lines to total lines. Too few means missing context; too many may indicate over-complex code.",
            "min_comment_ratio = 0.05  (5% of lines should be comments)",
            "Output shows the ratio. This is a soft metric — quality of comments matters more than quantity.",
            vec![
                "Add 'why' comments for non-obvious business logic.",
                "Remove redundant comments that restate the code.",
                "Use doc comments on public APIs instead of inline comments where possible.",
            ],
            "",
        ),
        "errhandle" => (
            "Error Handling",
            "Counts unsafe patterns: unwrap(), expect(), panic(), and discarded Result/Option values.",
            "max_errhandle = 50  (allowed occurrences)",
            "Output lists each occurrence with file:line and the pattern used. Each is a potential crash or silent failure.",
            vec![
                "Replace unwrap() with ? or match for graceful error propagation.",
                "Add context to errors with anyhow::Context or similar.",
                "Use must_use lint to catch discarded Results.",
            ],
            "",
        ),
        "typecov" => (
            "Type Coverage",
            "Measures the percentage of variables/parameters with explicit type annotations (Python/JS/TS).",
            "min_typecov = 0%  (off by default; enable for typed ecosystems)",
            "Output shows the percentage and which functions are missing annotations.",
            vec![
                "Enable strict mode (mypy --strict, tsc --noImplicitAny).",
                "Add return-type annotations to all public functions.",
                "Use a type-checking pre-commit hook to catch regressions.",
            ],
            "",
        ),
        "propcov" => (
            "Property Test Coverage",
            "Measures coverage achieved by property-based tests (e.g., proptest, Hypothesis, fast-check).",
            "min_propcov = 0%  (off by default)",
            "Output shows the percentage. Property tests find edge cases that example-based tests miss.",
            vec![
                "Add proptest or Hypothesis to your test suite.",
                "Write properties that invariants must hold (e.g., roundtrip serialize → deserialize).",
                "Run with --nocapture to see counter-examples on failure.",
            ],
            "",
        ),
        "fuzz" => (
            "Fuzz Surface",
            "Counts functions that accept raw bytes or strings and could benefit from fuzz testing.",
            "max_fuzz_risk = 0  (zero unprotected fuzzable endpoints)",
            "Output lists functions that parse untrusted input. These are ideal targets for cargo-fuzz or libFuzzer.",
            vec![
                "Add fuzz targets for parsers and deserializers.",
                "Use fuzzing to find panic paths and infinite loops.",
                "Sanitize all inputs before parsing (length limits, magic-byte checks).",
            ],
            "",
        ),
        "access-control" => (
            "Access Control",
            "Checks for missing authorization on sensitive endpoints and functions.",
            "max = 0  (zero missing auth checks)",
            "Output lists endpoints without explicit access-control checks.",
            vec![
                "Add middleware or decorators for authentication/authorization.",
                "Enforce least-privilege: default-deny, explicit allow.",
                "Run SAST alongside access-control checks for defense in depth.",
            ],
            "",
        ),
        "supply-chain" => (
            "Supply Chain Security",
            "Analyzes dependencies for supply-chain risks: typosquatting, unmaintained crates, suspicious publishers.",
            "max = 0  (zero supply-chain issues)",
            "Output lists each risky dependency with the reason for concern.",
            vec![
                "Pin dependencies and audit new ones before adding.",
                "Use cargo-vet or npm audit for transitive dependency review.",
                "Mirror critical dependencies internally to reduce external exposure.",
            ],
            "",
        ),
        "outdated" => (
            "Outdated Dependencies",
            "Counts direct dependencies that are a full major version behind the latest release.",
            "max_outdated = 0  (zero outdated major versions)",
            "Output lists each outdated dependency with current and latest versions.",
            vec![
                "Schedule monthly dependency update sprints.",
                "Enable Dependabot or Renovate for automatic PRs.",
                "Test thoroughly after major-version upgrades.",
            ],
            "",
        ),
        _ => {
            eprintln!("  {} Unknown tool: '{}'", "✗".red().bold(), tool);
            eprintln!();
            eprintln!("  Available tools:");
            let tools = [
                "crap", "debt", "doccov", "complexity", "taint", "dup", "risk", "coupling",
                "secrets", "vulnscan", "sast", "crypto", "licenses", "deadcode", "mutate",
                "linelen", "halstead", "cohesion", "comments", "errhandle", "typecov",
                "propcov", "fuzz", "access-control", "supply-chain", "outdated",
            ];
            for t in tools.chunks(4) {
                eprintln!("    {}", t.join(", ").bright_black());
            }
            eprintln!();
            eprintln!("  {} Example: cogent explain crap", "▶".cyan());
            return;
        }
    };

    eprintln!("  {}", title.cyan().bold());
    eprintln!("  {}", "─".repeat(visible_len(title)).cyan());
    eprintln!();
    eprintln!("  {}", "What it measures".bold());
    eprintln!("    {}", description);
    eprintln!();
    eprintln!("  {}", "Threshold".bold());
    eprintln!("    {}", threshold);
    eprintln!();
    eprintln!("  {}", "How to read the output".bold());
    eprintln!("    {}", how_to_read);
    eprintln!();
    eprintln!("  {}", "Quick fixes".bold());
    for (i, fix) in fixes.iter().enumerate() {
        eprintln!("    {} {}", format!("{}.", i + 1).cyan(), fix);
    }
    if !see_also.is_empty() {
        eprintln!();
        eprintln!(
            "  {} See {} for deeper explanation and examples.",
            "ℹ".cyan(),
            see_also.cyan()
        );
    }
    eprintln!();
}

fn build_gha_workflow(profile: &ProjectProfile) -> String {
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

fn discover_command(format: &str) {
    // Output tool discovery info (existing functionality)
    // This outputs internal ToolInfo format
    let tools = vec![
        ToolInfo {
            name: "crap".to_string(),
            binary: "crap".to_string(),
            description: "CRAP score calculator (maintenance risk)".to_string(),
            supported_formats: vec![
                "json".to_string(),
                "text".to_string(),
                "sarif".to_string(),
                "ndjson".to_string(),
            ],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["crap-error".to_string(), "crap-warning".to_string()],
        },
        ToolInfo {
            name: "debt".to_string(),
            binary: "debt".to_string(),
            description: "Technical debt scanner (TODO/FIXME/HACK)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "type".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec![
                "debt-todo".to_string(),
                "debt-fixme".to_string(),
                "debt-hack".to_string(),
                "debt-xxx".to_string(),
                "debt-bug".to_string(),
            ],
        },
        ToolInfo {
            name: "doccov".to_string(),
            binary: "doccov".to_string(),
            description: "Documentation coverage for public APIs".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["doccov-missing-doc".to_string()],
        },
        ToolInfo {
            name: "dupfind".to_string(),
            binary: "dupfind".to_string(),
            description: "Code duplication detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["dupfind-duplicate".to_string()],
        },
        ToolInfo {
            name: "coupling".to_string(),
            binary: "coupling".to_string(),
            description: "Module dependency analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["coupling-high".to_string()],
        },
        ToolInfo {
            name: "riskmap".to_string(),
            binary: "riskmap".to_string(),
            description: "Risk map (churn × complexity)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["riskmap-high-risk".to_string()],
        },
        ToolInfo {
            name: "mutate".to_string(),
            binary: "mutate".to_string(),
            description: "Mutation testing (Rust-only)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["mutate-unmutated".to_string()],
        },
        ToolInfo {
            name: "fuzz".to_string(),
            binary: "fuzz".to_string(),
            description: "Fuzz surface analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["fuzz-unsafe-surface".to_string()],
        },
        ToolInfo {
            name: "propcov".to_string(),
            binary: "propcov".to_string(),
            description: "Property test coverage".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["propcov-low-coverage".to_string()],
        },
        ToolInfo {
            name: "taint".to_string(),
            binary: "taint".to_string(),
            description: "Taint analysis (data flow)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["taint-unsafe-flow".to_string()],
        },
        ToolInfo {
            name: "init".to_string(),
            binary: "cogent".to_string(),
            description: "Auto-detect project ecosystem and write .quality.toml. Use --ci for full GitHub Actions + pre-commit hook + baseline bootstrap.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["ecosystem".to_string(), "config_path".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "check".to_string(),
            binary: "cogent".to_string(),
            description: "Run all quality checks in one call. Auto-loads .quality.toml thresholds. Exit 0=pass, 1=fail, 2=error.".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "sarif".to_string(), "junit".to_string(), "findings".to_string(), "ndjson".to_string(), "markdown".to_string()],
            output_fields: vec![
                "passed".to_string(),
                "checks".to_string(),
                "score".to_string(),
                "threshold".to_string(),
                "message".to_string(),
                "findings".to_string(),
                "file_summary".to_string(),
            ],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "watch".to_string(),
            binary: "cogent".to_string(),
            description: "Watch for file changes and re-run checks. Auto-detects test runner and coverage. Use --no-tests for metrics-only mode.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "install-hooks".to_string(),
            binary: "cogent".to_string(),
            description: "Install a pre-commit git hook. Default: full hook (tests + coverage + check). Use --fast for lightweight metrics-only hook.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "complexity".to_string(),
            binary: "complexity".to_string(),
            description: "Cyclomatic complexity violations".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["complexity-high".to_string()],
        },
        ToolInfo {
            name: "linelen".to_string(),
            binary: "linelen".to_string(),
            description: "Line length violations".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["linelen-exceeded".to_string()],
        },
        ToolInfo {
            name: "halstead".to_string(),
            binary: "halstead".to_string(),
            description: "Halstead complexity metrics".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["halstead-high".to_string()],
        },
        ToolInfo {
            name: "secrets".to_string(),
            binary: "secrets".to_string(),
            description: "Hardcoded secret and credential detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["secrets-exposed".to_string()],
        },
        ToolInfo {
            name: "deadcode".to_string(),
            binary: "deadcode".to_string(),
            description: "Dead code and unreachable branch detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["deadcode-unused".to_string()],
        },
        ToolInfo {
            name: "cohesion".to_string(),
            binary: "cohesion".to_string(),
            description: "LCOM4 cohesion analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["cohesion-low".to_string()],
        },
        ToolInfo {
            name: "comments".to_string(),
            binary: "comments".to_string(),
            description: "Comment ratio analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["comments-low".to_string()],
        },
        ToolInfo {
            name: "errhandle".to_string(),
            binary: "errhandle".to_string(),
            description: "Error handling pattern checker (unwrap/expect/panic)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["errhandle-unsafe".to_string()],
        },
        ToolInfo {
            name: "typecov".to_string(),
            binary: "typecov".to_string(),
            description: "Type annotation coverage for Python/JS/TS".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["typecov-low".to_string()],
        },
        ToolInfo {
            name: "vulnscan".to_string(),
            binary: "vulnscan".to_string(),
            description: "CVE vulnerability scanner".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["vulnscan-cve".to_string()],
        },
        ToolInfo {
            name: "sast".to_string(),
            binary: "sast".to_string(),
            description: "Static application security testing (SQLi, XSS, path traversal, smart contracts)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["sast-sqli".to_string(), "sast-xss".to_string(), "sast-pathtraversal".to_string(), "sast-cmdi".to_string(), "sast-reentrancy".to_string(), "sast-access-control".to_string()],
        },
        ToolInfo {
            name: "crypto".to_string(),
            binary: "crypto".to_string(),
            description: "Weak cryptography checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["crypto-weak".to_string(), "crypto-insecure-random".to_string(), "crypto-ecb".to_string()],
        },
        ToolInfo {
            name: "licenses".to_string(),
            binary: "licenses".to_string(),
            description: "OSS license compliance checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["license-violation".to_string()],
        },
        ToolInfo {
            name: "outdated".to_string(),
            binary: "outdated".to_string(),
            description: "Outdated dependency checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["outdated-dependency".to_string()],
        },
        ToolInfo {
            name: "report".to_string(),
            binary: "cogent".to_string(),
            description: "Generate a human-readable audit report (HTML or Markdown)".to_string(),
            supported_formats: vec!["html".to_string(), "markdown".to_string()],
            output_fields: vec!["passed".to_string(), "checks".to_string(), "score".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "diff".to_string(),
            binary: "cogent".to_string(),
            description: "Compare two check JSON snapshots".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "access-control".to_string(),
            binary: "access-control".to_string(),
            description: "Access control checker — missing auth guards, hardcoded credentials, IAM policies, CORS".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["acl-auth".to_string(), "acl-cred".to_string(), "acl-iam".to_string(), "acl-cors".to_string()],
        },
        ToolInfo {
            name: "supply-chain".to_string(),
            binary: "supply-chain".to_string(),
            description: "Supply chain checker — dependency integrity, typosquatting, abandoned packages, unpinned deps".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "package".to_string(), "version".to_string(), "help".to_string()],
            rule_ids: vec!["supply-lock".to_string(), "supply-typo".to_string(), "supply-pin".to_string()],
        },
        ToolInfo {
            name: "sbom".to_string(),
            binary: "sbom".to_string(),
            description: "Software bill of materials — generate CycloneDX/SPDX SBOM from Cargo/npm/go.mod".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string()],
            output_fields: vec!["components".to_string(), "format".to_string(), "version".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "policy".to_string(),
            binary: "cogent".to_string(),
            description: "Validate or enforce audit policies defined in .cogent-policies/. Subcommands: validate, check.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["valid".to_string(), "warnings".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "exception".to_string(),
            binary: "cogent".to_string(),
            description: "Manage approved exceptions (false positive overrides). Subcommands: add, list, approve, revoke.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["id".to_string(), "rule_id".to_string(), "file".to_string(), "status".to_string(), "reviewer".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "remediate".to_string(),
            binary: "cogent".to_string(),
            description: "Track remediation status of findings. Use --verify to confirm fixes applied.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["open".to_string(), "resolved".to_string(), "overdue".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "audit-trail".to_string(),
            binary: "cogent".to_string(),
            description: "Query and verify the signed JSONL audit trail. Use --verify to detect tampering, --since for time-bounded queries.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["timestamp".to_string(), "actor".to_string(), "command".to_string(), "scope".to_string(), "findings_count".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "audit".to_string(),
            binary: "cogent".to_string(),
            description: "Full DIY auditor: runs all checks, enriches every finding with suggested_fix + compliance controls, writes audit trail. --format agent emits NDJSON (one finding per line) for agent consumption.".to_string(),
            supported_formats: vec!["agent".to_string(), "json".to_string(), "markdown".to_string()],
            output_fields: vec![
                "type".to_string(),
                "tool".to_string(),
                "rule_id".to_string(),
                "severity".to_string(),
                "file".to_string(),
                "line".to_string(),
                "message".to_string(),
                "suggested_fix".to_string(),
                "controls".to_string(),
                "suppressed".to_string(),
                "evidence".to_string(),
            ],
            rule_ids: vec![],
        },
    ];

    match format {
        "text" => {
            for tool in &tools {
                println!("{} ({})", tool.name, tool.binary);
                println!("  Description: {}", tool.description);
                println!("  Supported Formats: {}", tool.supported_formats.join(", "));
                println!("  Output Fields: {}", tool.output_fields.join(", "));
                println!("  Rule IDs: {}", tool.rule_ids.join(", "));
                println!();
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&tools).unwrap());
        }
    }
}

// MAIN
// ═══════════════════════════════════════════

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Check {
            path,
            recursive,
            format,
            coverage,
            max_crap,
            min_doc,
            max_debt,
            max_complexity_violations,
            max_taint,
            max_duplication,
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
            max_outdated,
            skip,
            only,
            ci,
            verbose,
            pr_comment,
            force,
        } => {
            // --ci: force JSON output and suppress progress (no TTY)
            let format = if ci { "json".to_string() } else { format };
            if ci {
                std::env::set_var("COGENT_NO_PROGRESS", "1");
            }

            // Guard: require .quality.toml unless --force is used
            if !force && !std::path::Path::new(".quality.toml").exists() {
                eprintln!();
                eprintln!(
                    "  {} No {} found.",
                    "!".yellow().bold(),
                    ".quality.toml".cyan()
                );
                eprintln!(
                    "    {} Run {} to auto-detect your project and generate one.",
                    "→".cyan(),
                    "cogent init".cyan().bold()
                );
                eprintln!();
                eprintln!(
                    "    {} Use {} to run with hardcoded defaults anyway.",
                    "→".cyan(),
                    "cogent check . --force".cyan().bold()
                );
                eprintln!();
                std::process::exit(2);
            }

            // Auto-load .quality.toml if present; CLI flags override file values.
            let (
                max_crap,
                min_doc,
                max_debt,
                max_complexity_violations,
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
            ) = load_config_thresholds(
                ".quality.toml",
                (
                    max_crap,
                    min_doc,
                    max_debt,
                    max_complexity_violations,
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
                ),
            );

            let skip_list: Vec<String> = skip
                .map(|s| s.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default();

            // --only builds an explicit allowlist; if set, only those names run
            let only_list: Vec<String> = only
                .map(|s| s.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default();

            let should_run = |name: &str| -> bool {
                if !only_list.is_empty() {
                    only_list.contains(&name.to_string())
                } else {
                    !skip_list.contains(&name.to_string())
                }
            };

            let check_start = Instant::now();
            let show_progress = format == "text";

            // Helper: run a check with a live spinner on text format
            macro_rules! run_check {
                ($label:expr, $expr:expr) => {{
                    if show_progress {
                        let label = $label;
                        let t = Instant::now();
                        let result = run_with_spinner(label, || $expr);
                        let elapsed = format_elapsed(t.elapsed());
                        let detail = &result.message;
                        let icon = if result.passed {
                            "✓".green().bold()
                        } else {
                            "✗".red().bold()
                        };
                        let name_col = if result.passed {
                            label.normal()
                        } else {
                            label.red()
                        };
                        let msg_col = if result.passed {
                            detail.bright_black()
                        } else {
                            detail.red()
                        };
                        eprintln!(
                            "  {} {:<18} {}  {}",
                            icon,
                            name_col,
                            elapsed.bright_black(),
                            msg_col
                        );
                        if !result.passed || verbose {
                            print_offenders(&result);
                        }
                        result
                    } else {
                        $expr
                    }
                }};
            }

            let mut checks = Vec::new();

            if should_run("crap") {
                checks.push(run_check!(
                    "crap",
                    check_crap(&path, recursive, &coverage, max_crap)
                ));
            }
            if should_run("debt") {
                checks.push(run_check!("debt", check_debt(&path, recursive, max_debt)));
            }
            if should_run("doc") {
                checks.push(run_check!(
                    "doc_coverage",
                    check_doc_coverage(&path, recursive, min_doc)
                ));
            }
            if should_run("complexity") {
                checks.push(run_check!(
                    "complexity",
                    check_complexity(&path, recursive, 10, max_complexity_violations)
                ));
            }
            if should_run("taint") {
                checks.push(run_check!(
                    "taint",
                    check_taint(&path, recursive, max_taint)
                ));
            }
            if should_run("dup") || should_run("dupfind") || should_run("duplication") {
                checks.push(run_check!(
                    "duplication",
                    check_dupfind(&path, recursive, max_duplication)
                ));
            }
            if should_run("risk") || should_run("riskmap") {
                checks.push(run_check!(
                    "riskmap",
                    check_riskmap(&path, recursive, max_risk)
                ));
            }
            if should_run("coupling") {
                checks.push(run_check!("coupling", check_coupling(&path, max_coupling)));
            }
            if should_run("propcov") {
                checks.push(run_check!(
                    "propcov",
                    check_propcov(&path, recursive, min_propcov)
                ));
            }
            if should_run("fuzz") {
                checks.push(run_check!(
                    "fuzz",
                    check_fuzz(&path, recursive, max_fuzz_risk)
                ));
            }
            if should_run("linelen") {
                checks.push(run_check!(
                    "linelen",
                    check_linelen(&path, recursive, max_linelen)
                ));
            }
            if should_run("halstead") {
                checks.push(run_check!(
                    "halstead",
                    check_halstead(&path, recursive, max_halstead_bugs)
                ));
            }
            if should_run("secrets") {
                checks.push(run_check!(
                    "secrets",
                    check_secrets(&path, recursive, max_secrets)
                ));
            }
            if should_run("deadcode") {
                checks.push(run_check!(
                    "deadcode",
                    check_deadcode(&path, recursive, max_deadcode)
                ));
            }
            if should_run("cohesion") {
                checks.push(run_check!(
                    "cohesion",
                    check_cohesion(&path, recursive, max_cohesion)
                ));
            }
            if should_run("comments") {
                checks.push(run_check!(
                    "comments",
                    check_comments(&path, recursive, min_comment_ratio)
                ));
            }
            if should_run("errhandle") {
                checks.push(run_check!(
                    "errhandle",
                    check_errhandle(&path, recursive, max_errhandle)
                ));
            }
            if should_run("typecov") && min_typecov > 0.0 {
                checks.push(run_check!(
                    "typecov",
                    check_typecov(&path, recursive, min_typecov)
                ));
            }
            if should_run("vulnscan") {
                checks.push(run_check!(
                    "vulnscan",
                    check_vulnscan(&path, max_vuln_critical, max_vuln_high)
                ));
            }
            if should_run("sast") {
                checks.push(run_check!("sast", check_sast(&path, recursive, max_sast)));
            }
            if should_run("crypto") {
                checks.push(run_check!(
                    "crypto",
                    check_crypto(&path, recursive, max_crypto)
                ));
            }
            if should_run("licenses") {
                checks.push(run_check!(
                    "licenses",
                    check_licenses(&path, max_license_violations)
                ));
            }
            if should_run("mutate") {
                checks.push(run_check!("mutate", {
                    let args = vec![&path, "--format", "json"];
                    let res = run_tool("mutation-test", "mutate", &args, Instant::now());
                    let score = res
                        .data
                        .get("summary")
                        .and_then(|s| s.get("kill_rate"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let min_kill_rate = std::fs::read_to_string(".quality.toml")
                        .ok()
                        .and_then(|content| {
                            content.lines().find_map(|line| {
                                let line = line.trim();
                                if line.starts_with("min_kill_rate") {
                                    line.split('=').nth(1)?.trim().parse::<f64>().ok()
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(0.0);
                    let passed = score >= min_kill_rate;
                    let msg = if passed {
                        format!("Mutation testing passed (kill rate {:.1}%)", score)
                    } else {
                        format!(
                            "Mutation testing failed (kill rate {:.1}% < {:.0}%)",
                            score, min_kill_rate
                        )
                    };
                    CheckResult {
                        name: "mutate".into(),
                        passed,
                        score: Some(score),
                        threshold: Some(min_kill_rate),
                        message: msg,
                        details: serde_json::json!({}),
                        severity: None,
                        help: None,
                        rule_id: None,
                        findings: Vec::new(),
                    }
                }));
            }
            if should_run("access-control") {
                checks.push(run_check!("access-control", {
                    check_access_control(&path, true, 0)
                }));
            }

            if should_run("supply-chain") {
                checks.push(run_check!("supply-chain", check_supply_chain(&path, 0)));
            }

            if should_run("outdated") {
                checks.push(run_check!("outdated", check_outdated(&path, max_outdated)));
            }

            let passed = checks.iter().all(|c| c.passed);
            let total_funcs: usize = checks
                .iter()
                .filter_map(|c| c.details.get("total_functions").and_then(|v| v.as_u64()))
                .map(|v| v as usize)
                .sum();

            let passed_count = checks.iter().filter(|c| c.passed).count();
            let failed_count = checks.len() - passed_count;
            let total_checks = checks.len();

            let report = CheckReport {
                passed,
                path: path.clone(),
                checks: checks.clone(),
                summary: CheckSummary {
                    total_checks,
                    passed_checks: passed_count,
                    failed_checks: failed_count,
                    functions_analyzed: total_funcs,
                    avg_complexity: 0.0,
                    avg_crap: 0.0,
                },
                file_summary: aggregate_file_summary(&checks),
            };
            let (health, grade) = health_score(&report.checks);

            if pr_comment {
                let md = pr_comment_md(&report, &path);
                println!("{}", md);
                std::process::exit(if passed { 0 } else { 1 });
            }

            match format.as_str() {
                "text" => {
                    print_summary_box(
                        "COGENT CHECK",
                        passed,
                        &path,
                        passed_count,
                        total_checks,
                        check_start.elapsed(),
                        &report.checks,
                    );
                    if !passed {
                        print_severity_grouped(&report.checks);
                        print_fix_summary(&report.checks);
                    }
                }
                "ndjson" => output_ndjson(&report),
                "sarif" => output_sarif(&report),
                "junit" => output_junit(&report),
                "findings" => output_findings_ndjson(&report),
                "markdown" => output_markdown(&report, &path),
                _ => output_json(&report),
            }

            // CI artifact generation
            if ci {
                let summary = serde_json::json!({
                    "passed": report.passed,
                    "score": health,
                    "grade": grade.to_string(),
                    "failed_checks": report.checks.iter().filter(|c| !c.passed).map(|c| c.name.clone()).collect::<Vec<_>>(),
                    "critical_findings": report.checks.iter().map(|c| c.findings.iter().filter(|f| f.severity == "critical").count()).sum::<usize>(),
                    "report_url": "./cogent-report.html",
                });
                if let Err(e) = std::fs::write(
                    "cogent-summary.json",
                    serde_json::to_string_pretty(&summary).unwrap_or_default(),
                ) {
                    eprintln!("Warning: could not write cogent-summary.json: {}", e);
                }
                // Also write full HTML report to disk
                let html = render_html_report(
                    &report,
                    &path,
                    &chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
                    &[
                        "taint",
                        "secrets",
                        "sast",
                        "crypto",
                        "vulnscan",
                        "access-control",
                    ],
                    &[
                        "crap",
                        "debt",
                        "doc_coverage",
                        "complexity",
                        "duplication",
                        "riskmap",
                        "coupling",
                        "propcov",
                        "fuzz",
                        "linelen",
                        "halstead",
                        "deadcode",
                        "cohesion",
                        "comments",
                        "errhandle",
                        "typecov",
                    ],
                    &["licenses", "outdated", "supply-chain"],
                );
                if let Err(e) = std::fs::write("cogent-report.html", html) {
                    eprintln!("Warning: could not write cogent-report.html: {}", e);
                }
            }

            let total_findings: usize = report.checks.iter().map(|c| c.findings.len()).sum();
            audit::append_audit_trail("check", &path, total_findings, check_start.elapsed());

            if passed {
                0
            } else {
                1
            }
        }

        Commands::Crap {
            path,
            recursive,
            coverage,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("crap", || check_crap(&path, recursive, &coverage, 30.0))
            } else {
                check_crap(&path, recursive, &coverage, 30.0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} crap  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Debt {
            path,
            recursive,
            marker: _,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("debt", || check_debt(&path, recursive, 1000))
            } else {
                check_debt(&path, recursive, 1000)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} debt  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Doccov {
            path,
            recursive,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("doccov", || check_doc_coverage(&path, recursive, 0.0))
            } else {
                check_doc_coverage(&path, recursive, 0.0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} doccov  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Dupfind {
            path,
            recursive,
            min_lines,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("dupfind", || {
                    check_dupfind(&path, recursive, min_lines as f64)
                })
            } else {
                check_dupfind(&path, recursive, min_lines as f64)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Complexity {
            path,
            recursive,
            min_complexity,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("complexity", || {
                    check_complexity(&path, recursive, min_complexity, 0)
                })
            } else {
                check_complexity(&path, recursive, min_complexity, 0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} complexity  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Taint {
            path,
            recursive,
            max_taint,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("taint", || check_taint(&path, recursive, max_taint))
            } else {
                check_taint(&path, recursive, max_taint)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Coupling {
            path,
            max_coupling,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("coupling", || check_coupling(&path, max_coupling))
            } else {
                check_coupling(&path, max_coupling)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Riskmap {
            path,
            max_risk,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("riskmap", || check_riskmap(&path, false, max_risk))
            } else {
                check_riskmap(&path, false, max_risk)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Mutate { path, format } => {
            let args = vec![&path, "--format", "json"];
            let res = run_tool("mutation-test", "mutate", &args, Instant::now());
            let passed = res.success;
            let msg = res.error.unwrap_or_else(|| {
                res.data
                    .get("summary")
                    .and_then(|s| s.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });
            match format.as_str() {
                "text" => {
                    println!("{}", msg);
                }
                _ => println!("{}", serde_json::to_string_pretty(&res.data).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Fuzz {
            path,
            recursive,
            max_fuzz_risk,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("fuzz", || check_fuzz(&path, recursive, max_fuzz_risk))
            } else {
                check_fuzz(&path, recursive, max_fuzz_risk)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Propcov {
            path,
            recursive,
            min_propcov,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("propcov", || check_propcov(&path, recursive, min_propcov))
            } else {
                check_propcov(&path, recursive, min_propcov)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Linelen {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("linelen", || {
                    check_linelen(&path, recursive, max_violations)
                })
            } else {
                check_linelen(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Halstead {
            path,
            recursive,
            max_bugs,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("halstead", || check_halstead(&path, recursive, max_bugs))
            } else {
                check_halstead(&path, recursive, max_bugs)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Secrets {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("secrets", || check_secrets(&path, recursive, max_findings))
            } else {
                check_secrets(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Deadcode {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("deadcode", || {
                    check_deadcode(&path, recursive, max_findings)
                })
            } else {
                check_deadcode(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Cohesion {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("cohesion", || {
                    check_cohesion(&path, recursive, max_violations)
                })
            } else {
                check_cohesion(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Comments {
            path,
            recursive,
            min_ratio,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("comments", || check_comments(&path, recursive, min_ratio))
            } else {
                check_comments(&path, recursive, min_ratio)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Errhandle {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("errhandle", || {
                    check_errhandle(&path, recursive, max_violations)
                })
            } else {
                check_errhandle(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Typecov {
            path,
            recursive,
            min_pct,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("typecov", || check_typecov(&path, recursive, min_pct))
            } else {
                check_typecov(&path, recursive, min_pct)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Vulnscan {
            path,
            max_critical,
            max_high,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("vulnscan", || check_vulnscan(&path, max_critical, max_high))
            } else {
                check_vulnscan(&path, max_critical, max_high)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Sast {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("sast", || check_sast(&path, recursive, max_findings))
            } else {
                check_sast(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Crypto {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("crypto", || check_crypto(&path, recursive, max_findings))
            } else {
                check_crypto(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Licenses {
            path,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("licenses", || check_licenses(&path, max_violations))
            } else {
                check_licenses(&path, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Outdated {
            path,
            max_major_behind,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("outdated", || check_outdated(&path, max_major_behind))
            } else {
                check_outdated(&path, max_major_behind)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::AccessControl {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("access-control", || {
                    check_access_control(&path, recursive, max_violations)
                })
            } else {
                check_access_control(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::SupplyChain {
            path,
            max_risks,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("supply-chain", || check_supply_chain(&path, max_risks))
            } else {
                check_supply_chain(&path, max_risks)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Setup => {
            setup_command();
            0
        }

        Commands::Init { output, ci } => {
            let detect_start = Instant::now();
            let profile = run_with_spinner("detecting project ecosystem", || detect_project("."));

            // Determine detection reason
            let reason = if std::path::Path::new("Cargo.toml").exists() {
                "Cargo.toml found"
            } else if std::path::Path::new("go.mod").exists() {
                "go.mod found"
            } else if std::path::Path::new("pyproject.toml").exists()
                || std::path::Path::new("setup.py").exists()
            {
                "pyproject.toml / setup.py found"
            } else if std::path::Path::new("package.json").exists() {
                "package.json found"
            } else {
                "no known manifest found — using generic defaults"
            };

            eprintln!(
                "  {} detected: {}  ({})  {}",
                "✓".green().bold(),
                profile.ecosystem.to_string().cyan().bold(),
                reason.bright_black(),
                if profile.test_cmd.is_empty() {
                    "no test runner".to_string().bright_black().to_string()
                } else {
                    format!("test: {}", profile.test_cmd.join(" "))
                        .bright_black()
                        .to_string()
                }
            );
            let _ = detect_start;
            if ci {
                init_ci(&output, &profile)
            } else {
                let write_start = Instant::now();
                generate_config(&output, &profile);
                eprintln!(
                    "  {} wrote {}  ({})",
                    "✓".green().bold(),
                    output.cyan(),
                    format_elapsed(write_start.elapsed()).bright_black()
                );
                eprintln!();
                eprintln!("  {} Key thresholds chosen:", "▶".cyan().bold());
                eprintln!(
                    "    {} max_crap    = {}",
                    "·".bright_black(),
                    profile.max_crap.to_string().cyan()
                );
                eprintln!(
                    "    {} min_doc     = {}%",
                    "·".bright_black(),
                    profile.min_doc.to_string().cyan()
                );
                eprintln!(
                    "    {} max_debt    = {}",
                    "·".bright_black(),
                    profile.max_debt.to_string().cyan()
                );
                eprintln!(
                    "    {} max_complexity_violations = {}",
                    "·".bright_black(),
                    profile.max_complexity_violations.to_string().cyan()
                );
                eprintln!();
                eprintln!(
                    "  {} {} runs 20+ checks and produces a 0-100 score + letter grade.",
                    "▶".cyan().bold(),
                    "cogent check .".cyan().bold()
                );
                eprintln!();
                eprintln!("  {} Next steps:", "▶".cyan().bold());
                eprintln!(
                    "    1. {} cogent check .          {}",
                    "$".bright_black(),
                    "— run all checks now".bright_black()
                );
                eprintln!(
                    "    2. {} cogent report .         {}",
                    "$".bright_black(),
                    "— generate HTML audit report".bright_black()
                );
                eprintln!(
                    "    3. {} cogent init --ci        {}",
                    "$".bright_black(),
                    "— wire GitHub Actions + pre-commit hook".bright_black()
                );
                eprintln!(
                    "    4. {} cogent watch .          {}",
                    "$".bright_black(),
                    "— live re-check on file save".bright_black()
                );
                eprintln!();
                eprintln!(
                    "  {} Tip: edit {} to tune thresholds for your project.",
                    "ℹ".cyan(),
                    output.cyan()
                );
                0
            }
        }

        Commands::Explain { tool } => {
            explain_command(&tool);
            0
        }

        Commands::Discover { format } => {
            discover_command(&format);
            0
        }

        Commands::Run {
            path,
            config,
            format,
            baseline,
            no_fail_on_regression,
        } => run_batch(
            &path,
            &config,
            &format,
            baseline.as_deref(),
            no_fail_on_regression,
        ),

        Commands::History {
            action,
            dir,
            last,
            report,
            format,
        } => history_command(&action, &dir, last, report.as_deref(), &format),

        Commands::InstallHooks { repo, fast } => install_hooks(&repo, fast),

        Commands::UninstallHooks { repo } => uninstall_hooks(&repo),

        Commands::Watch {
            path,
            checks,
            debounce_ms,
            no_tests,
            full,
        } => watch_mode(&path, &checks, debounce_ms, no_tests, full),

        Commands::Report {
            path,
            format,
            output,
            project,
            from_json,
            skip,
            open,
        } => report_command(
            &path,
            &format,
            output.as_deref(),
            project.as_deref(),
            from_json.as_deref(),
            skip.as_deref(),
            open,
        ),

        Commands::Diff {
            before,
            after,
            format,
        } => diff_command(&before, &after, &format),

        Commands::Serve { port, history_dir } => {
            serve_command(port, &history_dir);
            0
        }

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            use clap_complete::{generate, Shell};
            let shell = match shell.as_str() {
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                "fish" => Shell::Fish,
                "powershell" => Shell::PowerShell,
                "elvish" => Shell::Elvish,
                _ => {
                    eprintln!(
                        "Unknown shell '{}'. Supported: bash, zsh, fish, powershell, elvish",
                        shell
                    );
                    std::process::exit(2);
                }
            };
            let mut cli = Cli::command();
            generate(shell, &mut cli, "cogent", &mut std::io::stdout());
            0
        }

        Commands::Policy { action } => match action {
            PolicyAction::Validate => {
                let known_tools: Vec<String> = [
                    "secrets",
                    "sast",
                    "debt",
                    "dupfind",
                    "deadcode",
                    "linelen",
                    "comments",
                    "coupling",
                    "cohesion",
                    "halstead",
                    "crap",
                    "riskmap",
                    "cryptocheck",
                    "errhandle",
                    "taint",
                    "typecov",
                    "propcov",
                    "fuzz",
                    "licenses",
                    "supply-chain",
                    "access-control",
                    "vulnscan",
                    "mutate",
                    "doccov",
                    "sbom",
                    "complexity",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                let policies = audit::discover_policies(".");
                if policies.is_empty() {
                    eprintln!("No policies found in ./.cogent-policies/");
                    0
                } else {
                    let mut all_valid = true;
                    for p in &policies {
                        let result = audit::validate_policy(p, &known_tools);
                        println!(
                            "{}  {}",
                            if result.valid {
                                "✓".green()
                            } else {
                                "✗".red()
                            },
                            p.display()
                        );
                        for w in &result.warnings {
                            println!("  ⚠  {}", w);
                        }
                        for e in &result.errors {
                            println!("  ✗  {}", e);
                            all_valid = false;
                        }
                    }
                    if all_valid {
                        0
                    } else {
                        1
                    }
                }
            }
            PolicyAction::Check {
                path,
                format,
                force,
            } => {
                // Placeholder: run only tools referenced in policies
                println!(
                    "Policy-based check on {} (format: {}, force: {})",
                    path, format, force
                );
                0
            }
        },

        Commands::Exception { action } => match action {
            ExceptionAction::Add {
                finding_id,
                rule_id,
                file,
                reason,
                reviewer,
            } => match audit::add_exception(&finding_id, &rule_id, &file, &reason, &reviewer) {
                Ok(id) => {
                    println!(
                        "Exception {} proposed (pending approval by {})",
                        id, reviewer
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Failed to add exception: {}", e);
                    2
                }
            },
            ExceptionAction::List { status } => {
                let exceptions = audit::list_exceptions(status.as_deref());
                if exceptions.is_empty() {
                    println!("No exceptions found.");
                } else {
                    println!(
                        "{:<10} {:<16} {:<20} {:<12} Reviewer",
                        "ID", "Finding", "Rule", "Status"
                    );
                    for e in &exceptions {
                        println!(
                            "{:<10} {:<16} {:<20} {:<12} {}",
                            e.id, e.finding_id, e.rule_id, e.status, e.reviewer
                        );
                    }
                }
                0
            }
            ExceptionAction::Approve { id } => match audit::approve_exception(&id) {
                Ok(()) => {
                    println!("Exception {} approved.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to approve exception: {}", e);
                    2
                }
            },
            ExceptionAction::Revoke { id } => match audit::revoke_exception(&id) {
                Ok(()) => {
                    println!("Exception {} revoked.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to revoke exception: {}", e);
                    2
                }
            },
        },

        Commands::Remediate { verify, path } => {
            if verify {
                println!("Verifying remediation on path: {}", path);
                // Placeholder: would run check, compare with remediation log
                audit::print_remediation_summary();
                0
            } else {
                println!("Showing remediation status for: {}", path);
                audit::print_remediation_summary();
                0
            }
        }

        Commands::AuditTrail {
            verify,
            command,
            since,
        } => {
            if verify {
                let (ok, errors) = audit::verify_audit_trail();
                if ok {
                    println!("{} Audit trail integrity verified.", "✓".green());
                } else {
                    println!("{} Audit trail verification failed:", "✗".red());
                    for e in &errors {
                        println!("  {}", e);
                    }
                }
                if ok {
                    0
                } else {
                    2
                }
            } else {
                let entries = audit::query_audit_trail(since.as_deref(), command.as_deref());
                if entries.is_empty() {
                    println!("No audit trail entries found.");
                } else {
                    println!(
                        "{:<26} {:<12} {:<20} {:<20} Findings",
                        "Timestamp", "Actor", "Command", "Scope"
                    );
                    for e in &entries {
                        println!(
                            "{:<26} {:<12} {:<20} {:<20} {}",
                            e.timestamp, e.actor, e.command, e.scope, e.findings_count
                        );
                    }
                }
                0
            }
        }

        Commands::Audit {
            path,
            format,
            only,
            evidence,
            framework,
            checks,
            skip,
            ci,
            verify,
        } => {
            if ci {
                std::env::set_var("COGENT_NO_PROGRESS", "1");
            }

            let audit_start = Instant::now();

            // Determine which check categories to run
            let security_checks = [
                "secrets",
                "sast",
                "crypto",
                "taint",
                "vulnscan",
                "access-control",
            ];
            let quality_checks = [
                "crap",
                "debt",
                "doccov",
                "complexity",
                "dupfind",
                "riskmap",
                "coupling",
                "propcov",
                "fuzz",
                "linelen",
                "halstead",
                "deadcode",
                "cohesion",
                "comments",
                "errhandle",
                "typecov",
            ];
            let compliance_checks = ["licenses", "supply-chain", "outdated", "sbom"];

            let active_categories: Vec<&str> = match only.as_deref() {
                Some("security") => security_checks.to_vec(),
                Some("quality") => quality_checks.to_vec(),
                Some("compliance") => compliance_checks.to_vec(),
                _ => security_checks
                    .iter()
                    .chain(quality_checks.iter())
                    .chain(compliance_checks.iter())
                    .copied()
                    .collect(),
            };

            let skip_set: std::collections::HashSet<String> = skip
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            let only_set: std::collections::HashSet<String> = checks
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            let should_run_audit = |name: &str| -> bool {
                if !only_set.is_empty() {
                    return only_set.contains(name);
                }
                if skip_set.contains(name) {
                    return false;
                }
                active_categories.contains(&name)
            };

            // Run checks using existing check_* functions with default thresholds from .quality.toml
            let mut checks_run: Vec<CheckResult> = Vec::new();

            macro_rules! run_audit_check {
                ($name:expr, $expr:expr) => {
                    if should_run_audit($name) {
                        checks_run.push(run_with_spinner($name, || $expr));
                    }
                };
            }

            run_audit_check!("secrets", check_secrets(&path, true, 0));
            run_audit_check!("sast", check_sast(&path, true, 0));
            run_audit_check!("crypto", check_crypto(&path, true, 0));
            run_audit_check!("taint", check_taint(&path, true, 0));
            run_audit_check!("vulnscan", check_vulnscan(&path, 0, 0));
            run_audit_check!("access-control", check_access_control(&path, true, 0));
            run_audit_check!("licenses", check_licenses(&path, 0));
            run_audit_check!("supply-chain", check_supply_chain(&path, 0));
            run_audit_check!("crap", check_crap(&path, true, &None, 30.0));
            run_audit_check!("debt", check_debt(&path, true, usize::MAX));
            run_audit_check!("doccov", check_doc_coverage(&path, true, 0.0));
            run_audit_check!(
                "complexity",
                check_complexity(&path, true, 10u32, usize::MAX)
            );
            run_audit_check!("dupfind", check_dupfind(&path, true, 100.0));
            run_audit_check!("riskmap", check_riskmap(&path, true, 100.0));
            run_audit_check!("coupling", check_coupling(&path, usize::MAX));
            run_audit_check!("deadcode", check_deadcode(&path, true, usize::MAX));
            run_audit_check!("cohesion", check_cohesion(&path, true, usize::MAX));
            run_audit_check!("comments", check_comments(&path, true, 0.0));
            run_audit_check!("errhandle", check_errhandle(&path, true, usize::MAX));
            run_audit_check!("halstead", check_halstead(&path, true, 100.0));
            run_audit_check!("linelen", check_linelen(&path, true, usize::MAX));

            // Collect all findings
            let mut all_findings: Vec<Finding> =
                checks_run.iter().flat_map(|c| c.findings.clone()).collect();

            // Enrich findings
            let use_framework = framework.as_deref().unwrap_or("");
            for f in &mut all_findings {
                // Always populate suggested_fix and controls
                if let Some((desc, diff, conf)) = audit::suggested_fix_for(&f.rule_id) {
                    f.suggested_fix = Some(SuggestedFix {
                        description: desc,
                        diff,
                        confidence: conf,
                    });
                }
                let ctrl = audit::controls_for(&f.rule_id);
                if !ctrl.is_empty() {
                    f.controls = Some(ctrl);
                }
                // Evidence only when requested
                if evidence {
                    audit::enrich_finding(f);
                }
            }

            let total_findings = all_findings.len();
            let critical = all_findings
                .iter()
                .filter(|f| f.severity == "critical")
                .count();
            let high = all_findings.iter().filter(|f| f.severity == "high").count();
            let medium = all_findings
                .iter()
                .filter(|f| f.severity == "medium")
                .count();
            let low = all_findings.iter().filter(|f| f.severity == "low").count();
            let passed = checks_run.iter().all(|c| c.passed);
            let (health, grade) = health_score(&checks_run);
            let duration = audit_start.elapsed();

            // Persist new findings to remediation log; optionally auto-close resolved ones
            let newly_recorded = audit::record_findings(&all_findings).len();
            let auto_closed = if verify {
                let closed = audit::verify_remediation(&all_findings);
                if !closed.is_empty() && !ci {
                    for id in &closed {
                        eprintln!("  ✓ auto-closed: {}", id);
                    }
                }
                closed.len()
            } else {
                0
            };

            // Write audit trail entry
            audit::append_audit_trail("audit", &path, total_findings, duration);

            match format.as_str() {
                "agent" => {
                    // NDJSON: one finding per line, then a summary line
                    for f in &all_findings {
                        let suppressed = audit::is_suppressed(&f.rule_id, &f.file);
                        let mut obj = serde_json::to_value(f).unwrap_or(serde_json::Value::Null);
                        if let Some(o) = obj.as_object_mut() {
                            o.insert("type".to_string(), serde_json::json!("finding"));
                            o.insert("suppressed".to_string(), serde_json::json!(suppressed));
                            if !use_framework.is_empty() {
                                // controls already populated above
                            }
                        }
                        println!("{}", serde_json::to_string(&obj).unwrap_or_default());
                    }
                    // Summary line
                    let controls_affected: Vec<String> = all_findings
                        .iter()
                        .filter_map(|f| f.controls.as_ref())
                        .flatten()
                        .cloned()
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    let summary = serde_json::json!({
                        "type": "summary",
                        "passed": passed,
                        "score": health,
                        "grade": grade.to_string(),
                        "total_findings": total_findings,
                        "critical": critical,
                        "high": high,
                        "medium": medium,
                        "low": low,
                        "controls_affected": controls_affected,
                        "checks_run": checks_run.len(),
                        "duration_ms": duration.as_millis(),
                        "path": path,
                        "framework": use_framework,
                        "newly_recorded": newly_recorded,
                        "auto_closed": auto_closed,
                    });
                    println!("{}", serde_json::to_string(&summary).unwrap_or_default());
                }
                "json" => {
                    let report = CheckReport {
                        passed,
                        path: path.clone(),
                        checks: checks_run.clone(),
                        summary: CheckSummary {
                            total_checks: checks_run.len(),
                            passed_checks: checks_run.iter().filter(|c| c.passed).count(),
                            failed_checks: checks_run.iter().filter(|c| !c.passed).count(),
                            functions_analyzed: 0,
                            avg_complexity: 0.0,
                            avg_crap: 0.0,
                        },
                        file_summary: aggregate_file_summary(&checks_run),
                    };
                    output_json(&report);
                }
                "markdown" => {
                    let report = CheckReport {
                        passed,
                        path: path.clone(),
                        checks: checks_run.clone(),
                        summary: CheckSummary {
                            total_checks: checks_run.len(),
                            passed_checks: checks_run.iter().filter(|c| c.passed).count(),
                            failed_checks: checks_run.iter().filter(|c| !c.passed).count(),
                            functions_analyzed: 0,
                            avg_complexity: 0.0,
                            avg_crap: 0.0,
                        },
                        file_summary: vec![],
                    };
                    output_markdown(&report, &path);
                }
                _ => {
                    eprintln!("Unknown format '{}'. Use: agent, json, markdown", format);
                    std::process::exit(2);
                }
            }

            if ci && total_findings > 0 {
                1
            } else if passed {
                0
            } else {
                1
            }
        }
    };

    std::process::exit(exit_code);
}

fn run_tool(crate_name: &str, bin_name: &str, args: &[&str], tool_start: Instant) -> ToolResult {
    use cogent_common::*;
    use std::process::{Command, Stdio};

    let output = Command::new(bin_name)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) if o.status.success() || !o.stdout.is_empty() => o,
        _ => {
            let cargo_output = Command::new("cargo")
                .args(["run", "--quiet", "-p", crate_name, "--bin", bin_name, "--"])
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match cargo_output {
                Ok(o) => o,
                Err(e) => {
                    let msg = if e.kind() == std::io::ErrorKind::NotFound {
                        format!(
                            "Binary '{}' not found. Install with: cargo install --path crates/{} (error: {})",
                            bin_name, crate_name, e
                        )
                    } else {
                        format!("Failed to run '{}': {}", bin_name, e)
                    };
                    return ToolResult {
                        tool: bin_name.to_string(),
                        success: false,
                        duration_ms: tool_start.elapsed().as_millis() as u64,
                        data: serde_json::Value::Null,
                        error: Some(msg),
                        suggested_fix: None,
                        auto_fix_available: None,
                    };
                }
            }
        }
    };

    let duration_ms = tool_start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let (data, error) = match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => (json, None),
        Err(_) => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    serde_json::Value::Null,
                    Some(format!("No output. stderr: {}", stderr.trim())),
                )
            } else {
                (serde_json::json!({ "raw": trimmed }), None)
            }
        }
    };

    ToolResult {
        tool: bin_name.to_string(),
        success: error.is_none() && output.status.success(),
        duration_ms,
        data,
        error,
        suggested_fix: None,
        auto_fix_available: None,
    }
}

fn run_batch(
    path: &str,
    _config: &str,
    format: &str,
    baseline: Option<&str>,
    no_fail_on_regression: bool,
) -> i32 {
    use cogent_common::*;

    use std::time::Instant;

    let start = Instant::now();

    // Initialize memory monitor (auto-terminates if memory exceeds safe threshold)
    let mut memory_monitor = MemoryMonitor::from_env();
    let mem_limit_mb = memory_monitor.max_rss_bytes / 1024 / 1024;
    let mem_display = if mem_limit_mb >= 1024 {
        format!("{:.1} GB", mem_limit_mb as f64 / 1024.0)
    } else {
        format!("{} MB", mem_limit_mb)
    };
    eprintln!(
        "  {} Cogent batch  ·  path: {}  ·  memory limit: {}",
        "▶".cyan().bold(),
        path.cyan(),
        mem_display.bright_black()
    );

    let tools: Vec<(&str, &str, Vec<&str>)> = vec![
        (
            "debt-scan",
            "debt",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "doc-coverage",
            "doccov",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "crap-metric",
            "crap",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("coupling", "coupling", vec![path, "--format", "json"]),
        ("risk-map", "riskmap", vec![path, "--format", "json"]),
        (
            "duplication",
            "dupfind",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "prop-cov",
            "propcov",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "taint-scan",
            "taint",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "fuzz-surface",
            "fuzz",
            vec!["--recursive", path, "--format", "json"],
        ),
        // mutation-test: run with capped mutants and enforced timeout.
        // Uses scratch workspace + watchdog kill — safe to include in batch.
        // Note: requires -p flag for package selection
        (
            "mutation-test",
            "mutate",
            vec![
                path,
                "-p",
                "ast-parse-ts",
                "--max-mutants",
                "5",
                "--timeout",
                "30",
                "--format",
                "json",
            ],
        ),
        (
            "line-length",
            "linelen",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "halstead",
            "halstead",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "secrets",
            "secrets",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "dead-code",
            "deadcode",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "cohesion",
            "cohesion",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "comment-ratio",
            "comments",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "error-handling",
            "errhandle",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "type-coverage",
            "typecov",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("vuln-scan", "vulnscan", vec![path, "--format", "json"]),
        (
            "sast",
            "sast",
            vec!["--recursive", path, "--format", "json"],
        ),
        (
            "crypto-check",
            "cryptocheck",
            vec!["--recursive", path, "--format", "json"],
        ),
        ("licenses", "licenses", vec![path, "--format", "json"]),
    ];

    // Run tools sequentially to prevent memory exhaustion
    // Previous concurrent execution (MAX_CONCURRENT=4) caused OOM crashes on 16GB/32GB systems
    let mut results: Vec<ToolResult> = Vec::new();
    let mut bar = Bar::new(tools.len());
    for (crate_name, bin_name, args) in &tools {
        bar.set_current(bin_name);

        // Check memory before starting tool
        if let Err(usage) = memory_monitor.check() {
            bar.finish();
            eprintln!(
                "  {} Memory limit exceeded before running {} ({} MB used). Stopping batch.",
                "✗".red().bold(),
                bin_name,
                usage.rss_bytes / 1024 / 1024
            );
            break;
        }

        let tool_start = Instant::now();
        let result = run_tool(crate_name, bin_name, args, tool_start);
        let duration_ms = result.duration_ms;
        let success = result.success;
        results.push(result);
        bar.advance(bin_name, success, duration_ms);

        // Check memory after tool completion
        if let Err(usage) = memory_monitor.check() {
            bar.finish();
            eprintln!(
                "  {} Memory limit exceeded after {} ({} MB used). Stopping batch.",
                "✗".red().bold(),
                bin_name,
                usage.rss_bytes / 1024 / 1024
            );
            break;
        }
    }
    bar.finish();

    let duration_ms = start.elapsed().as_millis() as u64;
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.len() - passed;

    // Baseline handling: must check before moving results into report
    let mut regression_detected = false;
    if let Some(baseline_file) = baseline {
        if let Ok(baseline_content) = std::fs::read_to_string(baseline_file) {
            if let Ok(baseline_report) = serde_json::from_str::<UnifiedReport>(&baseline_content) {
                let baseline_tools: std::collections::HashSet<String> = baseline_report
                    .tools
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let current_tools: std::collections::HashSet<String> = results
                    .iter()
                    .filter(|t| t.success)
                    .map(|t| t.tool.clone())
                    .collect();
                let regressed: Vec<String> =
                    baseline_tools.difference(&current_tools).cloned().collect();
                if !regressed.is_empty() {
                    eprintln!(
                        "BASELINE REGRESSION: previously-passing tools now failing: {:?}",
                        regressed
                    );
                    if !no_fail_on_regression {
                        regression_detected = true;
                    }
                }
            }
        }
    }

    match format {
        "sarif" => {
            // Build SARIF from results
            let mut log = SarifLog::new("cogent", env!("CARGO_PKG_VERSION"));
            let mut sarif_results: Vec<SarifResult> = Vec::new();

            for tool in &results {
                if !tool.success {
                    sarif_results.push(SarifResult {
                        rule_id: format!("{}-error", tool.tool),
                        rule_index: None,
                        level: "error".to_string(),
                        message: SarifMessage {
                            text: tool
                                .error
                                .clone()
                                .unwrap_or_else(|| format!("{} failed", tool.tool)),
                        },
                        locations: vec![SarifLocation {
                            physical_location: SarifPhysicalLocation {
                                artifact_location: Some(SarifArtifactLocation {
                                    uri: path.to_string(),
                                }),
                                region: None,
                            },
                        }],
                    });
                }
            }

            let run = sarif_run(
                "cogent-batch",
                env!("CARGO_PKG_VERSION"),
                sarif_results,
                if failed > 0 { 1 } else { 0 },
            );
            log.add_run(run);
            println!("{}", serde_json::to_string_pretty(&log).unwrap());
        }
        "json" => {
            let report = new_unified_report(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            );
            // Detect languages from source files at path
            let all_exts = [
                "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp",
                "cc", "cxx", "hpp", "cs", "java", "php", "rb", "swift",
            ];
            let mut langs_detected: Vec<String> = find_source_files(path, true, &all_exts)
                .iter()
                .map(|f| ast_parse_ts::Language::from_extension(f).to_string())
                .filter(|l| l != "unknown")
                .collect::<std::collections::HashSet<String>>()
                .into_iter()
                .collect();
            langs_detected.sort();
            let report = UnifiedReport {
                run_id: report.run_id,
                started_at: report.started_at,
                duration_ms,
                tools: results,
                summary: ReportSummary {
                    total_tools: tools.len(),
                    passed,
                    failed,
                    languages_detected: langs_detected,
                },
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        _ => {
            let all_ok = failed == 0;
            let summary_str = format!(
                "{}/{} tools passed  ·  {}",
                passed,
                results.len(),
                format_ms(duration_ms)
            );
            let summary_col = if all_ok {
                summary_str.green().to_string()
            } else {
                summary_str.red().to_string()
            };
            let inner = 46usize;
            let border = "═".repeat(inner + 2);
            eprintln!();
            eprintln!("  ╔{}╗", border);
            let title = format!(
                "COGENT RUN  ·  {}",
                if all_ok {
                    "PASSED ✓".green().bold().to_string()
                } else {
                    "FAILED ✗".red().bold().to_string()
                }
            );
            box_row(&title, inner);
            eprintln!("  ╠{}╣", border);
            box_row(&summary_col, inner);
            box_row(&format!("Path: {}", path), inner);
            eprintln!("  ╚{}╝", border);
            if !all_ok {
                eprintln!();
                for tool in results.iter().filter(|t| !t.success) {
                    let err = tool.error.as_deref().unwrap_or("check output for details");
                    eprintln!("  {} {}: {}", "✗".red(), tool.tool.red().bold(), err);
                }
            }
            eprintln!();
        }
    }

    if failed > 0 || regression_detected {
        1
    } else {
        0
    }
}

// ═══════════════════════════════════════════
// NDJSON OUTPUT
// ═══════════════════════════════════════════

fn output_ndjson(report: &CheckReport) {
    for check in &report.checks {
        let severity = check.severity.as_deref().unwrap_or("warning");
        let rule_id = check.rule_id.as_deref().unwrap_or(&check.name);
        let help = check.help.as_deref().unwrap_or("");
        if !check.passed {
            let items = check
                .details
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if items.is_empty() {
                println!(
                    "{}",
                    serde_json::json!({
                        "tool": check.name,
                        "severity": severity,
                        "rule_id": rule_id,
                        "message": check.message,
                        "help": help,
                        "file": report.path,
                        "line": null,
                        "col": null,
                    })
                );
            } else {
                for item in &items {
                    println!(
                        "{}",
                        serde_json::json!({
                            "tool": check.name,
                            "severity": severity,
                            "rule_id": rule_id,
                            "message": item.get("type").and_then(|v| v.as_str()).unwrap_or(&check.name),
                            "help": help,
                            "file": item.get("file"),
                            "line": item.get("line"),
                            "col": null,
                        })
                    );
                }
            }
        }
    }
}

// ═══════════════════════════════════════════
// SARIF OUTPUT
// ═══════════════════════════════════════════

fn output_sarif(report: &CheckReport) {
    let mut all_results: Vec<SarifResult> = Vec::new();
    let mut all_rules: Vec<SarifRule> = Vec::new();
    let mut rule_indices: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for check in &report.checks {
        for finding in &check.findings {
            let level = cogent_common::sarif_level(&finding.severity);
            let rule_id = finding.rule_id.clone();
            let idx = *rule_indices.entry(rule_id.clone()).or_insert_with(|| {
                let i = all_rules.len();
                all_rules.push(SarifRule {
                    id: rule_id.clone(),
                    name: Some(rule_id.clone()),
                    short_description: Some(SarifMessage {
                        text: finding.message.clone(),
                    }),
                    full_description: Some(SarifMessage {
                        text: finding.fix_hint.clone(),
                    }),
                    help: Some(SarifMessage {
                        text: finding.fix_hint.clone(),
                    }),
                    default_configuration: Some(SarifRuleConfig {
                        level: level.to_string(),
                    }),
                });
                i
            });
            let location = if !finding.file.is_empty() {
                vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: Some(SarifArtifactLocation {
                            uri: finding.file.clone(),
                        }),
                        region: finding.line.map(|l| SarifRegion {
                            start_line: Some(l as usize),
                            start_column: finding.column.map(|c| c as usize),
                            end_line: None,
                            end_column: None,
                        }),
                    },
                }]
            } else {
                Vec::new()
            };
            all_results.push(SarifResult {
                rule_id: finding.rule_id.clone(),
                rule_index: Some(idx),
                level: level.to_string(),
                message: SarifMessage {
                    text: finding.message.clone(),
                },
                locations: location,
            });
        }
    }

    let run = SarifRun {
        tool: SarifTool {
            driver: SarifDriver {
                name: "cogent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                rules: all_rules,
            },
        },
        invocations: Some(vec![SarifInvocation {
            execution_successful: report.passed,
            exit_code: Some(if report.passed { 0 } else { 1 }),
            end_time_utc: Some(chrono::Utc::now().to_rfc3339()),
        }]),
        results: all_results,
    };

    let log = SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![run],
    };
    println!("{}", serde_json::to_string_pretty(&log).unwrap());
}

// ═══════════════════════════════════════════
// JUNIT XML OUTPUT
// ═══════════════════════════════════════════

fn output_junit(report: &CheckReport) {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        r#"<testsuites name="Cogent" tests="{}" failures="{}" time="0">"#,
        report.summary.total_checks, report.summary.failed_checks
    ));
    xml.push('\n');

    for check in &report.checks {
        let test_count = check.findings.len().max(1);
        let failure_count = if check.passed { 0 } else { test_count };
        xml.push_str(&format!(
            r#"  <testsuite name="{}" tests="{}" failures="{}" errors="0" skipped="0">"#,
            html_escape(&check.name),
            test_count,
            failure_count
        ));
        xml.push('\n');

        if check.findings.is_empty() {
            xml.push_str(&format!(
                r#"    <testcase name="{}" classname="cogent.{}" />"#,
                html_escape(&check.name),
                html_escape(&check.name)
            ));
            xml.push('\n');
        } else {
            for finding in &check.findings {
                let case_name = html_escape(&finding.message);
                let classname = html_escape(&check.name);
                xml.push_str(&format!(
                    r#"    <testcase name="{}" classname="cogent.{}">"#,
                    case_name, classname
                ));
                xml.push('\n');
                if !check.passed {
                    xml.push_str(&format!(
                        r#"      <failure message="{}" type="{}">{} at {}:{}</failure>"#,
                        html_escape(&finding.message),
                        html_escape(&finding.rule_id),
                        html_escape(&finding.message),
                        html_escape(&finding.file),
                        finding.line.unwrap_or(0)
                    ));
                    xml.push('\n');
                }
                xml.push_str("    </testcase>");
                xml.push('\n');
            }
        }
        xml.push_str("  </testsuite>");
        xml.push('\n');
    }
    xml.push_str("</testsuites>");
    println!("{}", xml);
}

// ═══════════════════════════════════════════
// FINDINGS NDJSON OUTPUT
// ═══════════════════════════════════════════

fn output_findings_ndjson(report: &CheckReport) {
    for check in &report.checks {
        for finding in &check.findings {
            println!("{}", serde_json::to_string(finding).unwrap());
        }
    }
}

// ═══════════════════════════════════════════
// MARKDOWN OUTPUT
// ═══════════════════════════════════════════

fn output_markdown(report: &CheckReport, path: &str) {
    let security_tools = [
        "taint",
        "secrets",
        "sast",
        "crypto",
        "vulnscan",
        "access-control",
    ];
    let quality_tools = [
        "crap",
        "debt",
        "doc_coverage",
        "complexity",
        "duplication",
        "riskmap",
        "coupling",
        "propcov",
        "fuzz",
        "linelen",
        "halstead",
        "deadcode",
        "cohesion",
        "comments",
        "errhandle",
        "typecov",
    ];
    let compliance_tools = ["licenses", "outdated", "supply-chain"];
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let md = render_markdown_report(
        report,
        path,
        &date,
        &security_tools,
        &quality_tools,
        &compliance_tools,
    );
    println!("{}", md);
}

/// Generate a markdown snippet suitable for posting as a GitHub PR comment.
fn pr_comment_md(report: &CheckReport, path: &str) -> String {
    let (health, grade) = health_score(&report.checks);
    let overall = if report.passed {
        "✅ PASSED"
    } else {
        "❌ FAILED"
    };
    let mut md = format!(
        "## 🔍 Cogent Quality Check — {}\n\n**Status:** {}  \n**Health Score:** {}/100 (Grade {})  \n**Path:** {}\n\n",
        path, overall, health, grade, path
    );

    // Summary table
    md.push_str("| Metric | Value |\n|---|---|\n");
    md.push_str(&format!(
        "| Total Checks | {} |\n",
        report.summary.total_checks
    ));
    md.push_str(&format!("| Passed | {} |\n", report.summary.passed_checks));
    md.push_str(&format!("| Failed | {} |\n", report.summary.failed_checks));
    md.push('\n');

    // Failed checks collapsible sections
    let failed: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();
    if !failed.is_empty() {
        md.push_str("### ⚠️ Failed Checks\n\n");
        for check in failed {
            md.push_str(&format!(
                "<details>\n<summary><code>{}</code> — {}</summary>\n\n",
                check.name, check.message
            ));
            md.push_str("| File | Line | Message | Rule |\n|---|---|---|---|\n");
            for finding in &check.findings {
                let line = finding
                    .line
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!(
                    "| `{}` | {} | {} | `{}` |\n",
                    finding.file, line, finding.message, finding.rule_id
                ));
            }
            md.push_str("</details>\n\n");
        }
    }

    // File heatmap (top 10)
    if !report.file_summary.is_empty() {
        md.push_str("### 🔥 Top Files by Issue Count\n\n");
        md.push_str("| File | Issues | Severity Score |\n|---|---|---|\n");
        for fs in report.file_summary.iter().take(10) {
            md.push_str(&format!(
                "| `{}` | {} | {} |\n",
                fs.file, fs.issue_count, fs.severity_score
            ));
        }
        md.push('\n');
    }

    md.push_str("---\n\n*Generated by Cogent — automated code quality & security auditing.*\n");
    md
}

// ═══════════════════════════════════════════
// HISTORY
// ═══════════════════════════════════════════

fn history_command(
    action: &str,
    dir: &str,
    last: usize,
    report_path: Option<&str>,
    format: &str,
) -> i32 {
    match action {
        "record" => history_record(dir, report_path),
        "show" => {
            if format == "html" {
                history_html(dir, last)
            } else {
                history_show(dir, last)
            }
        }
        _ => history_show(dir, last),
    }
}

fn history_record(dir: &str, report_path: Option<&str>) -> i32 {
    use std::io::Read;

    let json_str = if let Some(path) = report_path {
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("history record: cannot read {}: {}", path, e);
                return 1;
            }
        }
    } else {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            eprintln!("history record: failed to read stdin");
            return 1;
        }
        buf
    };

    let report: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("history record: invalid JSON: {}", e);
            return 1;
        }
    };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = chrono_yymm(ts);

    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("history record: cannot create {}: {}", dir, e);
        return 1;
    }

    let path = format!("{}/{}.jsonl", dir, date);
    let tools_summary: serde_json::Value = report
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            let mut m = serde_json::Map::new();
            for t in arr {
                if let Some(name) = t.get("tool").and_then(|v| v.as_str()) {
                    m.insert(
                        name.to_string(),
                        serde_json::json!({
                            "success": t.get("success"),
                            "duration_ms": t.get("duration_ms"),
                        }),
                    );
                }
            }
            serde_json::Value::Object(m)
        })
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let record = serde_json::json!({
        "ts": ts,
        "run_id": report.get("run_id"),
        "passed": report.get("summary").and_then(|s| s.get("passed")),
        "failed": report.get("summary").and_then(|s| s.get("failed")),
        "tools": tools_summary,
    });

    let line = serde_json::to_string(&record).unwrap_or_default();
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", line)
        })
    {
        eprintln!("history record: write failed: {}", e);
        return 1;
    }

    eprintln!("history: recorded run to {}", path);
    0
}

fn history_show(dir: &str, last: usize) -> i32 {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            println!("No history found in {}", dir);
            return 0;
        }
    };

    let mut lines: Vec<String> = Vec::new();
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in &files {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for line in content.lines() {
                lines.push(line.to_string());
            }
        }
    }

    let show: Vec<&String> = lines.iter().rev().take(last).collect();
    if show.is_empty() {
        println!("No history records found.");
        return 0;
    }

    println!("\n{:<20} {:>6} {:>6}  TOOLS", "TIMESTAMP", "PASS", "FAIL");
    println!("{}", "─".repeat(70));
    for raw in show.iter().rev() {
        if let Ok(rec) = serde_json::from_str::<serde_json::Value>(raw) {
            let ts = rec.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
            let passed = rec.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
            let failed = rec.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
            let tools_str = rec
                .get("tools")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            let ok = v.get("success").and_then(|b| b.as_bool()).unwrap_or(false);
                            format!("{}:{}", k, if ok { "✓" } else { "✗" })
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                })
                .unwrap_or_default();
            println!(
                "{:<20} {:>6} {:>6}  {}",
                format_ts(ts),
                passed,
                failed,
                tools_str
            );
        }
    }
    println!();
    0
}

fn history_html(dir: &str, last: usize) -> i32 {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            println!("No history found in {}", dir);
            return 0;
        }
    };

    let mut records: Vec<(u64, u64, u64, u32)> = Vec::new();
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in &files {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for line in content.lines() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    let passed = json.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
                    let failed = json.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                    let ts = json.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                    let total = passed + failed;
                    let score = passed.checked_div(total).map_or(100, |r| (r * 100) as u32);
                    records.push((ts, passed, failed, score));
                }
            }
        }
    }

    records.sort_by_key(|(ts, _, _, _)| *ts);
    let mut recent: Vec<_> = records.into_iter().rev().take(last).collect();
    recent.reverse();

    if recent.is_empty() {
        println!("No history records found.");
        return 0;
    }

    let scores: Vec<u32> = recent.iter().map(|(_, _, _, s)| *s).collect();
    let sparkline = sparkline_svg(&scores, 600, 120);
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let mut table_rows = String::new();
    for (ts, passed, failed, score) in &recent {
        let trend = if recent.len() > 1 {
            let idx = recent
                .iter()
                .position(|(t, _, _, _)| *t == *ts)
                .unwrap_or(0);
            if idx > 0 {
                let prev = recent[idx - 1].3;
                if *score > prev {
                    "<span style=\"color:#22c55e\">↑</span>"
                } else if *score < prev {
                    "<span style=\"color:#ef4444\">↓</span>"
                } else {
                    "<span style=\"color:#9ca3af\">→</span>"
                }
            } else {
                ""
            }
        } else {
            ""
        };
        table_rows.push_str(&format!(
            "<tr style=\"border-bottom:1px solid #f3f4f6\"><td style=\"padding:10px 14px\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}</td><td style=\"padding:10px 14px;text-align:center;color:#22c55e\">{}</td><td style=\"padding:10px 14px;text-align:center;color:#ef4444\">{}</td><td style=\"padding:10px 14px;text-align:center\">{}/100 {}</td></tr>",
            format_ts(*ts), recent.iter().position(|(t, _, _, _)| *t == *ts).map(|i| i + 1).unwrap_or(0), passed, failed, score, trend
        ));
    }

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cogent History Trend</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;line-height:1.5;padding:40px;max-width:900px;margin:0 auto}}
.card{{background:#fff;border-radius:12px;padding:28px;box-shadow:0 1px 3px rgba(0,0,0,.06);margin-bottom:28px;border:1px solid #f1f5f9}}
.card-title{{font-size:15px;font-weight:700;color:#0f172a;margin-bottom:16px}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
th{{padding:8px 14px;text-align:center;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;border-bottom:2px solid #e5e7eb}}
.footer{{font-size:12px;color:#94a3b8;text-align:center;margin-top:48px}}
</style>
</head>
<body>
<h1 style="font-size:22px;font-weight:800;margin-bottom:8px">Cogent History Trend</h1>
<p style="color:#6b7280;font-size:13px;margin-bottom:28px">{} &nbsp;&middot;&nbsp; {} runs</p>

<div class="card">
  <div class="card-title">Health Score Over Time</div>
  {}
</div>

<div class="card">
  <div class="card-title">Run Details</div>
  <table>
    <thead><tr><th style="text-align:left">Date</th><th>#</th><th>Passed</th><th>Failed</th><th>Score</th></tr></thead>
    <tbody>{}</tbody>
  </table>
</div>

<div class="footer">Generated by Cogent — {}</div>
</body>
</html>"##,
        dir,
        recent.len(),
        sparkline,
        table_rows,
        date
    );
    println!("{}", html);
    0
}

fn chrono_yymm(ts: u64) -> String {
    let secs = ts % (365 * 24 * 3600);
    let _ = secs;
    let d = std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts);
    if let Ok(dur) = d.duration_since(std::time::UNIX_EPOCH) {
        let days = dur.as_secs() / 86400;
        let year = 1970 + days / 365;
        let month = (days % 365) / 30 + 1;
        return format!("{}-{:02}", year, month);
    }
    "unknown".to_string()
}

fn format_ts(ts: u64) -> String {
    let days = ts / 86400;
    let year = 1970 + days / 365;
    let month = (days % 365) / 30 + 1;
    let day = (days % 365) % 30 + 1;
    let h = (ts % 86400) / 3600;
    let m = (ts % 3600) / 60;
    format!("{}-{:02}-{:02} {:02}:{:02}", year, month, day, h, m)
}

// ═══════════════════════════════════════════
// HOOKS
// ═══════════════════════════════════════════

fn install_hooks(repo: &str, fast: bool) -> i32 {
    let profile = detect_project(repo);
    install_hooks_impl(repo, fast, &profile)
}

fn install_hooks_impl(repo: &str, fast: bool, profile: &ProjectProfile) -> i32 {
    let hook_dir = format!("{}/.git/hooks", repo);
    let hook_path = format!("{}/pre-commit", hook_dir);

    if !std::path::Path::new(&hook_dir).exists() {
        eprintln!(
            "install-hooks: {} is not a git repository (no .git/hooks directory)",
            repo
        );
        return 1;
    }

    if std::path::Path::new(&hook_path).exists() {
        eprintln!(
            "install-hooks: hook already exists at {} -- remove it first or use uninstall-hooks",
            hook_path
        );
        return 1;
    }

    let hook_script = build_hook_script(fast, profile);

    match std::fs::write(&hook_path, hook_script) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("install-hooks: write failed: {}", e);
            return 1;
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).ok();
    }

    println!("Installed pre-commit hook at {}", hook_path);
    if fast {
        println!("Mode: fast (metrics only, no tests)");
    } else {
        println!(
            "Mode: full (runs tests + coverage for {} before checking)",
            profile.ecosystem
        );
    }
    println!("To bypass: git commit --no-verify");
    println!("To remove: cogent uninstall-hooks {}", repo);
    0
}

fn build_hook_script(fast: bool, profile: &ProjectProfile) -> String {
    let cm_bin = r#"CM_BIN=""
if command -v cogent &>/dev/null; then
    CM_BIN="cogent"
elif [ -f target/release/cogent ]; then
    CM_BIN="./target/release/cogent"
else
    echo "cogent: binary not found, skipping pre-commit check" >&2
    exit 0
fi"#;

    if fast || !profile.is_coverage_available() {
        format!(
            r#"#!/usr/bin/env bash
# Cogent pre-commit hook (fast/metrics-only) — installed by `cogent install-hooks`
# Remove with: cogent uninstall-hooks
set -euo pipefail

{cm_bin}

$CM_BIN check . --format text
"#,
            cm_bin = cm_bin
        )
    } else {
        let test_cmd = profile.test_cmd.join(" ");
        let cov_cmd = profile.coverage_cmd.join(" ");
        let lcov_flag = if !profile.lcov_path.is_empty() {
            format!("--coverage {}", profile.lcov_path)
        } else {
            String::new()
        };
        format!(
            r#"#!/usr/bin/env bash
# Cogent pre-commit hook (full: tests + coverage + metrics) — installed by `cogent install-hooks`
# Remove with: cogent uninstall-hooks
# To skip: git commit --no-verify
set -euo pipefail

{cm_bin}

echo "[cogent] Running tests ({ecosystem})..."
{test_cmd}

echo "[cogent] Collecting coverage..."
{cov_cmd}

echo "[cogent] Running quality checks..."
$CM_BIN check . {lcov_flag} --format text
"#,
            cm_bin = cm_bin,
            ecosystem = profile.ecosystem,
            test_cmd = test_cmd,
            cov_cmd = cov_cmd,
            lcov_flag = lcov_flag,
        )
    }
}

fn uninstall_hooks(repo: &str) -> i32 {
    let hook_path = format!("{}/.git/hooks/pre-commit", repo);

    if !std::path::Path::new(&hook_path).exists() {
        eprintln!("uninstall-hooks: no pre-commit hook found at {}", hook_path);
        return 1;
    }

    let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
    if !content.contains("Cogent pre-commit hook") {
        eprintln!(
            "uninstall-hooks: {} exists but was not installed by cogent — refusing to remove",
            hook_path
        );
        return 1;
    }

    match std::fs::remove_file(&hook_path) {
        Ok(_) => {
            println!("Removed pre-commit hook from {}", hook_path);
            0
        }
        Err(e) => {
            eprintln!("uninstall-hooks: remove failed: {}", e);
            1
        }
    }
}

// ═══════════════════════════════════════════
// WATCH MODE
// ═══════════════════════════════════════════

fn watch_mode(path: &str, checks: &str, debounce_ms: u64, no_tests: bool, full: bool) -> i32 {
    use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let profile = detect_project(path);
    let check_list: Vec<String> = checks.split(',').map(|s| s.trim().to_lowercase()).collect();

    println!("cogent watch: watching {} ({})", path, profile.ecosystem);
    if full {
        println!("  checks: ALL (--full mode)");
    } else {
        println!("  checks: {}", check_list.join(", "));
    }
    println!(
        "  watching extensions: .{}",
        profile.watch_extensions.join(", .")
    );
    if no_tests || !profile.is_coverage_available() {
        println!("  mode: metrics-only (no test runner)");
    } else {
        println!("  mode: full (tests + coverage + metrics)");
        println!("  test cmd: {}", profile.test_cmd.join(" "));
        println!("  coverage cmd: {}", profile.coverage_cmd.join(" "));
    }
    println!("  debounce: {}ms", debounce_ms);
    println!("  Press Ctrl+C to stop.\n");

    // Tracks previous cycle results for diff display: name → passed
    let mut prev_results: Vec<(String, bool)> = Vec::new();

    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watch: failed to create watcher: {}", e);
            return 1;
        }
    };

    if let Err(e) = watcher.watch(std::path::Path::new(path), RecursiveMode::Recursive) {
        eprintln!("watch: failed to watch {}: {}", path, e);
        return 1;
    }

    let debounce = Duration::from_millis(debounce_ms);
    let mut last_run: Option<Instant> = None;
    let mut debounce_printed = false;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                let is_watched = event.paths.iter().any(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|ext| profile.watch_extensions.iter().any(|w| w == ext))
                });
                if !is_watched {
                    continue;
                }

                let now = Instant::now();
                let should_run = last_run.map_or(true, |t| now.duration_since(t) >= debounce);
                if !should_run {
                    if !debounce_printed {
                        eprintln!("  {} debouncing ({}ms)…", "⏳".bright_black(), debounce_ms);
                        debounce_printed = true;
                    }
                    continue;
                }
                debounce_printed = false;
                if should_run {
                    last_run = Some(now);
                    let changed: Vec<_> = event
                        .paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect();
                    let ts = {
                        let secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        format!(
                            "{:02}:{:02}:{:02}",
                            (secs / 3600) % 24,
                            (secs / 60) % 60,
                            secs % 60
                        )
                    };
                    let cycle_start = Instant::now();
                    eprintln!(
                        "\n  {} File changed: {}",
                        ts.bright_black(),
                        changed.join(", ").cyan()
                    );
                    let new_results = run_watch_checks(path, &check_list, no_tests, &profile, full);
                    print_cycle_diff(&prev_results, &new_results);
                    prev_results = new_results;
                    eprintln!(
                        "  {} Cycle complete  ({})",
                        "◉".bright_black(),
                        format_elapsed(cycle_start.elapsed()).bright_black()
                    );
                    eprintln!("  {} Watching for changes…", "◉".bright_black());
                }
            }
            Ok(Err(e)) => {
                eprintln!("watch error: {}", e);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    0
}

/// Run the test suite, then coverage, then metrics.
/// Returns the path to the lcov file if coverage succeeded.
fn run_tests_and_coverage(profile: &ProjectProfile) -> Option<String> {
    use std::process::Command;

    // Run tests
    if profile.test_cmd.is_empty() {
        return None;
    }
    let (test_bin, test_args) = profile.test_cmd.split_first()?;
    let cmd_str = profile.test_cmd.join(" ");
    let test_out = run_with_spinner(&format!("tests  {}", cmd_str.bright_black()), || {
        Command::new(test_bin).args(test_args).output()
    });
    match test_out {
        Ok(o) if o.status.success() => {
            eprintln!("  {} tests passed", "✓".green().bold());
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("  {} tests FAILED:", "✗".red().bold());
            for line in stderr.lines().take(15) {
                eprintln!("    {}", line);
            }
            return None;
        }
        Err(e) => {
            eprintln!("  {} Could not run test command: {}", "✗".red().bold(), e);
            return None;
        }
    }

    // Run coverage
    if !profile.is_coverage_available() || profile.lcov_path.is_empty() {
        return None;
    }
    let (cov_bin, cov_args) = profile.coverage_cmd.split_first()?;
    let cov_cmd_str = profile.coverage_cmd.join(" ");
    let cov_out = run_with_spinner(&format!("coverage  {}", cov_cmd_str.bright_black()), || {
        Command::new(cov_bin).args(cov_args).output()
    });
    match cov_out {
        Ok(o) if o.status.success() => {
            if std::path::Path::new(&profile.lcov_path).exists() {
                eprintln!(
                    "  {} coverage → {}",
                    "✓".green().bold(),
                    profile.lcov_path.cyan()
                );
                Some(profile.lcov_path.clone())
            } else {
                eprintln!(
                    "  {} coverage command succeeded but {} not found",
                    "!".yellow().bold(),
                    profile.lcov_path
                );
                None
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("  {} coverage failed:", "✗".red().bold());
            for line in stderr.lines().take(8) {
                eprintln!("    {}", line);
            }
            None
        }
        Err(e) => {
            eprintln!(
                "  {} Could not run coverage command: {}",
                "✗".red().bold(),
                e
            );
            None
        }
    }
}

fn run_watch_checks(
    path: &str,
    check_list: &[String],
    no_tests: bool,
    profile: &ProjectProfile,
    full: bool,
) -> Vec<(String, bool)> {
    let should = |name: &str| check_list.iter().any(|c| c == name);

    // Optionally run tests + coverage and get lcov path for CRAP
    let lcov_path: Option<String> = if no_tests || !profile.is_coverage_available() {
        None
    } else {
        run_tests_and_coverage(profile)
    };
    let coverage_opt = lcov_path.as_deref();

    // In --full mode run all checks (same set as `cogent check`)
    let mut results: Vec<(String, bool, String)> = Vec::new();

    macro_rules! wpush {
        ($name:expr, $r:expr) => {{
            let r = $r;
            results.push(($name.to_string(), r.passed, r.message));
        }};
    }

    if full {
        let cov_owned = coverage_opt.map(|s| s.to_string());
        wpush!("debt", check_debt(path, true, profile.max_debt));
        wpush!("doc", check_doc_coverage(path, true, profile.min_doc));
        wpush!("crap", check_crap(path, true, &cov_owned, profile.max_crap));
        wpush!(
            "complexity",
            check_complexity(path, true, 10, profile.max_complexity_violations)
        );
        wpush!("taint", check_taint(path, true, 0));
        wpush!("errhandle", check_errhandle(path, true, 50));
        wpush!("secrets", check_secrets(path, true, 0));
        wpush!("deadcode", check_deadcode(path, true, 10));
        wpush!("linelen", check_linelen(path, true, 0));
    } else {
        if should("debt") {
            wpush!("debt", check_debt(path, true, profile.max_debt));
        }
        if should("doc") {
            wpush!("doc", check_doc_coverage(path, true, profile.min_doc));
        }
        if should("crap") {
            let cov_owned = coverage_opt.map(|s| s.to_string());
            wpush!("crap", check_crap(path, true, &cov_owned, profile.max_crap));
        }
        if should("complexity") {
            wpush!(
                "complexity",
                check_complexity(path, true, 10, profile.max_complexity_violations)
            );
        }
        if should("taint") {
            wpush!("taint", check_taint(path, true, 0));
        }
        if should("errhandle") {
            wpush!("errhandle", check_errhandle(path, true, 50));
        }
        if should("secrets") {
            wpush!("secrets", check_secrets(path, true, 0));
        }
        if should("deadcode") {
            wpush!("deadcode", check_deadcode(path, true, 10));
        }
        if should("linelen") {
            wpush!("linelen", check_linelen(path, true, 0));
        }
    }

    let all_passed = results.iter().all(|(_, p, _)| *p);
    eprintln!();
    for (name, passed, msg) in &results {
        let icon = if *passed {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };
        let name_col = if *passed { name.normal() } else { name.red() };
        let msg_col = if *passed {
            msg.bright_black()
        } else {
            msg.red()
        };
        eprintln!("  {} {:<15}  {}", icon, name_col, msg_col);
    }
    if coverage_opt.is_some() {
        eprintln!(
            "  {} using coverage from {}",
            "ℹ".cyan(),
            profile.lcov_path.bright_black()
        );
    }
    let overall = if all_passed {
        "ALL CHECKS PASS".green().bold()
    } else {
        "SOME CHECKS FAILED".red().bold()
    };
    eprintln!("  {}", overall);

    results.into_iter().map(|(n, p, _)| (n, p)).collect()
}

/// Print a diff line if any checks changed pass/fail state since the last cycle.
fn print_cycle_diff(prev: &[(String, bool)], curr: &[(String, bool)]) {
    if prev.is_empty() {
        return;
    }
    let mut regressions: Vec<&str> = Vec::new();
    let mut fixes: Vec<&str> = Vec::new();
    for (name, passed) in curr {
        let prev_passed = prev.iter().find(|(n, _)| n == name).map(|(_, p)| *p);
        match prev_passed {
            Some(true) if !passed => regressions.push(name),
            Some(false) if *passed => fixes.push(name),
            _ => {}
        }
    }
    if regressions.is_empty() && fixes.is_empty() {
        return;
    }
    eprintln!("  {} Cycle diff:", "△".yellow());
    for name in &fixes {
        eprintln!("    {} {} now passing", "↑".green().bold(), name.green());
    }
    for name in &regressions {
        eprintln!("    {} {} now failing", "↓".red().bold(), name.red());
    }
}

// ═══════════════════════════════════════════

fn skipped_tool_check(
    name: &str,
    rule_id: &str,
    threshold: usize,
    error: Option<String>,
) -> CheckResult {
    CheckResult {
        name: name.into(),
        passed: true,
        score: None,
        threshold: Some(threshold as f64),
        message: match error {
            Some(error) if !error.is_empty() => format!("Skipped: {}", error),
            _ => "Skipped: tool unavailable or produced no JSON output".into(),
        },
        details: serde_json::Value::Null,
        severity: Some("info".into()),
        help: Some(
            "Install the check tool or run from the Cogent workspace to enable this check.".into(),
        ),
        findings: Vec::new(),
        rule_id: Some(rule_id.into()),
    }
}

fn check_access_control(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let max_str = format!("{}", max_violations);
    args.push("--max-violations");
    args.push(&max_str);
    let res = run_tool("access-control", "access-control", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check(
            "access-control",
            "access-control",
            max_violations,
            res.error.clone(),
        );
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_violations;
    let msg = if passed {
        format!(
            "Access control passed ({} findings, {} critical)",
            total, critical
        )
    } else {
        format!(
            "Access control failed ({} findings > {} threshold, {} critical)",
            total, max_violations, critical
        )
    };
    CheckResult {
        name: "access-control".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: msg,
        details: serde_json::json!({"findings": total, "critical": critical}),
        severity: if critical > 0 {
            Some("critical".into())
        } else {
            Some("high".into())
        },
        help: Some(
            "Review missing auth guards, hardcoded credentials, IAM policies, and CORS settings."
                .into(),
        ),
        findings: extract_findings_from_details(&res.data, "access-control", "high"),
        rule_id: Some("access-control".into()),
    }
}

fn check_supply_chain(path: &str, max_risks: usize) -> CheckResult {
    let max_str = format!("{}", max_risks);
    let args = vec![path, "--format", "json", "--max-risks", &max_str];
    let res = run_tool("supply-chain", "supply-chain", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("supply-chain", "supply-chain", max_risks, res.error.clone());
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_risks"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_risks;
    let msg = if passed {
        format!(
            "Supply chain passed ({} risks, {} critical)",
            total, critical
        )
    } else {
        format!(
            "Supply chain failed ({} risks > {} threshold, {} critical)",
            total, max_risks, critical
        )
    };
    CheckResult {
        name: "supply-chain".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_risks as f64),
        message: msg,
        details: serde_json::json!({"risks": total, "critical": critical}),
        severity: if critical > 0 { Some("critical".into()) } else { Some("high".into()) },
        help: Some("Review lockfile integrity, typosquatting, unpinned dependencies, and abandoned packages.".into()),
        findings: extract_findings_from_details(&res.data, "supply-chain", "high"),
        rule_id: Some("supply-chain".into()),
    }
}

// NEW 6 CHECK WRAPPERS & SETUP
// ═══════════════════════════════════════════

fn check_taint(path: &str, recursive: bool, max_taint: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("taint-scan", "taint", &args, Instant::now());
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = violations <= max_taint;
    CheckResult {
        name: "taint".into(),
        passed,
        score: Some(violations as f64),
        threshold: Some(max_taint as f64),
        message: if passed {
            format!("{} taint violations <= {}", violations, max_taint)
        } else {
            format!("{} taint violations > allowed {}", violations, max_taint)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "taint_limit", "high"),
        rule_id: Some("taint_limit".into()),
    }
}

fn check_dupfind(path: &str, recursive: bool, max_duplication: f64) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("duplication", "dupfind", &args, Instant::now());
    let groups = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_groups"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = groups <= max_duplication;
    CheckResult {
        name: "duplication".into(),
        passed,
        score: Some(groups),
        threshold: Some(max_duplication),
        message: if passed {
            format!("{} duplicated groups <= {}", groups, max_duplication)
        } else {
            format!("{} duplicated groups > allowed {}", groups, max_duplication)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "duplication_limit", "medium"),
        rule_id: Some("duplication_limit".into()),
    }
}

fn check_riskmap(path: &str, _recursive: bool, max_risk: f64) -> CheckResult {
    let args = vec![path, "--format", "json"];
    // riskmap doesn't use recursive flag, it always scans dir
    let res = run_tool("risk-map", "riskmap", &args, Instant::now());
    let max_found_risk = res
        .data
        .get("files")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("risk_score").and_then(|v| v.as_f64()))
                .fold(0.0f64, f64::max)
        })
        .unwrap_or(0.0);
    let passed = max_found_risk <= max_risk;
    CheckResult {
        name: "riskmap".into(),
        passed,
        score: Some(max_found_risk),
        threshold: Some(max_risk),
        message: if passed {
            format!("Max risk score {:.1} <= {:.1}", max_found_risk, max_risk)
        } else {
            format!(
                "Max risk score {:.1} > allowed {:.1}",
                max_found_risk, max_risk
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "riskmap_limit", "high"),
        rule_id: Some("riskmap_limit".into()),
    }
}

fn check_coupling(path: &str, max_coupling: usize) -> CheckResult {
    let args = vec![path, "--format", "json"];
    let res = run_tool("coupling", "coupling", &args, Instant::now());
    let avg_fan_out = res
        .data
        .get("summary")
        .and_then(|s| s.get("avg_fan_out"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = avg_fan_out <= max_coupling as f64;
    CheckResult {
        name: "coupling".into(),
        passed,
        score: Some(avg_fan_out),
        threshold: Some(max_coupling as f64),
        message: if passed {
            format!("Avg fan-out {:.1} <= {}", avg_fan_out, max_coupling)
        } else {
            format!("Avg fan-out {:.1} > allowed {}", avg_fan_out, max_coupling)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "coupling_limit", "medium"),
        rule_id: Some("coupling_limit".into()),
    }
}

fn check_propcov(path: &str, recursive: bool, min_propcov: f64) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("prop-cov", "propcov", &args, Instant::now());
    let coverage = res
        .data
        .get("summary")
        .and_then(|s| s.get("coverage_percentage"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = coverage >= min_propcov;
    CheckResult {
        name: "propcov".into(),
        passed,
        score: Some(coverage),
        threshold: Some(min_propcov),
        message: if passed {
            format!("PropCov {:.1}% >= {:.1}%", coverage, min_propcov)
        } else {
            format!("PropCov {:.1}% < required {:.1}%", coverage, min_propcov)
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "propcov_limit", "high"),
        rule_id: Some("propcov_limit".into()),
    }
}

fn check_fuzz(path: &str, recursive: bool, max_fuzz_risk: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("fuzz-surface", "fuzz", &args, Instant::now());
    let fuzzable = res
        .data
        .get("summary")
        .and_then(|s| s.get("fuzzable_functions"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = fuzzable <= max_fuzz_risk;
    CheckResult {
        name: "fuzz".into(),
        passed,
        score: Some(fuzzable as f64),
        threshold: Some(max_fuzz_risk as f64),
        message: if passed {
            format!("{} fuzzable endpoints <= {}", fuzzable, max_fuzz_risk)
        } else {
            format!(
                "{} fuzzable endpoints > allowed {}",
                fuzzable, max_fuzz_risk
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: None,
        findings: extract_findings_from_details(&res.data, "fuzz_limit", "high"),
        rule_id: Some("fuzz_limit".into()),
    }
}

fn check_linelen(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("line-length", "linelen", &args, Instant::now());
    let fn_viols = res
        .data
        .get("summary")
        .and_then(|s| s.get("fn_violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let file_viols = res
        .data
        .get("summary")
        .and_then(|s| s.get("file_violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = fn_viols + file_viols;
    let passed = total <= max_violations;
    CheckResult {
        name: "linelen".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if total == 0 {
                "All functions and files within size limits".to_string()
            } else {
                format!("{} violations <= allowed {}", total, max_violations)
            }
        } else {
            format!(
                "{} line-length violations > allowed {}",
                total, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some("Functions should be <= 40 lines; files should be <= 500 lines.".into()),
        findings: extract_findings_from_details(&res.data, "linelen_limit", "warning"),
        rule_id: Some("linelen_limit".into()),
    }
}

fn check_halstead(path: &str, recursive: bool, max_bugs: f64) -> CheckResult {
    let max_bugs_str = format!("{}", max_bugs);
    let mut args = vec![path, "--format", "json", "--max-bugs", &max_bugs_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("halstead", "halstead", &args, Instant::now());
    let exceeding = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_exceeding_bugs_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total_bugs = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_bugs_estimated"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = exceeding == 0;
    CheckResult {
        name: "halstead".into(),
        passed,
        score: Some(total_bugs),
        threshold: Some(max_bugs),
        message: if passed {
            format!(
                "Halstead bugs estimated {:.2} (no file exceeds {:.1})",
                total_bugs.max(0.0),
                max_bugs
            )
        } else {
            format!(
                "{} files exceed Halstead bugs threshold of {:.1}",
                exceeding, max_bugs
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some(
            "Halstead bugs = Volume/3000. High values indicate complex, error-prone code.".into(),
        ),
        findings: extract_findings_from_details(&res.data, "halstead_bugs", "warning"),
        rule_id: Some("halstead_bugs".into()),
    }
}

fn check_secrets(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("secrets", "secrets", &args, Instant::now());
    let findings = res
        .data
        .get("summary")
        .and_then(|s| s.get("findings_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = findings <= max_violations;
    CheckResult {
        name: "secrets".into(),
        passed,
        score: Some(findings as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if findings == 0 {
                "No hardcoded secrets detected".into()
            } else {
                format!("{} secret findings <= allowed {}", findings, max_violations)
            }
        } else {
            format!(
                "{} hardcoded secret findings > allowed {}",
                findings, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: Some("Move secrets to environment variables or a secrets manager.".into()),
        findings: extract_findings_from_details(&res.data, "secrets_limit", "high"),
        rule_id: Some("secrets_limit".into()),
    }
}

fn check_deadcode(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("dead-code", "deadcode", &args, Instant::now());
    let findings = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = findings <= max_violations;
    CheckResult {
        name: "deadcode".into(),
        passed,
        score: Some(findings as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if findings == 0 {
                "No dead code patterns detected".into()
            } else {
                format!(
                    "{} dead code findings <= allowed {}",
                    findings, max_violations
                )
            }
        } else {
            format!(
                "{} dead code findings > allowed {}",
                findings, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some(
            "Remove unused imports, #[allow(dead_code)] suppressions, and dead assignments.".into(),
        ),
        findings: extract_findings_from_details(&res.data, "deadcode_limit", "warning"),
        rule_id: Some("deadcode_limit".into()),
    }
}

fn check_sast(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("sast", "sast", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("sast", "sast_limit", max_findings, res.error.clone());
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let high = res
        .data
        .get("summary")
        .and_then(|s| s.get("high"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_findings;
    CheckResult {
        name: "sast".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_findings as f64),
        message: if passed {
            if total == 0 { "No SAST findings (SQL injection, XSS, path traversal, cmd injection)".into() }
            else { format!("{} SAST findings <= allowed {}", total, max_findings) }
        } else {
            format!("{} SAST findings ({} critical, {} high) — exceeds threshold of {}", total, critical, high, max_findings)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Review SAST findings. Parameterize SQL, sanitize input, use allowlists for file paths and commands.".into()),
        findings: extract_findings_from_details(&res.data, "sast_limit", "high"),
        rule_id: Some("sast_limit".into()),
    }
}

fn check_crypto(path: &str, recursive: bool, max_findings: usize) -> CheckResult {
    let max_str = format!("{}", max_findings);
    let mut args = vec![path, "--format", "json", "--max-findings", &max_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("crypto-check", "cryptocheck", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("crypto", "crypto_limit", max_findings, res.error.clone());
    }
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_findings;
    CheckResult {
        name: "crypto".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_findings as f64),
        message: if passed {
            if total == 0 { "No cryptographic issues (weak hash, insecure random, ECB, disabled TLS)".into() }
            else { format!("{} crypto findings <= allowed {}", total, max_findings) }
        } else {
            format!("{} crypto findings ({} critical) — exceeds threshold of {}", total, critical, max_findings)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Replace MD5/SHA1 with SHA-256. Use OsRng for security randomness. Use AES-GCM, not ECB.".into()),
        findings: extract_findings_from_details(&res.data, "crypto_limit", "high"),
        rule_id: Some("crypto_limit".into()),
    }
}

fn check_licenses(path: &str, max_violations: usize) -> CheckResult {
    let max_str = format!("{}", max_violations);
    let args = vec![path, "--format", "json", "--max-violations", &max_str];
    let res = run_tool("licenses", "licenses", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check(
            "licenses",
            "license_compliance",
            max_violations,
            res.error.clone(),
        );
    }
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("packages_scanned"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = violations <= max_violations;
    CheckResult {
        name: "licenses".into(),
        passed,
        score: Some(violations as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if violations == 0 { format!("No license violations in {} packages scanned", total) }
            else { format!("{} license violations <= allowed {} ({} packages)", violations, max_violations, total) }
        } else {
            format!("{} license violations — GPL/AGPL packages in deny list", violations)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("high".into()) },
        help: Some("Review copyleft (GPL/AGPL) licenses. They may require open-sourcing your code. Consult legal counsel.".into()),
        findings: extract_findings_from_details(&res.data, "license_compliance", "high"),
        rule_id: Some("license_compliance".into()),
    }
}

fn check_outdated(path: &str, max_major_behind: usize) -> CheckResult {
    use std::process::Command;
    // cargo-outdated must be installed; gracefully skip if not present
    let available = Command::new("cargo")
        .args(["outdated", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !available {
        return CheckResult {
            name: "outdated".into(),
            passed: true,
            score: None,
            threshold: None,
            message: "Skipped: cargo-outdated not installed (cargo install cargo-outdated)".into(),
            details: serde_json::Value::Null,
            severity: Some("info".into()),
            help: Some("Install with: cargo install cargo-outdated".into()),
            rule_id: Some("dep_freshness".into()),
            findings: Vec::new(),
        };
    }

    let output = Command::new("cargo")
        .args(["outdated", "--format", "json", "--root-deps-only"])
        .current_dir(path)
        .output();

    let major_behind = match output {
        Ok(ref o) if o.status.success() => {
            let json: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_default();
            json.get("dependencies")
                .and_then(|d| d.as_array())
                .map(|deps| {
                    deps.iter()
                        .filter(|dep| {
                            let latest = dep.get("latest").and_then(|v| v.as_str()).unwrap_or("");
                            let current = dep.get("project").and_then(|v| v.as_str()).unwrap_or("");
                            // Count as major-behind if first semver segment differs
                            let lat_major = latest
                                .split('.')
                                .next()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            let cur_major = current
                                .split('.')
                                .next()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            lat_major > cur_major
                        })
                        .count()
                })
                .unwrap_or(0)
        }
        _ => 0,
    };

    let passed = major_behind <= max_major_behind;
    CheckResult {
        name: "outdated".into(),
        passed,
        score: Some(major_behind as f64),
        threshold: Some(max_major_behind as f64),
        findings: Vec::new(),
        message: if major_behind == 0 {
            "All direct dependencies are within one major version".into()
        } else {
            format!(
                "{} direct dependencies are 1+ major versions behind latest",
                major_behind
            )
        },
        details: serde_json::Value::Null,
        severity: if passed {
            Some("info".into())
        } else {
            Some("low".into())
        },
        help: Some(
            "Run `cargo update` or review Cargo.toml to upgrade outdated dependencies.".into(),
        ),
        rule_id: Some("dep_freshness".into()),
    }
}

fn check_typecov(path: &str, recursive: bool, min_pct: f64) -> CheckResult {
    let min_pct_str = format!("{}", min_pct);
    let mut args = vec![path, "--format", "json", "--min-pct", &min_pct_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("type-coverage", "typecov", &args, Instant::now());
    let overall = res
        .data
        .get("summary")
        .and_then(|s| s.get("overall_coverage_pct"))
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let below = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_below_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = below == 0;
    CheckResult {
        name: "typecov".into(),
        passed,
        score: Some(overall),
        threshold: Some(min_pct),
        message: if passed {
            format!("Type coverage {:.1}% >= {:.0}%", overall, min_pct)
        } else {
            format!(
                "{} files below type coverage threshold of {:.0}%",
                below, min_pct
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: Some(
            "Add type annotations to Python/JS/TS functions for better maintainability.".into(),
        ),
        findings: extract_findings_from_details(&res.data, "typecov_limit", "medium"),
        rule_id: Some("typecov_limit".into()),
    }
}

fn check_vulnscan(path: &str, max_critical: usize, max_high: usize) -> CheckResult {
    let max_critical_str = format!("{}", max_critical);
    let max_high_str = format!("{}", max_high);
    let args = vec![
        path,
        "--format",
        "json",
        "--max-critical",
        &max_critical_str,
        "--max-high",
        &max_high_str,
    ];
    let res = run_tool("vuln-scan", "vulnscan", &args, Instant::now());
    if res.data.is_null() {
        return skipped_tool_check("vulnscan", "vuln_limit", max_critical, res.error.clone());
    }
    let critical = res
        .data
        .get("summary")
        .and_then(|s| s.get("critical"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let high = res
        .data
        .get("summary")
        .and_then(|s| s.get("high"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = critical <= max_critical && high <= max_high;
    CheckResult {
        name: "vulnscan".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_critical as f64),
        message: if passed {
            if total == 0 {
                "No known vulnerabilities".into()
            } else {
                format!(
                    "{} vulnerabilities ({} critical, {} high) within allowed thresholds",
                    total, critical, high
                )
            }
        } else {
            format!(
                "{} critical + {} high CVEs exceed allowed thresholds ({}/{})",
                critical, high, max_critical, max_high
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("high".into())
        },
        help: Some(
            "Update vulnerable dependencies. Run cargo audit / npm audit for details.".into(),
        ),
        findings: extract_findings_from_details(&res.data, "vuln_limit", "high"),
        rule_id: Some("vuln_limit".into()),
    }
}

fn check_cohesion(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("cohesion", "cohesion", &args, Instant::now());
    let violations = res
        .data
        .get("summary")
        .and_then(|s| s.get("violations"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let avg_lcom = res
        .data
        .get("summary")
        .and_then(|s| s.get("avg_lcom"))
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let passed = violations <= max_violations;
    CheckResult {
        name: "cohesion".into(),
        passed,
        score: Some(avg_lcom),
        threshold: Some(max_violations as f64),
        message: if passed {
            if violations == 0 {
                format!("All structs cohesive (avg LCOM4 {:.2})", avg_lcom)
            } else {
                format!(
                    "{} cohesion violations <= allowed {} (avg LCOM4 {:.2})",
                    violations, max_violations, avg_lcom
                )
            }
        } else {
            format!(
                "{} structs exceed LCOM4 threshold of {}",
                violations, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("warning".into())
        },
        help: Some("High LCOM4 means a struct does too many unrelated things. Split it.".into()),
        findings: extract_findings_from_details(&res.data, "cohesion_lcom4", "warning"),
        rule_id: Some("cohesion_lcom4".into()),
    }
}

fn check_comments(path: &str, recursive: bool, min_ratio: f64) -> CheckResult {
    let min_ratio_str = format!("{}", min_ratio);
    let mut args = vec![path, "--format", "json", "--min-ratio", &min_ratio_str];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("comment-ratio", "comments", &args, Instant::now());
    let below = res
        .data
        .get("summary")
        .and_then(|s| s.get("files_below_threshold"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let overall = res
        .data
        .get("summary")
        .and_then(|s| s.get("overall_comment_ratio"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = below == 0;
    CheckResult {
        name: "comments".into(),
        passed,
        score: Some(overall * 100.0),
        threshold: Some(min_ratio * 100.0),
        message: if passed {
            format!("Overall comment ratio {:.1}% >= {:.0}%", overall * 100.0, min_ratio * 100.0)
        } else {
            format!("{} files below comment ratio threshold of {:.0}%", below, min_ratio * 100.0)
        },
        details: res.data.clone(),
        severity: if passed { Some("info".into()) } else { Some("low".into()) },
        help: Some("Add inline comments explaining non-obvious logic. Doc comments are tracked separately by doccov.".into()),
        findings: extract_findings_from_details(&res.data, "comment_ratio", "low"),
        rule_id: Some("comment_ratio".into()),
    }
}

fn check_errhandle(path: &str, recursive: bool, max_violations: usize) -> CheckResult {
    let mut args = vec![path, "--format", "json"];
    if recursive {
        args.push("--recursive");
    }
    let res = run_tool("error-handling", "errhandle", &args, Instant::now());
    let total = res
        .data
        .get("summary")
        .and_then(|s| s.get("total_findings"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let passed = total <= max_violations;
    CheckResult {
        name: "errhandle".into(),
        passed,
        score: Some(total as f64),
        threshold: Some(max_violations as f64),
        message: if passed {
            if total == 0 {
                "No error handling issues detected".into()
            } else {
                format!(
                    "{} error handling findings <= allowed {}",
                    total, max_violations
                )
            }
        } else {
            format!(
                "{} error handling violations > allowed {}",
                total, max_violations
            )
        },
        details: res.data.clone(),
        severity: if passed {
            Some("info".into())
        } else {
            Some("medium".into())
        },
        help: Some(
            "Replace .unwrap()/.expect() with proper error propagation using `?` or match.".into(),
        ),
        findings: extract_findings_from_details(&res.data, "errhandle_limit", "medium"),
        rule_id: Some("errhandle_limit".into()),
    }
}

fn diff_command(before_path: &str, after_path: &str, format: &str) -> i32 {
    let load = |p: &str| -> Option<CheckReport> {
        let content = std::fs::read_to_string(p)
            .map_err(|e| eprintln!("Error reading {}: {}", p, e))
            .ok()?;
        serde_json::from_str(&content)
            .map_err(|e| eprintln!("Error parsing {}: {}", p, e))
            .ok()
    };

    let before = match load(before_path) {
        Some(r) => r,
        None => return 2,
    };
    let after = match load(after_path) {
        Some(r) => r,
        None => return 2,
    };

    let mut regressions: Vec<&str> = Vec::new();
    let mut fixes: Vec<&str> = Vec::new();
    let mut unchanged_pass = 0usize;
    let mut unchanged_fail = 0usize;

    for ac in &after.checks {
        if let Some(bc) = before.checks.iter().find(|b| b.name == ac.name) {
            match (bc.passed, ac.passed) {
                (true, false) => regressions.push(&ac.name),
                (false, true) => fixes.push(&ac.name),
                (true, true) => unchanged_pass += 1,
                (false, false) => unchanged_fail += 1,
            }
        }
    }

    let new_checks: Vec<&str> = after
        .checks
        .iter()
        .filter(|ac| !before.checks.iter().any(|bc| bc.name == ac.name))
        .map(|ac| ac.name.as_str())
        .collect();

    if format == "html" {
        let html = diff_html(
            &before,
            &after,
            before_path,
            after_path,
            &regressions,
            &fixes,
            &new_checks,
            unchanged_pass,
            unchanged_fail,
        );
        println!("{}", html);
        return if regressions.is_empty() { 0 } else { 1 };
    }

    eprintln!();
    eprintln!(
        "  {} {} → {}",
        "diff".bright_black(),
        before_path.cyan(),
        after_path.cyan()
    );
    eprintln!();

    if regressions.is_empty() && fixes.is_empty() && new_checks.is_empty() {
        eprintln!(
            "  {} No changes — {} pass, {} fail (unchanged)",
            "◉".bright_black(),
            unchanged_pass,
            unchanged_fail
        );
    } else {
        for name in &fixes {
            eprintln!(
                "  {} {} {}",
                "↑".green().bold(),
                name.green().bold(),
                "now passing".green()
            );
        }
        for name in &regressions {
            eprintln!(
                "  {} {} {}",
                "↓".red().bold(),
                name.red().bold(),
                "now failing".red()
            );
        }
        for name in &new_checks {
            let status = after
                .checks
                .iter()
                .find(|c| c.name == *name)
                .map(|c| c.passed)
                .unwrap_or(false);
            let icon = if status {
                "✓".green().bold().to_string()
            } else {
                "✗".red().bold().to_string()
            };
            eprintln!("  {} {} {}", icon, name, "(new check)".bright_black());
        }
        eprintln!();
        if unchanged_pass > 0 || unchanged_fail > 0 {
            eprintln!(
                "  {} {} unchanged passing, {} unchanged failing",
                "◉".bright_black(),
                unchanged_pass,
                unchanged_fail
            );
        }
    }

    let score_before = {
        let (s, g) = health_score(&before.checks);
        format!("{}/100 ({})", s, g)
    };
    let score_after = {
        let (s, g) = health_score(&after.checks);
        format!("{}/100 ({})", s, g)
    };
    eprintln!();
    eprintln!(
        "  {} Health: {} → {}",
        "▶".cyan(),
        score_before.bright_black(),
        score_after.cyan().bold()
    );
    eprintln!();

    if regressions.is_empty() {
        0
    } else {
        1
    }
}

#[allow(clippy::too_many_arguments)]
fn diff_html(
    before: &CheckReport,
    after: &CheckReport,
    before_path: &str,
    after_path: &str,
    regressions: &[&str],
    fixes: &[&str],
    new_checks: &[&str],
    unchanged_pass: usize,
    unchanged_fail: usize,
) -> String {
    let (health_before, grade_before) = health_score(&before.checks);
    let (health_after, grade_after) = health_score(&after.checks);
    let date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let mut rows = String::new();
    for ac in &after.checks {
        let bc = before.checks.iter().find(|b| b.name == ac.name);
        let change = match bc {
            Some(b) => {
                if !b.passed && ac.passed {
                    "<span style=\"color:#22c55e;font-weight:700\">↑ FIXED</span>"
                } else if b.passed && !ac.passed {
                    "<span style=\"color:#ef4444;font-weight:700\">↓ REGRESSION</span>"
                } else {
                    "<span style=\"color:#9ca3af\">—</span>"
                }
            }
            None => "<span style=\"color:#6366f1;font-weight:700\">NEW</span>",
        };
        let status = if ac.passed {
            "<span style=\"color:#22c55e\">✓ PASS</span>"
        } else {
            "<span style=\"color:#ef4444\">✗ FAIL</span>"
        };
        rows.push_str(&format!(
            "<tr style=\"border-bottom:1px solid #f3f4f6\"><td style=\"padding:10px 14px;font-weight:600\">{}</td><td style=\"padding:10px 14px\">{}</td><td style=\"padding:10px 14px\">{}</td></tr>",
            html_escape(&ac.name), status, change
        ));
    }

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cogent Diff — {} → {}</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;line-height:1.5;padding:40px;max-width:900px;margin:0 auto}}
.card{{background:#fff;border-radius:12px;padding:28px;box-shadow:0 1px 3px rgba(0,0,0,.06);margin-bottom:28px;border:1px solid #f1f5f9}}
.card-title{{font-size:15px;font-weight:700;color:#0f172a;margin-bottom:16px}}
.score-grid{{display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-bottom:28px}}
.score-card{{background:#fff;border-radius:12px;padding:24px;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.06)}}
.score-card .n{{font-size:36px;font-weight:900}}
.score-card .lbl{{font-size:11px;color:#94a3b8;margin-top:6px;text-transform:uppercase}}
.arrow{{text-align:center;font-size:28px;color:#64748b;margin:12px 0}}
table{{width:100%;border-collapse:collapse;font-size:13px}}
th{{padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;border-bottom:2px solid #e5e7eb}}
.footer{{font-size:12px;color:#94a3b8;text-align:center;margin-top:48px}}
</style>
</head>
<body>
<h1 style="font-size:22px;font-weight:800;margin-bottom:8px">Cogent Diff Report</h1>
<p style="color:#6b7280;font-size:13px;margin-bottom:28px">{} &nbsp;&middot;&nbsp; {} → {}</p>

<div class="score-grid">
  <div class="score-card">
    <div class="n" style="color:#1e293b">{}/100</div>
    <div class="lbl">Before — Grade {}</div>
  </div>
  <div class="score-card">
    <div class="n" style="color:#1e293b">{}/100</div>
    <div class="lbl">After — Grade {}</div>
  </div>
</div>

<div class="card">
  <div class="card-title">Summary</div>
  <p style="font-size:13px;color:#475569;line-height:1.6">
    {} regression(s), {} fix(es), {} new check(s).<br>
    {} unchanged passing, {} unchanged failing.
  </p>
</div>

<div class="card">
  <div class="card-title">Per-Check Comparison</div>
  <table>
    <thead><tr><th>Check</th><th>Status</th><th>Change</th></tr></thead>
    <tbody>{}</tbody>
  </table>
</div>

<div class="footer">Generated by Cogent — {}</div>
</body>
</html>"##,
        html_escape(before_path),
        html_escape(after_path),
        date,
        html_escape(before_path),
        html_escape(after_path),
        health_before,
        grade_before,
        health_after,
        grade_after,
        regressions.len(),
        fixes.len(),
        new_checks.len(),
        unchanged_pass,
        unchanged_fail,
        rows,
        date
    )
}

fn serve_command(port: u16, history_dir: &str) {
    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr).expect("Failed to start HTTP server");
    eprintln!(
        "  {} Cogent serve running at http://{}",
        "▶".cyan().bold(),
        addr.cyan()
    );
    eprintln!("  {} Press Ctrl+C to stop", "ℹ".cyan());

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let response = match url.as_str() {
            "/" => {
                let mut body = String::new();
                body.push_str("<!DOCTYPE html><html><head><meta charset='UTF-8'><meta http-equiv='refresh' content='30'><title>Cogent Reports</title><style>");
                body.push_str("body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f1f5f9;color:#1e293b;padding:40px;max-width:800px;margin:0 auto}");
                body.push_str("h1{font-size:24px;font-weight:800;margin-bottom:24px}");
                body.push_str("ul{list-style:none;padding:0}");
                body.push_str("li{border-bottom:1px solid #e2e8f0;padding:12px 0;display:flex;justify-content:space-between;align-items:center}");
                body.push_str("a{color:#6366f1;text-decoration:none;font-weight:600}");
                body.push_str("a:hover{text-decoration:underline}");
                body.push_str(".meta{font-size:12px;color:#94a3b8}");
                body.push_str(".nav{margin-bottom:20px}");
                body.push_str(".nav a{margin-right:16px;font-size:13px}");
                body.push_str("</style></head><body>");
                body.push_str("<div class='nav'><a href='/latest'>Latest Report</a><a href='/api/latest'>API (JSON)</a></div>");
                body.push_str("<h1>Cogent Reports</h1><ul>");

                if let Ok(entries) = std::fs::read_dir(history_dir) {
                    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                    files.sort_by_key(|a| std::cmp::Reverse(a.file_name()));
                    for entry in files.iter().take(50) {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.ends_with(".json") {
                            let path = format!("/report/{}", name);
                            body.push_str(&format!(
                                "<li><a href='{}'>{}</a><span class='meta'>{} bytes</span></li>",
                                path,
                                name,
                                entry.metadata().map(|m| m.len()).unwrap_or(0)
                            ));
                        }
                    }
                }
                body.push_str("</ul></body></html>");
                tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    )
                    .unwrap(),
                )
            }
            "/latest" => {
                let candidates = ["cogent-report.html", "check-report.html"];
                let mut served = false;
                let mut content = String::new();
                for cand in &candidates {
                    if let Ok(html) = std::fs::read_to_string(cand) {
                        content = html;
                        served = true;
                        break;
                    }
                }
                if served {
                    tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"text/html; charset=utf-8"[..],
                        )
                        .unwrap(),
                    )
                } else {
                    tiny_http::Response::from_string(
                        "No latest report found. Run cogent check --ci to generate one.",
                    )
                    .with_status_code(tiny_http::StatusCode(404))
                }
            }
            "/api/latest" => {
                let candidates = ["cogent-summary.json", "check-report.json"];
                let mut served = false;
                let mut content = String::new();
                for cand in &candidates {
                    if let Ok(json) = std::fs::read_to_string(cand) {
                        content = json;
                        served = true;
                        break;
                    }
                }
                if served {
                    tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    )
                } else {
                    tiny_http::Response::from_string(r##"{"error":"No latest report found. Run cogent check --ci to generate one."}"##).with_status_code(tiny_http::StatusCode(404)).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                }
            }
            path if path.starts_with("/report/") => {
                let file_name = &path[8..];
                let file_path = std::path::Path::new(history_dir).join(file_name);
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => tiny_http::Response::from_string(content).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    ),
                    Err(_) => tiny_http::Response::from_string("Not found")
                        .with_status_code(tiny_http::StatusCode(404)),
                }
            }
            _ => tiny_http::Response::from_string("Not found")
                .with_status_code(tiny_http::StatusCode(404)),
        };
        let _ = request.respond(response);
    }
}

fn open_in_browser(path: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", path])
        .spawn();
}

fn report_command(
    path: &str,
    format: &str,
    output: Option<&str>,
    project: Option<&str>,
    from_json: Option<&str>,
    skip: Option<&str>,
    open: bool,
) -> i32 {
    // --- 1. Gather check data ---
    let check_report: CheckReport = if let Some(json_path) = from_json {
        let content = match std::fs::read_to_string(json_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading {}: {}", json_path, e);
                return 2;
            }
        };
        match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error parsing JSON: {}", e);
                return 2;
            }
        }
    } else {
        // Run cogent check internally
        eprintln!("  {} Running checks…", "·".bright_black());
        let mut args = vec![path, "--format", "json"];
        if let Some(s) = skip {
            args.push("--skip");
            args.push(s);
        }
        let output_bytes =
            std::process::Command::new(std::env::current_exe().unwrap_or_else(|_| "cogent".into()))
                .arg("check")
                .args(&args)
                .output();
        match output_bytes {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                match serde_json::from_str(&stdout) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to parse check output: {}", e);
                        return 2;
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to run checks: {}", e);
                return 2;
            }
        }
    };

    let project_name = project
        .map(|s| s.to_string())
        .or_else(|| {
            std::fs::canonicalize(path)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        })
        .unwrap_or_else(|| path.to_string());

    let now = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let hh = (secs % 86400) / 3600;
        let mm = (secs % 3600) / 60;
        // Correct Gregorian calendar from Unix timestamp
        let mut days = (secs / 86400) as i64;
        let mut year = 1970i64;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let y_days = if leap { 366 } else { 365 };
            if days < y_days {
                break;
            }
            days -= y_days;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let month_days = [
            31i64,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut month = 0usize;
        for &md in &month_days {
            if days < md {
                break;
            }
            days -= md;
            month += 1;
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            year,
            month + 1,
            days + 1,
            hh,
            mm
        )
    };

    let passed = check_report.passed;
    let total = check_report.summary.total_checks;
    let passed_n = check_report.summary.passed_checks;
    let failed_n = check_report.summary.failed_checks;

    // Categorise checks by domain
    let security_tools = [
        "secrets",
        "vulnscan",
        "taint",
        "errhandle",
        "sast",
        "crypto",
    ];
    let compliance_tools = ["licenses", "sbom"];
    let quality_tools = [
        "crap",
        "debt",
        "doc_coverage",
        "complexity",
        "duplication",
        "cohesion",
        "coupling",
        "riskmap",
        "linelen",
        "halstead",
        "deadcode",
        "comments",
        "propcov",
        "fuzz",
        "typecov",
    ];

    match format {
        "markdown" | "md" => {
            let md = render_markdown_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
            );
            let out_path = output.unwrap_or("cogent-report.md");
            std::fs::write(out_path, &md).expect("Failed to write report");
            eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
            if open {
                open_in_browser(out_path);
            }
        }
        "pdf" => {
            // Render HTML first, then convert via headless Chrome/Chromium
            let html = render_html_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
            );
            let html_tmp = "/tmp/cogent-report-tmp.html";
            std::fs::write(html_tmp, &html).expect("Failed to write temp HTML");
            let pdf_path = output.unwrap_or("cogent-report.pdf");
            let browser = [
                "chromium",
                "chromium-browser",
                "google-chrome",
                "google-chrome-stable",
            ]
            .iter()
            .find(|b| {
                std::process::Command::new(b)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            })
            .copied();
            match browser {
                Some(bin) => {
                    let abs_html = std::fs::canonicalize(html_tmp)
                        .map(|p| format!("file://{}", p.display()))
                        .unwrap_or_else(|_| format!("file://{}", html_tmp));
                    let result = std::process::Command::new(bin)
                        .args([
                            "--headless",
                            "--disable-gpu",
                            "--no-sandbox",
                            &format!("--print-to-pdf={}", pdf_path),
                            &abs_html,
                        ])
                        .output();
                    match result {
                        Ok(o) if o.status.success() => {
                            eprintln!("  {} Report written to {}", "✓".green().bold(), pdf_path);
                            if open {
                                open_in_browser(pdf_path);
                            }
                        }
                        Ok(o) => {
                            let err = String::from_utf8_lossy(&o.stderr);
                            eprintln!(
                                "  {} PDF conversion failed: {}",
                                "✗".red().bold(),
                                err.lines().next().unwrap_or("unknown error")
                            );
                            eprintln!("  {} HTML saved to {}", "ℹ".cyan(), html_tmp);
                        }
                        Err(e) => eprintln!("  {} Could not run {}: {}", "✗".red().bold(), bin, e),
                    }
                }
                None => {
                    eprintln!(
                        "  {} No Chromium/Chrome found — falling back to HTML",
                        "!".yellow().bold()
                    );
                    let out_path = output.unwrap_or("cogent-report.html");
                    std::fs::write(out_path, &html).expect("Failed to write HTML report");
                    eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
                    if open {
                        open_in_browser(out_path);
                    }
                }
            }
        }
        _ => {
            let html = render_html_report(
                &check_report,
                &project_name,
                &now,
                &security_tools,
                &quality_tools,
                &compliance_tools,
            );
            let out_path = output.unwrap_or("cogent-report.html");
            std::fs::write(out_path, &html).expect("Failed to write report");
            eprintln!("  {} Report written to {}", "✓".green().bold(), out_path);
            eprintln!(
                "  {} {} checks: {}/{} passed",
                if passed {
                    "✓".green().bold()
                } else {
                    "✗".red().bold()
                },
                total,
                passed_n,
                total
            );
            if open {
                open_in_browser(out_path);
            }
        }
    }

    let _ = (passed_n, failed_n);
    if passed {
        0
    } else {
        1
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn severity_color_html(sev: &str) -> &'static str {
    match sev {
        "high" | "critical" | "error" => "#ef4444",
        "medium" | "warning" => "#f59e0b",
        "low" => "#3b82f6",
        _ => "#6b7280",
    }
}

fn severity_badge(sev: &str) -> String {
    let color = severity_color_html(sev);
    format!(
        r#"<span style="background:{c};color:#fff;padding:2px 8px;border-radius:12px;font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:.03em">{s}</span>"#,
        c = color,
        s = sev
    )
}

/// Build a collapsible offender list from CheckResult.details JSON
fn offender_rows_html(c: &CheckResult) -> String {
    let arrays = [
        "items",
        "functions",
        "findings",
        "violations",
        "secrets",
        "duplicates",
    ];
    for key in &arrays {
        if let Some(arr) = c.details.get(key).and_then(|v| v.as_array()) {
            if arr.is_empty() {
                continue;
            }
            let mut rows = String::new();
            for item in arr.iter().take(10) {
                let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("");
                let line = item
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .map(|l| format!(":{}", l))
                    .unwrap_or_default();
                let desc = item
                    .get("context")
                    .or_else(|| item.get("kind"))
                    .or_else(|| item.get("name"))
                    .or_else(|| item.get("type"))
                    .or_else(|| item.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let loc = if file.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", file, line)
                };
                let desc_trunc = if desc.len() > 80 {
                    format!("{}…", &desc[..80])
                } else {
                    desc.to_string()
                };
                rows.push_str(&format!(
                    r#"<div style="display:flex;gap:12px;padding:4px 0;border-bottom:1px solid #f3f4f6;font-size:12px">
  <span style="color:#6366f1;font-family:monospace;white-space:nowrap;min-width:180px">{}</span>
  <span style="color:#6b7280">{}</span>
</div>"#,
                    html_escape(&loc), html_escape(&desc_trunc)
                ));
            }
            let more = if arr.len() > 10 {
                format!(
                    r#"<div style="font-size:12px;color:#9ca3af;padding-top:6px">… {} more findings</div>"#,
                    arr.len() - 10
                )
            } else {
                String::new()
            };
            return format!(
                r#"<details style="margin-top:8px">
<summary style="font-size:12px;color:#6366f1;cursor:pointer;user-select:none;padding:4px 0">▶ Show {} finding{}</summary>
<div style="margin-top:8px;padding:8px 12px;background:#f9fafb;border-radius:6px;border-left:3px solid #6366f1">
{}{}
</div>
</details>"#,
                arr.len().min(10),
                if arr.len() == 1 { "" } else { "s" },
                rows,
                more
            );
        }
    }
    String::new()
}

fn check_row_html(c: &CheckResult) -> String {
    let icon = if c.passed { "&#10003;" } else { "&#10007;" };
    let icon_color = if c.passed { "#22c55e" } else { "#ef4444" };
    let row_bg = if c.passed { "#fff" } else { "#fef2f2" };
    let name_color = if c.passed { "#111827" } else { "#ef4444" };
    let sev = c.severity.as_deref().unwrap_or("info");
    let help = c.help.as_deref().unwrap_or("");
    let score_str = match (c.score, c.threshold) {
        (Some(s), Some(t)) => format!("{:.1} / {:.1}", s, t),
        (Some(s), None) => format!("{:.1}", s),
        _ => "&#8212;".to_string(),
    };
    let offenders = if !c.passed {
        offender_rows_html(c)
    } else {
        String::new()
    };
    let msg_cell = format!(
        "<div style=\"font-size:13px;color:#374151\">{msg}</div><div style=\"font-size:11px;color:#9ca3af;margin-top:3px\">{help}</div>{off}",
        msg = html_escape(&c.message), help = html_escape(help), off = offenders
    );
    format!(
        "<tr style=\"background:{rb};border-bottom:1px solid #f3f4f6;vertical-align:top\">\n  <td style=\"padding:12px 14px;font-size:18px;color:{ic};text-align:center;width:40px;font-weight:700\">{icon}</td>\n  <td style=\"padding:12px 14px;font-weight:600;font-size:13px;color:{nc};white-space:nowrap\">{name}</td>\n  <td style=\"padding:12px 14px\">{mc}</td>\n  <td style=\"padding:12px 14px;font-size:12px;color:#6b7280;white-space:nowrap\">{score}</td>\n  <td style=\"padding:12px 14px;white-space:nowrap\">{sb}</td>\n</tr>",
        rb = row_bg, ic = icon_color, icon = icon, nc = name_color,
        name = c.name, mc = msg_cell, score = score_str, sb = severity_badge(sev),
    )
}

/// SVG donut ring showing pass percentage. r=44 → circumference≈276.
fn donut_svg(pct: f64, color: &str) -> String {
    let circ = 276.46f64;
    let dash = circ * pct / 100.0;
    let gap = circ - dash;
    let pct_int = pct as u32;
    // Build without format! to avoid Rust 2021 prefixed-literal issues with HTML
    let mut s = String::from(
        r#"<svg viewBox="0 0 100 100" width="120" height="120" style="display:block">"#,
    );
    s.push_str("\n  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" stroke=\"#e5e7eb\" stroke-width=\"10\"/>\n");
    s.push_str(&format!(
        "  <circle cx=\"50\" cy=\"50\" r=\"44\" fill=\"none\" stroke=\"{}\" stroke-width=\"10\"\n",
        color
    ));
    s.push_str(&format!(
        "    stroke-dasharray=\"{:.2} {:.2}\" stroke-dashoffset=\"69.12\"\n",
        dash, gap
    ));
    s.push_str("    stroke-linecap=\"round\" transform=\"rotate(-90 50 50)\"/>\n");
    s.push_str(&format!("  <text x=\"50\" y=\"46\" text-anchor=\"middle\" font-size=\"18\" font-weight=\"800\" fill=\"{}\" font-family=\"system-ui\">{}%</text>\n", color, pct_int));
    s.push_str("  <text x=\"50\" y=\"60\" text-anchor=\"middle\" font-size=\"9\" fill=\"#9ca3af\" font-family=\"system-ui\">pass rate</text>\n");
    s.push_str("</svg>");
    s
}

/// Inline horizontal mini-bar for a category (e.g. "6/8 ██████░░")
fn mini_bar(pass: usize, total: usize, color: &str) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (pass * 12) / total;
    let bar: String = "█".repeat(filled) + &"░".repeat(12 - filled);
    let pct = pass * 100 / total;
    format!(
        "<div style=\"display:flex;align-items:center;gap:8px;font-size:12px\">\n  <span style=\"font-family:monospace;color:{color};letter-spacing:.1em\">{bar}</span>\n  <span style=\"color:#6b7280\">{pass}/{total} ({pct}%)</span>\n</div>",
        color = color, bar = bar, pass = pass, total = total, pct = pct
    )
}

/// Semi-circle gauge SVG for health score 0-100.
fn gauge_svg(score: u32, color: &str) -> String {
    let radius = 80f64;
    let cx = 100f64;
    let cy = 100f64;
    let start_angle = std::f64::consts::PI;
    let _end_angle = 0f64;
    let angle = start_angle - (score as f64 / 100.0) * start_angle;
    let needle_x = cx + radius * angle.cos();
    let needle_y = cy - radius * angle.sin();
    let mut s = String::from(
        r##"<svg viewBox="0 0 200 110" width="200" height="110" style="display:block">"##,
    );
    // Background arc
    s.push_str(&format!(
        r##"<path d="M {} {} A {} {} 0 0 1 {} {}" fill="none" stroke="#e5e7eb" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, cx + radius, cy
    ));
    // Foreground arc
    let large_arc = if score > 50 { 1 } else { 0 };
    s.push_str(&format!(
        r##"<path d="M {} {} A {} {} 0 {} 1 {} {}" fill="none" stroke="{}" stroke-width="12" stroke-linecap="round"/>"##,
        cx - radius, cy, radius, radius, large_arc, needle_x, needle_y, color
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="28" font-weight="800" fill="{}" font-family="system-ui">{}</text>"##,
        cx, cy + 8.0, color, score
    ));
    s.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" font-size="10" fill="#9ca3af" font-family="system-ui">Health Score</text>"##,
        cx, cy + 24.0
    ));
    s.push_str("</svg>");
    s
}

/// Horizontal bar chart SVG for severity distribution.
fn severity_bar_chart_svg(counts: &[(String, usize, &str)]) -> String {
    let total: usize = counts.iter().map(|(_, c, _)| *c).sum();
    if total == 0 {
        return String::new();
    }
    let bar_h = 16usize;
    let gap = 4usize;
    let max_w = 280f64;
    let max_c = counts.iter().map(|(_, c, _)| *c).max().unwrap_or(1).max(1);
    let h = counts.len() * (bar_h + gap) + gap;
    let mut s = format!(
        r##"<svg viewBox="0 0 320 {}" width="320" height="{}" style="display:block">"##,
        h, h
    );
    for (i, (label, count, color)) in counts.iter().enumerate() {
        let y = gap + i * (bar_h + gap);
        let w = (*count as f64 / max_c as f64) * max_w;
        s.push_str(&format!(
            r##"<rect x="40" y="{}" width="{}" height="{}" rx="3" fill="{}"/>"##,
            y,
            w.max(2.0),
            bar_h,
            color
        ));
        s.push_str(&format!(
            r##"<text x="35" y="{}" text-anchor="end" font-size="11" fill="#6b7280" font-family="system-ui" dy="{}">{} ({})</text>"##,
            y + bar_h / 2 + 4, 0, html_escape(label), count
        ));
    }
    s.push_str("</svg>");
    s
}

/// Sparkline SVG from a series of health scores.
fn sparkline_svg(scores: &[u32], width: usize, height: usize) -> String {
    if scores.len() < 2 {
        return String::new();
    }
    let max_score = scores.iter().copied().max().unwrap_or(100).max(1);
    let min_score = scores.iter().copied().min().unwrap_or(0);
    let range = (max_score - min_score).max(1) as f64;
    let n = scores.len();
    let step_x = width as f64 / (n - 1) as f64;
    let padding = 4f64;
    let plot_h = (height as f64) - padding * 2.0;

    let mut points = String::new();
    for (i, &score) in scores.iter().enumerate() {
        let x = i as f64 * step_x;
        let y = padding + plot_h - ((score - min_score) as f64 / range) * plot_h;
        if i > 0 {
            points.push(' ');
        }
        points.push_str(&format!("{:.1},{:.1}", x, y));
    }

    let mut s = format!(
        r##"<svg viewBox="0 0 {} {}" width="{}" height="{}" style="display:block">"##,
        width, height, width, height
    );
    // Grid line at 50%
    let y50 = padding + plot_h * 0.5;
    s.push_str(&format!(
        r##"<line x1="0" y1="{}" x2="{}" y2="{}" stroke="#e2e8f0" stroke-width="1" stroke-dasharray="3,3"/>"##,
        y50, width, y50
    ));
    // Polyline
    s.push_str(&format!(
        r##"<polyline points="{}" fill="none" stroke="#6366f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##,
        points
    ));
    // Dots
    for (i, &score) in scores.iter().enumerate() {
        let x = i as f64 * step_x;
        let y = padding + plot_h - ((score - min_score) as f64 / range) * plot_h;
        s.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="2.5" fill="#6366f1"/>"##,
            x, y
        ));
    }
    s.push_str("</svg>");
    s
}

/// Read `.cogent-history/` and return last N health scores.
fn history_health_scores(dir: &str, last: usize) -> Vec<u32> {
    let mut scores: Vec<(u64, u32)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".jsonl") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    for line in content.lines() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                            let passed = json.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
                            let failed = json.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                            let ts = json.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                            let total = passed + failed;
                            let score = passed.checked_div(total).map_or(100, |r| (r * 100) as u32);
                            scores.push((ts, score));
                        }
                    }
                }
            }
        }
    }
    scores.sort_by_key(|(ts, _)| *ts);
    let mut result: Vec<u32> = scores
        .into_iter()
        .map(|(_, s)| s)
        .rev()
        .take(last)
        .collect();
    result.reverse();
    result
}

fn render_html_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
) -> String {
    let (health, grade) = health_score(&report.checks);
    let overall_color = if report.passed { "#22c55e" } else { "#ef4444" };
    let overall_label = if report.passed { "PASSED" } else { "FAILED" };
    let pct = if report.summary.total_checks == 0 {
        100.0
    } else {
        report.summary.passed_checks as f64 / report.summary.total_checks as f64 * 100.0
    };
    let grade_color = match grade {
        'A' => "#22c55e",
        'B' => "#06b6d4",
        'C' => "#f59e0b",
        _ => "#ef4444",
    };

    // Split checks by category
    let mut sec_checks: Vec<&CheckResult> = Vec::new();
    let mut qual_checks: Vec<&CheckResult> = Vec::new();
    let mut comp_checks: Vec<&CheckResult> = Vec::new();
    let mut other_checks: Vec<&CheckResult> = Vec::new();
    for c in &report.checks {
        if security_tools.contains(&c.name.as_str()) {
            sec_checks.push(c);
        } else if compliance_tools.contains(&c.name.as_str()) {
            comp_checks.push(c);
        } else if quality_tools.contains(&c.name.as_str()) {
            qual_checks.push(c);
        } else {
            other_checks.push(c);
        }
    }
    qual_checks.extend(other_checks);

    // Category pass counts
    let sec_pass = sec_checks.iter().filter(|c| c.passed).count();
    let qual_pass = qual_checks.iter().filter(|c| c.passed).count();
    let comp_pass = comp_checks.iter().filter(|c| c.passed).count();
    let sec_col = if sec_pass == sec_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };
    let qual_col = if qual_pass == qual_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };
    let comp_col = if comp_pass == comp_checks.len() {
        "#22c55e"
    } else {
        "#ef4444"
    };

    let failed_checks: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();

    // ── Executive summary ────────────────────────────────────
    let risk_domain = if sec_checks.iter().any(|c| !c.passed) {
        "security"
    } else if comp_checks.iter().any(|c| !c.passed) {
        "compliance"
    } else if qual_checks.iter().any(|c| !c.passed) {
        "code quality"
    } else {
        "none"
    };
    let exec_verdict = if report.passed {
        format!("This codebase passed all {} checks with a health score of {}/100 (grade {}). No critical findings were detected across security, quality, or compliance domains.", report.summary.total_checks, health, grade)
    } else {
        let high_count = failed_checks
            .iter()
            .filter(|c| {
                matches!(
                    c.severity.as_deref(),
                    Some("high") | Some("critical") | Some("error")
                )
            })
            .count();
        format!(
            "{} of {} checks failed, concentrated in {}. {} finding{} rated high/critical severity require immediate attention before the next release.",
            failed_checks.len(), report.summary.total_checks, risk_domain,
            high_count, if high_count == 1 { "" } else { "s" }
        )
    };
    let top3: Vec<&CheckResult> = {
        let mut sorted = failed_checks.clone();
        sorted.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        sorted.into_iter().take(3).collect()
    };
    let top3_html = if top3.is_empty() {
        r#"<p style="color:#22c55e;font-size:14px">✓ No action items — all checks passed.</p>"#
            .to_string()
    } else {
        let mut h = String::new();
        for (i, c) in top3.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" | "error" => "High effort",
                "medium" | "warning" => "Medium effort",
                _ => "Low effort",
            };
            let help = c.help.as_deref().unwrap_or("Review and fix flagged items.");
            h.push_str(&format!(
                r#"<div style="display:flex;gap:14px;padding:12px 0;border-bottom:1px solid #f3f4f6;align-items:flex-start">
  <div style="font-size:20px;font-weight:800;color:#d1d5db;min-width:24px">{}</div>
  <div style="flex:1">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:4px">
      <span style="font-weight:700;font-size:14px">{}</span>{}
      <span style="font-size:11px;color:#9ca3af;margin-left:auto">{}</span>
    </div>
    <div style="font-size:13px;color:#6b7280">{}</div>
  </div>
</div>"#, i+1, html_escape(&c.name), severity_badge(sev), effort, html_escape(help)));
        }
        h
    };

    // ── Remediation table ─────────────────────────────────────
    let remediation_html = if failed_checks.is_empty() {
        r#"<p style="color:#22c55e;font-weight:600;font-size:14px">✓ No findings — all checks passed.</p>"#.to_string()
    } else {
        let mut rows = String::new();
        let mut sorted_failed = failed_checks.clone();
        sorted_failed.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        for (i, c) in sorted_failed.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" | "error" => "High",
                "medium" | "warning" => "Medium",
                _ => "Low",
            };
            let help = c
                .help
                .as_deref()
                .unwrap_or("Review and fix the flagged items.");
            rows.push_str(&format!(
                r#"<tr style="border-bottom:1px solid #f3f4f6">
  <td style="padding:10px 14px;font-weight:700;color:#9ca3af">{}</td>
  <td style="padding:10px 14px;font-weight:600">{}</td>
  <td style="padding:10px 14px">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:10px 14px;font-size:12px;color:#6b7280">{}</td>
</tr>"#,
                i + 1,
                html_escape(&c.name),
                severity_badge(sev),
                effort,
                html_escape(help),
            ));
        }
        format!(
            r#"<table style="width:100%;border-collapse:collapse;font-size:13px">
<thead><tr style="background:#f9fafb;border-bottom:2px solid #e5e7eb">
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">#</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Check</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Severity</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Effort</th>
  <th style="padding:8px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Action</th>
</tr></thead><tbody>{rows}</tbody></table>"#,
            rows = rows
        )
    };

    // ── Section builder ───────────────────────────────────────
    fn section_html(title: &str, icon: &str, anchor: &str, checks: &[&CheckResult]) -> String {
        if checks.is_empty() {
            return String::new();
        }
        let rows: String = checks.iter().map(|c| check_row_html(c)).collect();
        let pass_c = checks.iter().filter(|c| c.passed).count();
        let fail_c = checks.len() - pass_c;
        let status_color = if fail_c == 0 { "#22c55e" } else { "#ef4444" };
        let status_pill = if fail_c == 0 {
            r#"<span style="background:#dcfce7;color:#16a34a;padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">ALL PASSED</span>"#.to_string()
        } else {
            format!(
                r#"<span style="background:#fee2e2;color:#ef4444;padding:2px 10px;border-radius:12px;font-size:11px;font-weight:600">{} FAILED</span>"#,
                fail_c
            )
        };
        format!(
            "<section id=\"{anch}\" style=\"margin-bottom:40px\">\n<div style=\"display:flex;align-items:center;gap:12px;margin-bottom:16px;padding-bottom:12px;border-bottom:2px solid #f3f4f6\">\n  <span style=\"font-size:22px\">{icn}</span>\n  <h2 style=\"font-size:18px;font-weight:800;color:#111827;margin:0\">{ttl}</h2>\n  <span style=\"font-size:13px;color:{sc};font-weight:600;margin-left:4px\">{ps}/{tot}</span>\n  <div style=\"margin-left:auto\">{pill}</div>\n</div>\n<div style=\"border-radius:10px;overflow:hidden;box-shadow:0 1px 4px rgba(0,0,0,.08)\">\n<table style=\"width:100%;border-collapse:collapse;font-size:13px\">\n<thead><tr style=\"background:#f9fafb;border-bottom:2px solid #e5e7eb\">\n  <th style=\"padding:9px 14px;width:42px\"></th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Check</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Result / Details</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Score</th>\n  <th style=\"padding:9px 14px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600\">Sev</th>\n</tr></thead>\n<tbody>{rows}</tbody>\n</table></div></section>",
            anch = anchor, icn = icon, ttl = title, sc = status_color,
            ps = pass_c, tot = checks.len(), pill = status_pill, rows = rows,
        )
    }

    // ── File heatmap ─────────────────────────────────────────
    let file_heatmap_html = if report.file_summary.is_empty() {
        String::new()
    } else {
        let mut rows = String::new();
        for fs in &report.file_summary {
            let max_issues = report
                .file_summary
                .first()
                .map(|f| f.issue_count)
                .unwrap_or(1)
                .max(1);
            let bar_width = (fs.issue_count as f64 / max_issues as f64 * 200.0) as usize;
            let bar_color = if fs.severity_score >= 10 {
                "#ef4444"
            } else if fs.severity_score >= 5 {
                "#f59e0b"
            } else {
                "#22c55e"
            };
            rows.push_str(&format!(
                r#"<div class="heatmap-row" style="display:flex;align-items:center;gap:10px;padding:6px 0;border-bottom:1px solid #f3f4f6">
  <div style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{}</div>
  <div style="width:{}px;height:8px;background:{};border-radius:4px"></div>
  <div style="font-size:11px;color:#6b7280;min-width:24px;text-align:right">{}</div>
</div>"#,
                html_escape(&fs.file), bar_width, bar_color, fs.issue_count
            ));
        }
        format!(
            r#"<div class="card" id="heatmap">
  <div class="card-title">&#128293; File Heatmap</div>
  <p style="font-size:13px;color:#9ca3af;margin-bottom:12px">Top files by total issue count across all tools.</p>
  {}
</div>"#,
            rows
        )
    };

    // ── Per-tool findings drill-down ─────────────────────────
    let findings_section_html = {
        let mut checks_with_findings: Vec<&CheckResult> = report
            .checks
            .iter()
            .filter(|c| !c.findings.is_empty())
            .collect();
        checks_with_findings.sort_by_key(|c| match c.severity.as_deref() {
            Some("critical") => 0,
            Some("high") | Some("error") => 1,
            Some("medium") | Some("warning") => 2,
            _ => 3,
        });
        if checks_with_findings.is_empty() {
            String::new()
        } else {
            let mut sections = String::new();
            for check in &checks_with_findings {
                let mut table_rows = String::new();
                for finding in &check.findings {
                    let line = finding
                        .line
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    table_rows.push_str(&format!(
                        r#"<tr class="finding-row" style="border-bottom:1px solid #f3f4f6">
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
  <td style="padding:8px 12px;font-size:12px">{}</td>
  <td style="padding:8px 12px;font-size:12px;color:#6b7280">{}</td>
</tr>"#,
                        html_escape(&finding.file),
                        line,
                        html_escape(&finding.message),
                        html_escape(&finding.fix_hint)
                    ));
                }
                let sev_badge = severity_badge(check.severity.as_deref().unwrap_or("info"));
                sections.push_str(&format!(
                    r#"<div class="card" style="margin-bottom:20px">
  <div class="collapsible-header" style="display:flex;align-items:center;gap:10px;margin-bottom:0px;padding-bottom:12px;border-bottom:1px solid #f3f4f6">
    <span style="font-weight:700;font-size:14px">{}</span>
    <span style="margin-left:4px">{}</span>
    <span style="margin-left:auto;font-size:12px;color:#6b7280">{} findings</span>
  </div>
  <div class="collapsible-body closed" style="max-height:0">
  <table style="width:100%;border-collapse:collapse;font-size:13px;margin-top:12px">
    <thead><tr style="background:#f9fafb;border-bottom:2px solid #e5e7eb">
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">File</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600;width:60px">Line</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Message</th>
      <th style="padding:8px 12px;text-align:left;font-size:11px;text-transform:uppercase;color:#9ca3af;font-weight:600">Fix Hint</th>
    </tr></thead>
    <tbody>{}</tbody>
  </table>
  </div>
</div>"#,
                    html_escape(&check.name), sev_badge, check.findings.len(), table_rows
                ));
            }
            format!(
                r#"<div id="findings" style="scroll-margin-top:80px">
  <div class="card">
    <div class="card-title">&#128269; Findings Drill-Down
      <input type="text" placeholder="Search findings..." oninput="filterFindings(this.value)" style="margin-left:auto;padding:6px 10px;border:1px solid #e2e8f0;border-radius:6px;font-size:12px;width:220px">
    </div>
  </div>
  {}
</div>"#,
                sections
            )
        }
    };

    let sec_section = section_html("Security Checks", "🔒", "security", &sec_checks);
    let qual_section = section_html("Code Quality Checks", "📊", "quality", &qual_checks);
    let comp_section = section_html("Compliance Checks", "📋", "compliance", &comp_checks);

    let donut = donut_svg(pct, overall_color);
    let sec_bar = mini_bar(sec_pass, sec_checks.len(), sec_col);
    let qual_bar = mini_bar(qual_pass, qual_checks.len(), qual_col);
    let comp_bar = mini_bar(comp_pass, comp_checks.len(), comp_col);

    // ── Severity distribution ────────────────────────────────
    let mut sev_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for check in &report.checks {
        for finding in &check.findings {
            *sev_counts.entry(finding.severity.clone()).or_insert(0) += 1;
        }
    }
    let severity_order = [
        ("critical", "#dc2626"),
        ("high", "#ef4444"),
        ("error", "#ef4444"),
        ("medium", "#f59e0b"),
        ("warning", "#f59e0b"),
        ("low", "#22c55e"),
        ("info", "#3b82f6"),
    ];
    let mut severity_chart_data: Vec<(String, usize, &str)> = Vec::new();
    for (sev, color) in &severity_order {
        if let Some(&count) = sev_counts.get(*sev) {
            severity_chart_data.push((sev.to_string(), count, *color));
        }
    }
    let severity_chart_html = if severity_chart_data.is_empty() {
        String::new()
    } else {
        let chart = severity_bar_chart_svg(&severity_chart_data);
        let mut badges = String::new();
        for (sev, count, color) in &severity_chart_data {
            badges.push_str(&format!(
                r#"<span style="background:{}20;color:{};padding:3px 10px;border-radius:12px;font-size:11px;font-weight:600;margin-right:6px">{} {}</span>"#,
                color, color, count, html_escape(sev)
            ));
        }
        format!(
            r#"<div class="card" id="severity-dist">
  <div class="card-title">&#128202; Severity Distribution</div>
  <div style="margin-bottom:14px">{}</div>
  {}
</div>"#,
            badges, chart
        )
    };

    // ── Health score gauge ─────────────────────────────────
    let gauge = gauge_svg(health, grade_color);
    let gauge_html = format!(
        r#"<div class="card" style="display:flex;align-items:center;justify-content:center;flex-direction:column;padding:20px">
  {}
</div>"#,
        gauge
    );

    // ── Trend sparkline ──────────────────────────────────────
    let history_scores = history_health_scores(".cogent-history", 10);
    let sparkline_html = if history_scores.len() >= 2 {
        let spark = sparkline_svg(&history_scores, 320, 80);
        format!(
            r#"<div class="card" id="trends">
  <div class="card-title">&#128200; Health Score Trend</div>
  <p style="font-size:12px;color:#9ca3af;margin-bottom:10px">Last {} runs from .cogent-history</p>
  {}
</div>"#,
            history_scores.len(),
            spark
        )
    } else {
        String::new()
    };

    // Use token replacement instead of format! to avoid CSS hex/class-name conflicts
    let tmpl = include_str!("report.html.tmpl");

    tmpl.replace("__PROJECT__", &html_escape(project))
        .replace("__DATE__", date)
        .replace("__PATH__", &html_escape(&report.path))
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__OC__", overall_color)
        .replace("__GC__", grade_color)
        .replace("__GRADE__", &grade.to_string())
        .replace("__OVERALL_LABEL__", overall_label)
        .replace("__PCT__", &format!("{:.0}", pct))
        .replace("__TOTAL__", &report.summary.total_checks.to_string())
        .replace("__PASSED_N__", &report.summary.passed_checks.to_string())
        .replace("__FAILED_N__", &report.summary.failed_checks.to_string())
        .replace("__HEALTH__", &health.to_string())
        .replace("__DONUT__", &donut)
        .replace("__SEC_BAR__", &sec_bar)
        .replace("__QUAL_BAR__", &qual_bar)
        .replace("__COMP_BAR__", &comp_bar)
        .replace("__SEC_PASS__", &sec_pass.to_string())
        .replace("__SEC_TOTAL__", &sec_checks.len().to_string())
        .replace("__QUAL_PASS__", &qual_pass.to_string())
        .replace("__QUAL_TOTAL__", &qual_checks.len().to_string())
        .replace("__COMP_PASS__", &comp_pass.to_string())
        .replace("__COMP_TOTAL__", &comp_checks.len().to_string())
        .replace(
            "__SS__",
            if sec_pass == sec_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__QS__",
            if qual_pass == qual_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__CS__",
            if comp_pass == comp_checks.len() {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace(
            "__OS__",
            if report.passed {
                "score-pass"
            } else {
                "score-fail"
            },
        )
        .replace("__OL__", if report.passed { "PASS" } else { "FAIL" })
        .replace("__EXEC_VERDICT__", &html_escape(&exec_verdict))
        .replace("__TOP3_HTML__", &top3_html)
        .replace("__REMEDIATION_HTML__", &remediation_html)
        .replace("__FILE_HEATMAP__", &file_heatmap_html)
        .replace("__FINDINGS_SECTION__", &findings_section_html)
        .replace("__SEVERITY_CHART__", &severity_chart_html)
        .replace("__GAUGE__", &gauge_html)
        .replace("__SPARKLINE__", &sparkline_html)
        .replace("__QUAL_SECTION__", &qual_section)
        .replace("__SEC_SECTION__", &sec_section)
        .replace("__COMP_SECTION__", &comp_section)
}

fn render_markdown_report(
    report: &CheckReport,
    project: &str,
    date: &str,
    security_tools: &[&str],
    quality_tools: &[&str],
    compliance_tools: &[&str],
) -> String {
    let overall = if report.passed {
        "✅ PASSED"
    } else {
        "❌ FAILED"
    };
    let pct = if report.summary.total_checks == 0 {
        100.0
    } else {
        report.summary.passed_checks as f64 / report.summary.total_checks as f64 * 100.0
    };

    let mut md = format!(
        "# Cogent Audit Report — {}\n\n\
         **Status:** {}  \n\
         **Generated:** {}  \n\
         **Path:** `{}`  \n\
         **Version:** Cogent v{}\n\n\
         ---\n\n\
         ## Summary\n\n\
         | Metric | Value |\n|---|---|\n\
         | Total Checks | {} |\n\
         | Passed | {} |\n\
         | Failed | {} |\n\
         | Pass Rate | {:.0}% |\n\n",
        project,
        overall,
        date,
        report.path,
        env!("CARGO_PKG_VERSION"),
        report.summary.total_checks,
        report.summary.passed_checks,
        report.summary.failed_checks,
        pct,
    );

    // File heatmap
    if !report.file_summary.is_empty() {
        md.push_str("## File Heatmap\n\n");
        md.push_str("| File | Issues | Severity Score |\n|---|---|---|\n");
        for fs in &report.file_summary {
            md.push_str(&format!(
                "| `{}` | {} | {} |\n",
                fs.file, fs.issue_count, fs.severity_score
            ));
        }
        md.push('\n');
    }

    let failed: Vec<&CheckResult> = report.checks.iter().filter(|c| !c.passed).collect();
    if !failed.is_empty() {
        md.push_str("## Remediation Checklist\n\n");
        md.push_str("| # | Check | Severity | Effort | Action |\n|---|---|---|---|---|\n");
        for (i, c) in failed.iter().enumerate() {
            let sev = c.severity.as_deref().unwrap_or("medium");
            let effort = match sev {
                "critical" | "high" => "High",
                "medium" => "Medium",
                _ => "Low",
            };
            let help = c.help.as_deref().unwrap_or("Review and fix.");
            md.push_str(&format!(
                "| {} | `{}` | {} | {} | {} |\n",
                i + 1,
                c.name,
                sev,
                effort,
                help
            ));
        }
        md.push('\n');
    }

    // Per-tool findings
    let checks_with_findings: Vec<&CheckResult> = report
        .checks
        .iter()
        .filter(|c| !c.findings.is_empty())
        .collect();
    if !checks_with_findings.is_empty() {
        md.push_str("## Findings by Tool\n\n");
        for check in &checks_with_findings {
            md.push_str(&format!(
                "### `{}` ({} findings)\n\n",
                check.name,
                check.findings.len()
            ));
            md.push_str("| File | Line | Message | Rule | Fix Hint |\n|---|---|---|---|---|\n");
            for finding in &check.findings {
                let line = finding
                    .line
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!(
                    "| `{}` | {} | {} | `{}` | {} |\n",
                    finding.file, line, finding.message, finding.rule_id, finding.fix_hint
                ));
            }
            md.push('\n');
        }
    }

    let categories: &[(&str, &str, &[&str])] = &[
        ("Code Quality Checks", "📊", quality_tools),
        ("Security Checks", "🔒", security_tools),
        ("Compliance Checks", "📋", compliance_tools),
    ];
    for (title, icon, tools) in categories {
        let checks: Vec<&CheckResult> = report
            .checks
            .iter()
            .filter(|c| tools.contains(&c.name.as_str()))
            .collect();
        if checks.is_empty() {
            continue;
        }
        md.push_str(&format!("## {} {}\n\n", icon, title));
        md.push_str("| Check | Status | Score | Severity | Message |\n|---|---|---|---|---|\n");
        for c in &checks {
            let status = if c.passed { "✅" } else { "❌" };
            let sev = c.severity.as_deref().unwrap_or("info");
            let score = match (c.score, c.threshold) {
                (Some(s), Some(t)) => format!("{:.1}/{:.1}", s, t),
                (Some(s), None) => format!("{:.1}", s),
                _ => "—".to_string(),
            };
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                c.name, status, score, sev, c.message
            ));
        }
        md.push('\n');
    }

    md.push_str("---\n\n*Generated by Cogent — automated code quality & security auditing.*  \n");
    md.push_str("*This report is machine-generated. Results should be reviewed by a qualified engineer before use in compliance filings.*\n");
    md
}

fn setup_command() {
    let ascii_art = r#"
   ____          _      __  __      _        _          
  / ___|___   __| | ___|  \/  | ___| |_ _ __(_) ___ ___ 
 | |   / _ \ / _` |/ _ \ |\/| |/ _ \ __| '__| |/ __/ __|
 | |__| (_) | (_| |  __/ |  | |  __/ |_| |  | | (__\__ \
  \____\___/ \__,_|\___|_|  |_|\___|\__|_|  |_|\___|___/
"#;
    println!("{}", ascii_art.cyan().bold());
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
    println!("{}", "  Cogent Doctor & Setup".cyan().bold());
    println!(
        "{}\n",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );

    let mut all_passed = true;

    // Check cargo
    if std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .is_ok()
    {
        println!("  {} cargo installed", "[✓]".green().bold());
    } else {
        println!("  {} cargo NOT installed", "[✗]".red().bold());
        println!("      => {}", "Install Rust: https://rustup.rs/".yellow());
        all_passed = false;
    }

    // Check cargo-llvm-cov
    if std::process::Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .output()
        .is_ok()
    {
        println!("  {} cargo-llvm-cov installed", "[✓]".green().bold());
    } else {
        println!("  {} cargo-llvm-cov NOT installed", "[✗]".red().bold());
        println!("      => {}", "Run: cargo install cargo-llvm-cov".yellow());
        all_passed = false;
    }

    // Check llvm-tools-preview
    let rustup_out = std::process::Command::new("rustup")
        .args(["component", "list"])
        .output()
        .ok();
    if let Some(out) = rustup_out {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.contains("llvm-tools-preview (installed)")
            || stdout.contains("llvm-tools (installed)")
        {
            println!("  {} llvm-tools installed", "[✓]".green().bold());
        } else {
            println!("  {} llvm-tools NOT installed", "[✗]".red().bold());
            println!(
                "      => {}",
                "Run: rustup component add llvm-tools-preview".yellow()
            );
            all_passed = false;
        }
    } else {
        println!(
            "  {} rustup not found, could not verify llvm-tools",
            "[?]".yellow().bold()
        );
    }

    // Check .quality.toml
    if std::path::Path::new(".quality.toml").exists() {
        println!(
            "  {} .quality.toml configuration found",
            "[✓]".green().bold()
        );
    } else {
        println!("  {} .quality.toml NOT found", "[✗]".red().bold());
        println!("      => {}", "Run: cogent init".yellow());
        all_passed = false;
    }

    println!(
        "\n{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
    if all_passed {
        println!(
            "  {}",
            "Everything looks good! Your codebase is ready."
                .green()
                .bold()
        );
    } else {
        println!(
            "  {}",
            "Please resolve the missing requirements above."
                .red()
                .bold()
        );
    }
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
}
