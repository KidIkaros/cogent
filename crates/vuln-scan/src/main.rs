#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(
    name = "vulnscan",
    about = "Vulnerability scanner — CVE detection via cargo audit / npm audit / safety"
)]
struct Cli {
    /// Path to the project root to scan
    path: String,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Max critical severity CVEs allowed (default: 0)
    #[arg(long, default_value = "0")]
    max_critical: usize,

    /// Max high severity CVEs allowed (default: 0)
    #[arg(long, default_value = "0")]
    max_high: usize,

    /// Force ecosystem detection: rust, node, python (auto-detected by default)
    #[arg(long)]
    ecosystem: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Vulnerability {
    package: String,
    version: String,
    advisory_id: String,
    severity: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    ecosystem: String,
}

#[derive(Serialize)]
struct VulnReport {
    vulnerabilities: Vec<Vulnerability>,
    summary: VulnSummary,
}

#[derive(Serialize)]
struct VulnSummary {
    ecosystem: String,
    tool_used: String,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    unknown: usize,
    total: usize,
    max_critical_threshold: usize,
    max_high_threshold: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Ecosystem {
    Rust,
    Node,
    Python,
    Unknown,
}

impl std::fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ecosystem::Rust => write!(f, "rust"),
            Ecosystem::Node => write!(f, "node"),
            Ecosystem::Python => write!(f, "python"),
            Ecosystem::Unknown => write!(f, "unknown"),
        }
    }
}

fn detect_ecosystem(path: &str, forced: Option<&str>) -> Ecosystem {
    if let Some(eco) = forced {
        return match eco.to_lowercase().as_str() {
            "rust" => Ecosystem::Rust,
            "node" | "nodejs" | "js" | "npm" => Ecosystem::Node,
            "python" | "py" => Ecosystem::Python,
            _ => Ecosystem::Unknown,
        };
    }
    let p = Path::new(path);
    if p.join("Cargo.toml").exists() {
        return Ecosystem::Rust;
    }
    if p.join("package.json").exists() {
        return Ecosystem::Node;
    }
    if p.join("requirements.txt").exists()
        || p.join("Pipfile").exists()
        || p.join("pyproject.toml").exists()
    {
        return Ecosystem::Python;
    }
    Ecosystem::Unknown
}

fn find_tool(names: &[&str]) -> Option<String> {
    for name in names {
        if Command::new(name)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Some(name.to_string());
        }
    }
    None
}

fn run_cargo_audit(path: &str) -> Result<Vec<Vulnerability>, String> {
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());

    // Always invoke as `cargo audit --json` (cargo-audit plugin)
    let output = Command::new("cargo")
        .args(["audit", "--json"])
        .current_dir(&canonical)
        .output()
        .map_err(|e| format!("Failed to run cargo audit: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Empty output means cargo-audit not installed
    if stdout.trim().is_empty() {
        return Err(
            "cargo audit produced no output. Install with: cargo install cargo-audit".to_string(),
        );
    }

    // cargo audit JSON: { "vulnerabilities": { "list": [...] } }
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse cargo audit output: {}", e))?;

    let list = v
        .get("vulnerabilities")
        .and_then(|x| x.get("list"))
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    let mut vulns = Vec::new();
    for item in list {
        let pkg = item
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let ver = item
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|n| n.as_str())
            .unwrap_or("?");
        let adv = item.get("advisory").unwrap_or(&serde_json::Value::Null);
        let id = adv.get("id").and_then(|n| n.as_str()).unwrap_or("?");
        let title = adv.get("title").and_then(|n| n.as_str()).unwrap_or("?");
        let url = adv
            .get("url")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        // cargo audit doesn't always have severity; derive from CVSS if present
        let severity = adv
            .get("cvss")
            .and_then(|c| c.as_str())
            .map(|s| {
                // CVSS v3 base score is in the string "CVSS:3.1/AV:N/... /E:..."
                // Use presence of "CRITICAL" or "HIGH" words or score-based heuristic
                if s.contains("9.") || s.contains("10.") {
                    "critical"
                } else if s.contains("7.") || s.contains("8.") {
                    "high"
                } else if s.contains("4.") || s.contains("5.") || s.contains("6.") {
                    "medium"
                } else {
                    "low"
                }
            })
            .unwrap_or("unknown")
            .to_string();

        vulns.push(Vulnerability {
            package: pkg.to_string(),
            version: ver.to_string(),
            advisory_id: id.to_string(),
            severity,
            title: title.to_string(),
            url,
            ecosystem: "rust".to_string(),
        });
    }
    Ok(vulns)
}

