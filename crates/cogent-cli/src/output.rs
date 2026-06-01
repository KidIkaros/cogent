//! Terminal output helpers: progress bar, spinner, summary boxes, formatting.

#![deny(clippy::all)]

use colored::Colorize;
use cogent_common::{CheckResult, health_score};
use std::time::Instant;

/// Detect whether stderr is a real TTY (not CI, not piped).
pub fn is_tty() -> bool {
    if std::env::var("CI").is_ok()
        || std::env::var("NO_COLOR").is_ok()
        || std::env::var("COGENT_NO_PROGRESS").is_ok()
    {
        return false;
    }
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

pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// An overall progress bar for multi-step operations (run_batch).
pub struct Bar {
    total: usize,
    done: usize,
    start: Instant,
    tty: bool,
    last_len: usize,
    current_tool: String,
}

impl Bar {
    pub fn new(total: usize) -> Self {
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

    pub fn set_current(&mut self, tool: &str) {
        self.current_tool = tool.to_string();
        self.render();
    }

    pub fn advance(&mut self, tool: &str, passed: bool, duration_ms: u64) {
        self.done += 1;
        let icon = if passed {
            "  ✓".green().bold()
        } else {
            "  ✗".red().bold()
        };
        let name_col = if passed { tool.normal() } else { tool.red() };
        let dur_str = format_ms(duration_ms);
        if self.tty {
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
            format!("  {} {}", frame, self.current_tool.bright_black())
        };
        let bar_line = format!(
            "\r  {} {:>3}% [{}] {}{}",
            frame,
            pct,
            bar,
            eta_str.bright_black(),
            running
        );
        eprint!("{}", bar_line);
        self.last_len = bar_line.len();
        if !running.is_empty() {
            eprint!("\n{}", running);
            eprint!("\x1b[1A");
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }

    pub fn finish(&self) {
        if self.tty {
            eprintln!("\r{:<80}", "");
        }
    }
}

pub fn format_elapsed(d: std::time::Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        format!("{:.1}s", total_ms as f64 / 1000.0)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

pub fn format_ms(ms: u64) -> String {
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

/// Strip ANSI CSI escape sequences (ESC [ ... m) from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
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
pub fn box_row(content: &str, inner_width: usize) {
    let vlen = visible_len(content);
    let padding = inner_width.saturating_sub(vlen);
    eprintln!("  ║  {}{}║", content, " ".repeat(padding));
}

/// Visible character width of a string (strips ANSI first).
pub fn visible_len(s: &str) -> usize {
    let plain = strip_ansi(s);
    plain.chars().count()
}

/// Print a compact summary box after check completion.
pub fn print_summary_box(
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
pub fn print_fix_summary(checks: &[CheckResult]) {
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
            _ => check.help.clone().unwrap_or_default(),
        };
        rows.push((check.name.clone(), fix));
    }
    let inner = 50usize;
    let border = "═".repeat(inner + 2);
    eprintln!("  ╔{}╗", border);
    box_row("Quick Fixes", inner);
    eprintln!("  ╠{}╣", border);
    for (name, fix) in rows {
        let line = format!("  {}  {}", name.cyan(), fix.bright_black());
        box_row(&line, inner);
    }
    eprintln!("  ╚{}╝", border);
    eprintln!();
}

/// Extract up to `limit` top offenders from a CheckResult's details JSON.
pub fn extract_offenders(check: &CheckResult, limit: usize) -> Vec<(String, Option<u64>, String)> {
    let mut out = Vec::new();
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
pub fn print_offenders(check: &CheckResult) {
    let offenders = extract_offenders(check, 5);
    for (file, line, desc) in offenders {
        let loc = if file.is_empty() {
            String::new()
        } else if let Some(l) = line {
            format!("{}:{}", file, l)
        } else {
            file
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
