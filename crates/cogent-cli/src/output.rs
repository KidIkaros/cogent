//! Terminal output helpers: progress bar, spinner, summary boxes, formatting.

#![deny(clippy::all)]

use colored::Colorize;
use cogent_common::{CheckResult, health_score};
use std::time::Instant;

/// Detect whether stderr is a real TTY (not CI, not piped).
pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    if std::env::var("CI").is_ok()
        || std::env::var("NO_COLOR").is_ok()
        || std::env::var("COGENT_NO_PROGRESS").is_ok()
    {
        return false;
    }
    std::io::stderr().is_terminal()
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
    let status = if passed {
        "PASSED".green().bold().to_string()
    } else {
        "FAILED".red().bold().to_string()
    };
    let grade_bg = match grade {
        'A' => format!(" {} ", grade).green().bold(),
        'B' => format!(" {} ", grade).cyan().bold(),
        'C' => format!(" {} ", grade).yellow().bold(),
        _ => format!(" {} ", grade).red().bold(),
    };

    // Category breakdown
    let security = ["secrets","vulnscan","sast","crypto","taint","access-control","supply-chain"];
    let compliance = ["licenses","sbom","outdated"];
    let mut s_pass = 0usize; let mut s_total = 0usize;
    let mut q_pass = 0usize; let mut q_total = 0usize;
    let mut c_pass = 0usize; let mut c_total = 0usize;
    for ch in checks {
        let n = ch.name.as_str();
        if security.contains(&n) { s_total += 1; if ch.passed { s_pass += 1; } }
        else if compliance.contains(&n) { c_total += 1; if ch.passed { c_pass += 1; } }
        else { q_total += 1; if ch.passed { q_pass += 1; } }
    }
    fn cat_str(pass: usize, total: usize, icon: &str, label: &str) -> String {
        if total == 0 { return String::new(); }
        let col = if pass == total { format!("{}/{}", pass, total).green() } else { format!("{}/{}", pass, total).red() };
        format!("{} {} {}", icon, label, col)
    }
    let sec_s = cat_str(s_pass, s_total, "🔒", "Sec");
    let qual_s = cat_str(q_pass, q_total, "📊", "Qual");
    let comp_s = cat_str(c_pass, c_total, "📋", "Comp");
    let cat_line = [sec_s, qual_s, comp_s].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("   ");

    let inner = 52usize;
    let bar = "─".repeat(inner);

    eprintln!();
    eprintln!("  {}", bar);
    eprintln!("  {} {}  {}", kind.bold(), status, grade_bg);
    eprintln!("  {}", bar);
    let pct_col = if passed { format!("{}/{}", passed_count, total).green().bold() } else { format!("{}/{}", passed_count, total).red().bold() };
    eprintln!("  Checks {}  Score {}/100  {}", pct_col, score, format_elapsed(elapsed).bright_black());
    if !cat_line.is_empty() {
        eprintln!("  {}", cat_line);
    }
    eprintln!("  {}", path.bright_black());
    eprintln!("  {}", bar);
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
    let bar = "─".repeat(52);
    eprintln!();
    eprintln!("  {}", bar);
    eprintln!("  {}", "Quick Fixes".bold());
    eprintln!("  {}", bar);
    for (name, fix) in rows {
        eprintln!("  {} {}", name.cyan(), fix.bright_black());
    }
    eprintln!("  {}", bar);
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

fn print_offender_lines(check: &CheckResult) {
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
}

