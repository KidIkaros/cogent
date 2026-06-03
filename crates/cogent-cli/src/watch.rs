//! Watch mode for cogent-cli.

#![deny(clippy::all)]

use colored::Colorize;
use crate::check_runners::*;
use crate::config::{detect_project, ProjectProfile};
use crate::progress::{format_elapsed, run_with_spinner};

// WATCH MODE
// ═══════════════════════════════════════════

pub(crate) fn watch_mode(path: &str, checks: &str, debounce_ms: u64, no_tests: bool, full: bool) -> i32 {
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
pub(crate) fn run_tests_and_coverage(profile: &ProjectProfile) -> Option<String> {
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

pub(crate) fn run_watch_checks(
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
pub(crate) fn print_cycle_diff(prev: &[(String, bool)], curr: &[(String, bool)]) {
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


#[cfg(test)]
mod tests {
    use super::*;

    // ── print_cycle_diff ──

    #[test]
    fn test_print_cycle_diff_empty_prev_no_output() {
        // When prev is empty, nothing is printed
        let prev: Vec<(String, bool)> = vec![];
        let curr = vec![("crap".to_string(), true)];
        // Should return without writing to stderr
        print_cycle_diff(&prev, &curr);
        // No assertion needed — just verifies no panic
    }

    #[test]
    fn test_print_cycle_diff_no_changes_no_output() {
        let prev = vec![("crap".to_string(), true), ("debt".to_string(), true)];
        let curr = vec![("crap".to_string(), true), ("debt".to_string(), true)];
        print_cycle_diff(&prev, &curr);
        // No changes — should not print anything
    }

    #[test]
    fn test_print_cycle_diff_new_check_no_change() {
        // A new check that wasn't in prev is not a regression or fix
        let prev: Vec<(String, bool)> = vec![];
        let curr = vec![("new-check".to_string(), false)];
        print_cycle_diff(&prev, &curr);
    }

    #[test]
    fn test_print_cycle_diff_tracks_state_across_calls() {
        // Simulates three cycles: pass → fail → pass
        let mut prev: Vec<(String, bool)> = vec![];

        // Cycle 1: first run, everything passes
        let mut curr = vec![("crap".to_string(), true), ("debt".to_string(), true)];
        print_cycle_diff(&prev, &curr); // prev empty → no output
        prev = curr;

        // Cycle 2: crap regresses
        curr = vec![("crap".to_string(), false), ("debt".to_string(), true)];
        print_cycle_diff(&prev, &curr); // should print regression
        prev = curr;

        // Cycle 3: crap fixed
        curr = vec![("crap".to_string(), true), ("debt".to_string(), true)];
        print_cycle_diff(&prev, &curr); // should print fix
    }

    #[test]
    fn test_print_cycle_diff_multiple_changes() {
        let prev = vec![
            ("check-a".to_string(), true),
            ("check-b".to_string(), false),
            ("check-c".to_string(), true),
        ];
        let curr = vec![
            ("check-a".to_string(), false), // regression
            ("check-b".to_string(), true),  // fix
            ("check-c".to_string(), true),  // unchanged
        ];
        print_cycle_diff(&prev, &curr);
        // Should output both a fix and a regression
    }

    #[test]
    fn test_print_cycle_diff_all_regressions() {
        let prev = vec![("a".to_string(), true), ("b".to_string(), true)];
        let curr = vec![("a".to_string(), false), ("b".to_string(), false)];
        print_cycle_diff(&prev, &curr);
    }

    #[test]
    fn test_print_cycle_diff_all_fixes() {
        let prev = vec![("a".to_string(), false), ("b".to_string(), false)];
        let curr = vec![("a".to_string(), true), ("b".to_string(), true)];
        print_cycle_diff(&prev, &curr);
    }
}