fn run_npm_audit(path: &str) -> Result<Vec<Vulnerability>, String> {
    let tool = find_tool(&["npm", "yarn"])
        .ok_or_else(|| "npm/yarn not found. Install Node.js to enable npm audit.".to_string())?;
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());

    let output = Command::new(&tool)
        .args(["audit", "--json"])
        .current_dir(&canonical)
        .output()
        .map_err(|e| format!("Failed to run {} audit: {}", tool, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse npm audit output: {}", e))?;

    let mut vulns = Vec::new();

    // npm audit v2 JSON: { "vulnerabilities": { "pkg": { "severity", "via": [...] } } }
    if let Some(vmap) = v.get("vulnerabilities").and_then(|x| x.as_object()) {
        for (pkg_name, pkg_data) in vmap {
            let severity = pkg_data
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let via = pkg_data
                .get("via")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            for item in &via {
                if item.is_object() {
                    let id = item
                        .get("source")
                        .and_then(|s| s.as_u64())
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".to_string());
                    let title = item.get("title").and_then(|s| s.as_str()).unwrap_or("?");
                    let url = item
                        .get("url")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string());
                    vulns.push(Vulnerability {
                        package: pkg_name.clone(),
                        version: pkg_data
                            .get("range")
                            .and_then(|s| s.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        advisory_id: id,
                        severity: severity.to_string(),
                        title: title.to_string(),
                        url,
                        ecosystem: "node".to_string(),
                    });
                }
            }
        }
    }
    Ok(vulns)
}

fn run_safety(path: &str) -> Result<Vec<Vulnerability>, String> {
    let _ = find_tool(&["safety"])
        .ok_or_else(|| "safety not found. Install with: pip install safety".to_string())?;
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());

    let output = Command::new("safety")
        .args(["check", "--json"])
        .current_dir(&canonical)
        .output()
        .map_err(|e| format!("Failed to run safety: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse safety output: {}", e))?;

    let mut vulns = Vec::new();
    // safety JSON: array of [package, specs, installed_version, advisory, vuln_id, ...]
    if let Some(arr) = v.as_array() {
        for item in arr {
            if let Some(arr2) = item.as_array() {
                let pkg = arr2.first().and_then(|v| v.as_str()).unwrap_or("?");
                let ver = arr2.get(2).and_then(|v| v.as_str()).unwrap_or("?");
                let adv = arr2.get(3).and_then(|v| v.as_str()).unwrap_or("?");
                let id = arr2.get(4).and_then(|v| v.as_str()).unwrap_or("?");
                vulns.push(Vulnerability {
                    package: pkg.to_string(),
                    version: ver.to_string(),
                    advisory_id: id.to_string(),
                    severity: "unknown".to_string(),
                    title: adv.chars().take(100).collect(),
                    url: None,
                    ecosystem: "python".to_string(),
                });
            }
        }
    }
    Ok(vulns)
}

fn run(cli: Cli) {
    let path = &cli.path;
    let eco = detect_ecosystem(path, cli.ecosystem.as_deref());

    let (vulns, tool_used) = match eco {
        Ecosystem::Rust => match run_cargo_audit(path) {
            Ok(v) => (v, "cargo audit".to_string()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        Ecosystem::Node => match run_npm_audit(path) {
            Ok(v) => (v, "npm audit".to_string()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        Ecosystem::Python => match run_safety(path) {
            Ok(v) => (v, "safety".to_string()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        Ecosystem::Unknown => {
            eprintln!(
                "Error: Could not detect ecosystem in '{}'. Use --ecosystem rust|node|python.",
                path
            );
            std::process::exit(2);
        }
    };

    let critical = vulns.iter().filter(|v| v.severity == "critical").count();
    let high = vulns.iter().filter(|v| v.severity == "high").count();
    let medium = vulns.iter().filter(|v| v.severity == "medium").count();
    let low = vulns.iter().filter(|v| v.severity == "low").count();
    let unknown = vulns.iter().filter(|v| v.severity == "unknown").count();

    let summary = VulnSummary {
        ecosystem: eco.to_string(),
        tool_used: tool_used.clone(),
        critical,
        high,
        medium,
        low,
        unknown,
        total: vulns.len(),
        max_critical_threshold: cli.max_critical,
        max_high_threshold: cli.max_high,
    };

    match cli.format.as_str() {
        "json" => {
            let report = VulnReport {
                vulnerabilities: vulns,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for v in &vulns {
                println!("{}", serde_json::to_string(v).unwrap());
            }
        }
        _ => {
            if vulns.is_empty() {
                println!("No known vulnerabilities found ({}).", tool_used);
            } else {
                let cols = vec![
                    Column {
                        header: "Package",
                        width: 25,
                        align_right: false,
                    },
                    Column {
                        header: "Version",
                        width: 12,
                        align_right: false,
                    },
                    Column {
                        header: "Severity",
                        width: 9,
                        align_right: false,
                    },
                    Column {
                        header: "Advisory",
                        width: 15,
                        align_right: false,
                    },
                    Column {
                        header: "Title",
                        width: 50,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for v in &vulns {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&v.package, 25),
                            &truncate(&v.version, 12),
                            &v.severity,
                            &truncate(&v.advisory_id, 15),
                            &truncate(&v.title, 50),
                        ],
                    );
                }
            }
            let status = if critical > cli.max_critical || high > cli.max_high {
                "FAIL"
            } else {
                "PASS"
            };
            println!(
                "\nSummary: {} total ({} critical, {} high, {} medium, {} low) — {}",
                summary.total, critical, high, medium, low, status
            );
        }
    }

    // Exit 1 if thresholds exceeded
    if critical > cli.max_critical || high > cli.max_high {
        std::process::exit(1);
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ecosystem_rust() {
        // Use the Cogent workspace root — has Cargo.toml
        let eco = detect_ecosystem(".", None);
        assert_eq!(eco, Ecosystem::Rust);
    }

    #[test]
    fn test_detect_ecosystem_forced() {
        let eco = detect_ecosystem(".", Some("python"));
        assert_eq!(eco, Ecosystem::Python);
    }

    #[test]
    fn test_detect_ecosystem_unknown() {
        let eco = detect_ecosystem("/tmp", None);
        assert_eq!(eco, Ecosystem::Unknown);
    }

    #[test]
    fn test_ecosystem_display() {
        assert_eq!(Ecosystem::Rust.to_string(), "rust");
        assert_eq!(Ecosystem::Node.to_string(), "node");
        assert_eq!(Ecosystem::Python.to_string(), "python");
    }
}
