//! Git hook installation and management.

#![deny(clippy::all)]

use crate::config::{detect_project, ProjectProfile};

/// Install a pre-commit hook in the specified git repository.
pub fn install_hooks(repo: &str, fast: bool) -> i32 {
    let profile = detect_project(repo);
    install_hooks_impl(repo, fast, &profile)
}

pub(crate) fn install_hooks_impl(repo: &str, fast: bool, profile: &ProjectProfile) -> i32 {
    let hook_dir = format!("{}/.git/hooks", repo);
    #[cfg(windows)]
    let hook_path = format!("{}/pre-commit.cmd", hook_dir);
    #[cfg(not(windows))]
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

    #[cfg(windows)]
    let hook_script = build_hook_script_windows(fast, profile);
    #[cfg(not(windows))]
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
        if let Ok(meta) = std::fs::metadata(&hook_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&hook_path, perms).ok();
        } else {
            tracing::warn!(path = %hook_path, "could not read hook metadata to set permissions");
        }
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

#[cfg(windows)]
fn build_hook_script_windows(fast: bool, profile: &ProjectProfile) -> String {
    build_hook_template_windows(fast, &profile.test_cmd.join(" "), profile.is_coverage_available(), &profile.ecosystem.to_string())
}

/// Build the Windows pre-commit hook script content.
/// Extracted from `build_hook_script_windows` so it can be tested on all platforms.
#[cfg(any(windows, test))]
fn build_hook_template_windows(fast: bool, test_cmd: &str, has_coverage: bool, ecosystem: &str) -> String {
    let check_cmd = r#"@echo off
REM Cogent pre-commit hook (Windows) — installed by `cogent install-hooks`
REM Remove with: cogent uninstall-hooks
REM To skip: git commit --no-verify

where cogent >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    set CM_BIN=cogent
) else if exist target\release\cogent.exe (
    set CM_BIN=target\release\cogent.exe
) else (
    echo cogent: binary not found, skipping pre-commit check >&2
    exit /b 0
)"#;

    if fast || !has_coverage {
        format!(
            r#"{check_cmd}

%CM_BIN% check . --no-cache --format text
if %ERRORLEVEL% NEQ 0 exit /b 1
"#,
            check_cmd = check_cmd
        )
    } else {
        format!(
            r#"{check_cmd}

echo [cogent] Running tests ({ecosystem})...
{test_cmd}
if %ERRORLEVEL% NEQ 0 exit /b 1

echo [cogent] Running quality checks...
%CM_BIN% check . --no-cache --format text
if %ERRORLEVEL% NEQ 0 exit /b 1
"#,
            check_cmd = check_cmd,
            test_cmd = test_cmd,
        )
    }
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
# To skip: git commit --no-verify
set -euo pipefail

{cm_bin}

$CM_BIN check . --no-cache --format text
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
$CM_BIN check . {lcov_flag} --no-cache --format text
"#,
            cm_bin = cm_bin,
            ecosystem = profile.ecosystem,
            test_cmd = test_cmd,
            cov_cmd = cov_cmd,
            lcov_flag = lcov_flag,
        )
    }
}

