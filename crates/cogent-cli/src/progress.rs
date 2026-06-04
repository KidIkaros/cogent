//! Progress indicators, spinners, and standalone check runners.

#![deny(clippy::all)]

use colored::Colorize;
use crate::types::CheckResult;
use std::time::Instant;

pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Detect whether stderr is a real TTY.
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

pub fn format_elapsed(d: std::time::Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        format!("{:.1}s", total_ms as f64 / 1000.0)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

pub(crate) fn format_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

#[allow(dead_code)]
fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() { break; }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn visible_len(s: &str) -> usize {
    strip_ansi(s).chars().count()
}



pub(crate) fn box_row(content: &str, inner_width: usize) {
    let vlen = visible_len(content);
    let pad = inner_width.saturating_sub(vlen);
    eprintln!("  ║ {}{} ║", content, " ".repeat(pad));
}


pub(crate) fn health_score(checks: &[CheckResult]) -> (u32, char) {
    if checks.is_empty() {
        return (100, 'A');
    }
    let passed = checks.iter().filter(|c| c.passed).count();
    let total = checks.len();
    let raw = (passed * 100 / total) as u32;
    let grade = match raw {
        90..=100 => 'A',
        80..=89 => 'B',
        65..=79 => 'C',
        50..=64 => 'D',
        _ => 'F',
    };
    (raw, grade)
}

fn extract_offenders(check: &CheckResult, limit: usize) -> Vec<(String, Option<u64>, String)> {
    let mut out = Vec::new();
    let arrays = ["items", "functions", "findings", "violations", "secrets", "duplicates"];
    for key in &arrays {
        if let Some(arr) = check.details.get(key).and_then(|v| v.as_array()) {
            for item in arr.iter().take(limit) {
                let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let line = item.get("line").and_then(|v| v.as_u64());
                let desc = item.get("context").or_else(|| item.get("kind"))
                    .or_else(|| item.get("name")).or_else(|| item.get("type"))
                    .or_else(|| item.get("message"))
                    .and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !file.is_empty() || !desc.is_empty() {
                    out.push((file, line, desc));
                }
            }
            if !out.is_empty() { break; }
        }
    }
    out
}

pub(crate) fn print_offenders(check: &CheckResult) {
    let offenders = extract_offenders(check, 5);
    if offenders.is_empty() { return; }
    for (file, line, desc) in &offenders {
        let loc = match line {
            Some(l) => format!("{}:{}", file, l),
            None if file.is_empty() => String::new(),
            None => file.clone(),
        };
        if loc.is_empty() && desc.is_empty() { continue; }
        let truncated_desc = if desc.len() > 60 { format!("{}…", &desc[..60]) } else { desc.clone() };
        if loc.is_empty() {
            eprintln!("      {}", truncated_desc.bright_black());
        } else {
            eprintln!("      {}  {}", loc.cyan(), truncated_desc.bright_black());
        }
    }
    let arrays = ["items", "functions", "findings", "violations", "secrets", "duplicates"];
    for key in &arrays {
        if let Some(arr) = check.details.get(key).and_then(|v| v.as_array()) {
            if arr.len() > 5 {
                eprintln!("      {}", format!("… {} more", arr.len() - 5).bright_black());
            }
            break;
        }
    }
}

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
    let checks_str = format!("{}/{} checks passed  ·  {} total", passed_count, total, format_elapsed(elapsed));
    let checks_col = if passed { checks_str.green().to_string() } else { checks_str.red().to_string() };
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

pub fn print_fix_summary(checks: &[CheckResult]) {
    let failed: Vec<&CheckResult> = checks.iter().filter(|c| !c.passed).collect();
    if failed.is_empty() { return; }
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
pub fn print_severity_grouped(checks: &[CheckResult]) {
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
pub fn run_with_spinner<T, F>(label: &str, f: F) -> T
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

/// Run a standalone check (e.g. `cogent crap`, `cogent debt`) with optional
/// spinner progress, format the result, and return the appropriate exit code.
#[allow(dead_code)]
pub fn run_standalone_check<F>(name: &str, format: &str, check_fn: F) -> i32
where
    F: FnOnce() -> cogent_common::CheckResult,
    F: Send + 'static,
{
    tracing::info!(tool = name, format, "running standalone check");
    let result = if format == "text" {
        run_with_spinner(name, check_fn)
    } else {
        check_fn()
    };
    let passed = result.passed;
    match format {
        "text" => {
            let icon = if passed {
                "✓".green().bold()
            } else {
                "✗".red().bold()
            };
            eprintln!("  {} {}  {}", icon, name, result.message.bright_black());
            println!("{}", result.message);
        }
        _ => match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize check result");
            }
        },
    }
    if passed { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::{extract_offenders, format_elapsed, format_ms, strip_ansi, visible_len};
    use crate::types::CheckResult;

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
        // \x1b alone (not followed by '[') is dropped by the function
        // (the else branch only pushes non-ESC chars)
        let input = "escape\x1bchar";
        assert_eq!(strip_ansi(input), "escapechar");
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
        assert_eq!(visible_len(input), 16); // "normal green end" = 16 chars
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
        let items = serde_json::json!({
            "items": [
                {"file": "src/main.rs", "line": 10, "context": "unsafe block"},
                {"file": "src/lib.rs", "line": 42, "context": "missing check"},
            ]
        });
        let check = make_check(items);
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
    fn test_extract_offenders_uses_kind_field() {
        // The function tries context, then kind, then name, then type, then message
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "test.rs", "line": 1, "kind": "violation_kind"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result[0].2, "violation_kind");
    }

    #[test]
    fn test_extract_offenders_uses_type_field() {
        let check = make_check(serde_json::json!({
            "items": [
                {"file": "test.rs", "line": 1, "type": "item_type"},
            ]
        }));
        let result = extract_offenders(&check, 5);
        assert_eq!(result[0].2, "item_type");
    }

    // ── health_score ──────────────────────────────────────────────────────

    #[test]
    fn test_health_score_empty() {
        let (score, grade) = super::health_score(&[]);
        assert_eq!(score, 100);
        assert_eq!(grade, 'A');
    }

    #[test]
    fn test_health_score_all_pass() {
        let checks = [
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
        ];
        let (score, grade) = super::health_score(&checks);
        assert_eq!(score, 100);
        assert_eq!(grade, 'A');
    }

    #[test]
    fn test_health_score_mixed() {
        let checks = vec![
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        // 2/4 = 50 → D
        let (score, grade) = super::health_score(&checks);
        assert_eq!(score, 50);
        assert_eq!(grade, 'D');
    }

    #[test]
    fn test_health_score_all_fail() {
        let checks = vec![
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        let (score, grade) = super::health_score(&checks);
        assert_eq!(score, 0);
        assert_eq!(grade, 'F');
    }

    #[test]
    fn test_health_score_grades() {
        // 2/3 = 66% → C (65..=79 range)
        let c_checks = vec![
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        assert_eq!(super::health_score(&c_checks).1, 'C'); // 2/3 = 66% → C
        
        // 9/10 = 90 → A
        let a_checks = vec![
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        assert_eq!(super::health_score(&a_checks).1, 'A'); // 9/10 = 90 → A
        
        // 8/10 = 80 → B
        let b_checks = vec![
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        assert_eq!(super::health_score(&b_checks).1, 'B'); // 8/10 = 80 → B
        
        // 6/10 = 60 → D (50..=64)
        let d_checks = vec![
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: true, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
            CheckResult { passed: false, ..make_check(serde_json::json!({})) },
        ];
        assert_eq!(super::health_score(&d_checks).1, 'D'); // 6/10 = 60 → D
    }
}
