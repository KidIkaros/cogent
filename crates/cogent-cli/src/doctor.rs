//! `cogent doctor` — collect diagnostic info for support scenarios.

use colored::Colorize;
use serde_json::json;
use std::collections::HashMap;

/// Gather diagnostic information: versions, config, PATH, available binaries, OS info.
pub fn collect_diagnostics() -> serde_json::Value {
    let mut binaries = HashMap::new();
    for bin in &[
        "cogent",
        "cargo",
        "rustc",
        "git",
        "clippy-driver",
    ] {
        binaries.insert(
            *bin,
            which(bin).unwrap_or_else(|| "not found".to_string()),
        );
    }

    let mut config = json!({});
    if let Ok(content) = std::fs::read_to_string(".quality.toml") {
        config = json!({ "present": true, "size_bytes": content.len() });
    } else {
        config = json!({ "present": false });
    }

    json!({
        "cogent_version": env!("CARGO_PKG_VERSION"),
        "rust_version": rustc_version(),
        "cargo_version": cargo_version(),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "path": std::env::var("PATH").unwrap_or_default(),
        "cwd": std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        "config": config,
        "binaries": binaries,
        "git_repo": git_repo_info(),
    })
}

/// Run the doctor command and print diagnostics to stdout.
pub fn doctor_command(format: &str) -> i32 {
    let diagnostics = collect_diagnostics();
    match format {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&diagnostics).unwrap_or_default()
            );
        }
        _ => {
            // Pretty human-readable output
            println!("  {}", "Cogent Diagnostic Report".bold());
            println!();
            println!(
                "  {} Cogent version: {}",
                "•".cyan(),
                diagnostics["cogent_version"].as_str().unwrap_or("unknown")
            );
            println!(
                "  {} Rust version:     {}",
                "•".cyan(),
                diagnostics["rust_version"].as_str().unwrap_or("unknown")
            );
            println!(
                "  {} Platform:         {} ({})",
                "•".cyan(),
                diagnostics["platform"].as_str().unwrap_or("unknown"),
                diagnostics["arch"].as_str().unwrap_or("unknown")
            );
            println!();
            println!("  {}", "Available binaries:".bold());
            if let Some(map) = diagnostics["binaries"].as_object() {
                for (name, path) in map {
                    let path_str = path.as_str().unwrap_or("unknown");
                    let icon = if path_str == "not found" {
                        "✗".red()
                    } else {
                        "✓".green()
                    };
                    println!("    {} {:20} {}", icon, name, path_str.bright_black());
                }
            }
            println!();
            println!(
                "  {} Config (.quality.toml): {}",
                "•".cyan(),
                if diagnostics["config"]["present"].as_bool() == Some(true) {
                    "present".green().to_string()
                } else {
                    "missing".yellow().to_string()
                }
            );
            if let Some(repo) = diagnostics["git_repo"].as_str() {
                println!("  {} Git repo:           {}", "•".cyan(), repo);
            }
        }
    }
    0
}

fn which(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(name);
            if full.is_file() {
                Some(full.to_string_lossy().to_string())
            } else {
                None
            }
        })
    })
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn cargo_version() -> String {
    std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn git_repo_info() -> Option<String> {
    std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}