/// Remove the Cogent pre-commit git hook.
pub fn uninstall_hooks(repo: &str) -> i32 {
    #[cfg(windows)]
    let hook_path = format!("{}/.git/hooks/pre-commit.cmd", repo);
    #[cfg(not(windows))]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(test_cmd: Vec<&str>) -> ProjectProfile {
        ProjectProfile {
            ecosystem: crate::config::ProjectEcosystem::Rust,
            test_cmd: test_cmd.into_iter().map(String::from).collect(),
            coverage_cmd: vec!["cargo", "llvm-cov", "--lcov", "--output-path", "lcov.info"]
                .into_iter()
                .map(String::from)
                .collect(),
            lcov_path: "lcov.info".into(),
            watch_extensions: vec!["rs".into()],
            max_crap: 15.0,
            min_doc: 95.0,
            max_debt: 0,
            max_complexity_violations: 0,
        }
    }

    #[test]
    fn test_build_hook_script_shebang() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.starts_with("#!/usr/bin/env bash"));
    }

    #[test]
    fn test_build_hook_script_set_euo() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.contains("set -euo pipefail"));
    }

    #[test]
    fn test_build_hook_script_cm_bin_detection() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.contains("command -v cogent"));
        assert!(script.contains("target/release/cogent"));
    }

    #[test]
    fn test_build_hook_script_full_mode_contains_test_and_coverage() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.contains("Running tests"));
        assert!(script.contains("cargo test"));
        assert!(script.contains("Collecting coverage"));
        assert!(script.contains("cargo llvm-cov --lcov --output-path lcov.info"));
        assert!(script.contains("Running quality checks"));
        assert!(script.contains("$CM_BIN check . --coverage lcov.info"));
        assert!(script.contains("--no-cache"));
    }

    #[test]
    fn test_build_hook_script_fast_mode_skips_tests() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(true, &profile);
        assert!(!script.contains("Running tests"));
        assert!(!script.contains("Collecting coverage"));
        assert!(script.contains("$CM_BIN check . --no-cache --format text"));
    }

    #[test]
    fn test_build_hook_script_fast_mode_comment() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(true, &profile);
        assert!(script.contains("fast/metrics-only"));
    }

    #[test]
    fn test_build_hook_script_full_mode_comment() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.contains("full: tests + coverage + metrics"));
    }

    #[test]
    fn test_build_hook_script_no_coverage_fast_mode() {
        let mut profile = make_profile(vec!["cargo", "test"]);
        profile.coverage_cmd = vec![];
        profile.lcov_path = String::new();
        // When coverage is unavailable, full mode should behave like fast mode
        let script = build_hook_script(false, &profile);
        assert!(!script.contains("Running tests"));
        assert!(!script.contains("Collecting coverage"));
        assert!(script.contains("$CM_BIN check . --no-cache --format text"));
    }

    #[test]
    fn test_build_hook_script_comment_header() {
        let profile = make_profile(vec!["cargo", "test"]);
        let script = build_hook_script(false, &profile);
        assert!(script.contains("Cogent pre-commit hook"));
        assert!(script.contains("cogent install-hooks"));
        assert!(script.contains("cogent uninstall-hooks"));
        assert!(script.contains("git commit --no-verify"));
    }

    // ── Windows hook variant ─────────────────────────────────────────────

    #[test]
    fn test_windows_hook_fast_mode_contains_no_cache() {
        let script = build_hook_template_windows(true, "cargo test", true, "Rust");
        assert!(script.contains("--no-cache"));
        assert!(script.contains("--format text"));
        assert!(!script.contains("Running tests"));
    }

    #[test]
    fn test_windows_hook_full_mode_contains_no_cache() {
        let script = build_hook_template_windows(false, "cargo test", true, "Rust");
        assert!(script.contains("--no-cache"));
        assert!(script.contains("Running tests (Rust)"));
        assert!(script.contains("Running quality checks"));
        assert!(script.contains("cargo test"));
    }

    #[test]
    fn test_windows_hook_no_coverage_falls_back_to_fast() {
        // When coverage is unavailable, full mode should behave like fast mode
        let script = build_hook_template_windows(false, "cargo test", false, "Rust");
        assert!(script.contains("--no-cache"));
        assert!(!script.contains("Running tests"));
    }

    #[test]
    fn test_windows_hook_comment_header() {
        let script = build_hook_template_windows(false, "cargo test", true, "Rust");
        assert!(script.contains("Cogent pre-commit hook"));
        assert!(script.contains("cogent install-hooks"));
        assert!(script.contains("cogent uninstall-hooks"));
        assert!(script.contains("git commit --no-verify"));
        assert!(script.contains("@echo off"));
    }

    #[test]
    fn test_windows_hook_cmd_bin_detection() {
        let script = build_hook_template_windows(true, "cargo test", true, "Rust");
        assert!(script.contains("where cogent"));
        assert!(script.contains("target\\release\\cogent.exe"));
    }

    #[test]
    fn test_windows_hook_empty_test_cmd() {
        // Edge case: no test command configured
        let script = build_hook_template_windows(false, "", false, "Unknown");
        assert!(script.contains("--no-cache"));
        assert!(!script.contains("Running tests"));
    }
}