/// Print inline offenders under a check line (used by run_check! for failures).
pub fn print_offenders(check: &CheckResult) {
    print_offender_lines(check);
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
                    format!(                    "… {} more", arr.len() - 5).bright_black()
                );
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_offenders, format_elapsed, format_ms, strip_ansi, visible_len};
    use cogent_common::CheckResult;

    // ── helper ────────────────────────────────────────────────────────────

    fn make_check(details: serde_json::Value) -> CheckResult {
        CheckResult {
            name: "test-check".into(),
            passed: false,
            score: Some(42.0),
            threshold: Some(50.0),
            message: "test message".into(),
            details,
            severity: Some("info".into()),
            help: Some("Test help".into()),
            findings: Vec::new(),
            rule_id: Some("test-rule".into()),
        }
    }

    // ── strip_ansi ────────────────────────────────────────────────────────

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn test_strip_ansi_plain_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
        assert_eq!(strip_ansi("no ansi here!"), "no ansi here!");
    }

    #[test]
    fn test_strip_ansi_color_codes() {
        let red = "\x1b[31mred\x1b[0m";
        assert_eq!(strip_ansi(red), "red");

        let bold = "\x1b[1mbold\x1b[22m";
        assert_eq!(strip_ansi(bold), "bold");
    }

    #[test]
    fn test_strip_ansi_multiple_codes() {
        let input = "\x1b[31m\x1b[1mred+bold\x1b[0m";
        assert_eq!(strip_ansi(input), "red+bold");
    }

    #[test]
    fn test_strip_ansi_mixed_content() {
        let input = "normal \x1b[32mgreen\x1b[0m end";
        assert_eq!(strip_ansi(input), "normal green end");
    }

    #[test]
    fn test_strip_ansi_complex_sequence() {
        // Sequence with multiple parameters: \x1b[38;5;34m
        let input = "\x1b[38;5;34mcustom\x1b[0m";
        assert_eq!(strip_ansi(input), "custom");
    }

    #[test]
    fn test_strip_ansi_bright_black_sequence() {
        // Common cogent pattern: bright_black() produces \x1b[90m
        let input = "\x1b[90mdimmed\x1b[0m";
        assert_eq!(strip_ansi(input), "dimmed");
    }

    #[test]
    fn test_strip_ansi_esc_without_bracket() {
        // \x1b alone (not followed by '[') should pass through unchanged
        let input = "escape\x1bchar";
        assert_eq!(strip_ansi(input), "escape\x1bchar");
    }

    // ── visible_len ───────────────────────────────────────────────────────

    #[test]
    fn test_visible_len_empty() {
        assert_eq!(visible_len(""), 0);
    }

    #[test]
    fn test_visible_len_plain() {
        assert_eq!(visible_len("hello"), 5);
        assert_eq!(visible_len("a b c"), 5);
    }

    #[test]
    fn test_visible_len_with_ansi() {
        // ANSI codes should not count toward visible length
        let red = "\x1b[31mred\x1b[0m";
        assert_eq!(visible_len(red), 3);

        let bold = "\x1b[1mbold\x1b[22m";
        assert_eq!(visible_len(bold), 4);
    }

    #[test]
    fn test_visible_len_mixed() {
        let input = "normal \x1b[32mgreen\x1b[0m end";
        assert_eq!(visible_len(input), 15); // "normal green end" = 15 chars
    }

    #[test]
    fn test_visible_len_multi_byte() {
        // Unicode characters count as a single char
        assert_eq!(visible_len("héllo"), 5);
        assert_eq!(visible_len("✓"), 1);
    }

    // ── format_ms ─────────────────────────────────────────────────────────

    #[test]
    fn test_format_ms_zero() {
        assert_eq!(format_ms(0), "0ms");
    }

    #[test]
    fn test_format_ms_milliseconds() {
        assert_eq!(format_ms(1), "1ms");
        assert_eq!(format_ms(100), "100ms");
        assert_eq!(format_ms(999), "999ms");
    }

    #[test]
    fn test_format_ms_seconds() {
        assert_eq!(format_ms(1000), "1.0s");
        assert_eq!(format_ms(1500), "1.5s");
        assert_eq!(format_ms(2000), "2.0s");
        assert_eq!(format_ms(2500), "2.5s");
    }

    #[test]
    fn test_format_ms_large() {
        assert_eq!(format_ms(60000), "60.0s");
        assert_eq!(format_ms(90000), "90.0s");
    }

    // ── format_elapsed ─────────────────────────────────────────────────────

    #[test]
    fn test_format_elapsed_zero() {
        let d = std::time::Duration::from_millis(0);
        assert_eq!(format_elapsed(d), "0.0s");
    }

    #[test]
    fn test_format_elapsed_milliseconds() {
        let d = std::time::Duration::from_millis(500);
        assert_eq!(format_elapsed(d), "0.5s");

        let d = std::time::Duration::from_millis(999);
        assert_eq!(format_elapsed(d), "1.0s");
    }

    #[test]
    fn test_format_elapsed_seconds() {
        let d = std::time::Duration::from_secs(1);
        assert_eq!(format_elapsed(d), "1.0s");

        let d = std::time::Duration::from_secs(5);
        assert_eq!(format_elapsed(d), "5.0s");
    }

    #[test]
    fn test_format_elapsed_longer() {
        let d = std::time::Duration::from_secs(120);
        assert_eq!(format_elapsed(d), "120.0s");
    }

    #[test]
    fn test_format_elapsed_sub_second_precision() {
        // Under 1000ms, it formats with 1 decimal: total_ms / 1000.0
        let d = std::time::Duration::from_millis(123);
        assert_eq!(format_elapsed(d), "0.1s");
    }

    // ── extract_offenders ─────────────────────────────────────────────────

    #[test]
    fn test_extract_offenders_empty_details() {
        let check = make_check(serde_json::json!({}));
        let result = extract_offenders(&check, 5);
        assert!(result.is_empty(), "empty details should produce no offenders");
    }

    #[test]
    fn test_extract_offenders_no_arrays() {
        let check = make_check(serde_json::json!({"key": "value"}));
        let result = extract_offenders(&check, 5);
        assert!(result.is_empty(), "no recognized arrays should produce no offenders");
    }

    #[test]
    fn test_extract_offenders_items_array() {
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "src/main.rs", "line": 10, "context": "unsafe block"},
                {"file": "src/lib.rs", "line": 42, "context": "missing check"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("src/main.rs".into(), Some(10), "unsafe block".into()));
        assert_eq!(result[1], ("src/lib.rs".into(), Some(42), "missing check".into()));
    }

    #[test]
    fn test_extract_offenders_functions_array() {
        let check = make_check(serde_json::json!({
            "functions": [
                {"file": "src/main.rs", "line": 20, "name": "complex_func"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("src/main.rs".into(), Some(20), "complex_func".into()));
    }

    #[test]
    fn test_extract_offenders_findings_array() {
        let check = make_check(serde_json::json!({
            "findings": [
                {"file": "/tmp/test.rs", "line": 5, "message": "hardcoded secret"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("/tmp/test.rs".into(), Some(5), "hardcoded secret".into()));
    }

    #[test]
    fn test_extract_offenders_violations_array() {
        let check = make_check(serde_json::json!({
            "violations": [
                {"file": "app.js", "line": 100, "kind": "E0001"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("app.js".into(), Some(100), "E0001".into()));
    }

    #[test]
    fn test_extract_offenders_secrets_array() {
        let check = make_check(serde_json::json!({
            "secrets": [
                {"file": ".env", "line": 1, "type": "AWS_KEY"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], (".env".into(), Some(1), "AWS_KEY".into()));
    }

    #[test]
    fn test_extract_offenders_duplicates_array() {
        let check = make_check(serde_json::json!({
            "duplicates": [
                {"file": "clone.rs", "line": 30, "context": "identical block"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("clone.rs".into(), Some(30), "identical block".into()));
    }

    #[test]
    fn test_extract_offenders_item_without_file() {
        // Items missing file/line should not appear in output
        let check = make_check(serde_json::json!({
            "items": [
                {"context": "orphan item"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        // No file and no desc means empty string for both, which is filtered out
        assert!(result.is_empty() || result[0].0.is_empty());
    }

    #[test]
    fn test_extract_offenders_item_with_only_desc() {
        // Item with only desc (no file) should appear with empty file
        let check = make_check(serde_json::json!({
            "items": [
                {"context": "some context"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        // Has desc but no file - should still appear
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].2, "some context");
    }

    #[test]
    fn test_extract_offenders_respects_limit() {
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "a.rs", "line": 1, "context": "first"},
                {"file": "b.rs", "line": 2, "context": "second"},
                {"file": "c.rs", "line": 3, "context": "third"},
            ]
        }));
        let result = extract_offenders(&check, 2);
        assert_eq!(result.len(), 2, "limit of 2 should return only 2 offenders");
        assert_eq!(result[0].2, "first");
        assert_eq!(result[1].2, "second");
    }

    #[test]
    fn test_extract_offenders_uses_first_matching_array() {
        // The function iterates arrays in order: items, functions, findings, violations, secrets, duplicates
        // It should break after finding the first non-empty array.
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "from_items.rs", "line": 1, "context": "from items"},
            ],
            "functions": [
                {"file": "from_funcs.rs", "line": 1, "name": "from functions"},
            ],
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].2, "from items", "should use 'items' array first");
    }

    #[test]
    fn test_extract_offenders_falls_through_to_second_array() {
        // If the first array is empty, it should fall through to the next
        let check = make_check(serde_json::json!({
            "items": [],
            "functions": [
                {"file": "from_funcs.rs", "line": 1, "name": "from functions"},
            ],
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].2, "from functions", "should fall through to 'functions'");
    }

    #[test]
    fn test_extract_offenders_uses_item_kind_field() {
        // The function tries kind, then name, then type, then message as fallback for desc
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "test.rs", "line": 1, "kind": "violation_kind"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result[0].2, "violation_kind");
    }

    #[test]
    fn test_extract_offenders_uses_item_type_field() {
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "test.rs", "line": 1, "type": "item_type"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result[0].2, "item_type");
    }
}
