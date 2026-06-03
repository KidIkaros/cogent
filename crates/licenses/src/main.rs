#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "licenses",
    about = "OSS license compliance — scan Cargo.lock/package-lock.json/requirements.txt for GPL, AGPL, unknown, or policy-violating licenses"
)]
struct Cli {
    /// Project root path to scan
    path: String,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Comma-separated list of denied license types (default: AGPL,GPL-3.0,GPL-2.0)
    #[arg(long, default_value = "AGPL-3.0,AGPL-1.0,GPL-3.0,GPL-2.0,GPL-1.0")]
    deny: String,

    /// Fail if any unknown licenses are found
    #[arg(long)]
    deny_unknown: bool,

    /// Max policy violations allowed (default: 0)
    #[arg(long, default_value = "0")]
    max_violations: usize,
}

#[derive(Debug, Clone, Serialize)]
struct LicenseFinding {
    package: String,
    version: String,
    license: String,
    license_category: String,
    violation: bool,
    violation_reason: String,
    ecosystem: String,
}

#[derive(Serialize)]
struct LicenseReport {
    findings: Vec<LicenseFinding>,
    summary: LicenseSummary,
}

#[derive(Serialize)]
struct LicenseSummary {
    packages_scanned: usize,
    violations: usize,
    unknown_licenses: usize,
    copyleft: usize,
    permissive: usize,
    max_violations_threshold: usize,
    deny_list: Vec<String>,
}

/// Classify a license string into a category
fn classify_license(lic: &str) -> &'static str {
    let l = lic.trim().to_uppercase();
    if l.contains("AGPL") {
        return "copyleft-strong";
    }
    if l.contains("LGPL") {
        return "copyleft-weak";
    }
    if l.contains("GPL-3") || l.contains("GPL-2") || l.contains("GPL-1") || l == "GPL" {
        return "copyleft-strong";
    }
    if l.contains("MPL") || l.contains("EUPL") || l.contains("CDDL") || l.contains("EPL") {
        return "copyleft-weak";
    }
    if l.contains("MIT")
        || l.contains("BSD")
        || l.contains("APACHE")
        || l.contains("ISC")
        || l.contains("ZLIB")
        || l.contains("WTFPL")
        || l.contains("PSF")
        || l.contains("CC0")
        || l.contains("UNLICENSE")
        || l.contains("0BSD")
        || l.contains("BOOST")
    {
        return "permissive";
    }
    if l.is_empty() || l == "UNKNOWN" || l == "NONE" {
        return "unknown";
    }
    "other"
}

/// Parse Cargo.lock — extracts (name, version, license) triples.
/// Cargo.lock doesn't include license info directly; we parse `Cargo.toml` files in the registry
/// or fall back to a best-effort approach from Cargo.lock package entries.
fn parse_cargo_lock(path: &Path) -> Vec<(String, String, String)> {
    let lock_path = path.join("Cargo.lock");
    let Ok(content) = std::fs::read_to_string(&lock_path) else {
        return vec![];
    };

    let mut packages = Vec::new();
    let mut name = String::new();
    let mut version = String::new();

    for line in content.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            name.clear();
            version.clear();
        } else if let Some(v) = t.strip_prefix("name = \"") {
            name = v.trim_end_matches('"').to_string();
        } else if let Some(v) = t.strip_prefix("version = \"") {
            version = v.trim_end_matches('"').to_string();
            if !name.is_empty() {
                packages.push((name.clone(), version.clone(), "unknown".to_string()));
            }
        }
    }

    // Try to read license from workspace Cargo.toml metadata for known packages
    // Best effort: scan all Cargo.toml files under the workspace for their license fields
    let mut license_map: HashMap<String, String> = HashMap::new();
    collect_workspace_licenses(path, &mut license_map);

    packages
        .into_iter()
        .map(|(n, v, _)| {
            let lic = license_map
                .get(&n)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            (n, v, lic)
        })
        .collect()
}

/// Walk all Cargo.toml files and extract package name + license fields.
fn collect_workspace_licenses(root: &Path, map: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let ep = entry.path();
        if ep.is_dir() {
            collect_workspace_licenses(&ep, map);
        } else if ep.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&ep) {
                let mut pkg_name = String::new();
                let mut in_pkg = false;
                for line in content.lines() {
                    let t = line.trim();
                    if t == "[package]" {
                        in_pkg = true;
                    } else if t.starts_with('[') && t != "[package]" {
                        in_pkg = false;
                    }
                    if !in_pkg {
                        continue;
                    }
                    if let Some(v) = t.strip_prefix("name = \"") {
                        pkg_name = v.trim_end_matches('"').to_string();
                    } else if let Some(v) = t.strip_prefix("license = \"") {
                        let lic = v.trim_end_matches('"').to_string();
                        if !pkg_name.is_empty() {
                            map.insert(pkg_name.clone(), lic);
                        }
                    }
                }
            }
        }
    }
}

/// Parse package.json in node_modules by reading each package.json under node_modules.
fn parse_npm_licenses(path: &Path) -> Vec<(String, String, String)> {
    let nm = path.join("node_modules");
    if !nm.exists() {
        // Fall back to parsing package.json dependencies if node_modules isn't present
        let pkg = path.join("package.json");
        let Ok(content) = std::fs::read_to_string(&pkg) else {
            return vec![];
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            return vec![];
        };
        let mut results = Vec::new();
        for section in &["dependencies", "devDependencies"] {
            if let Some(deps) = v.get(section).and_then(|d| d.as_object()) {
                for (name, _ver) in deps {
                    results.push((name.clone(), "?".to_string(), "unknown".to_string()));
                }
            }
        }
        return results;
    }

    let Ok(entries) = std::fs::read_dir(&nm) else {
        return vec![];
    };
    let mut results = Vec::new();
    for entry in entries.flatten() {
        let pkg_json = entry.path().join("package.json");
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                let name = v
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("?")
                    .to_string();
                let ver = v
                    .get("version")
                    .and_then(|n| n.as_str())
                    .unwrap_or("?")
                    .to_string();
                let lic = v
                    .get("license")
                    .and_then(|l| l.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                results.push((name, ver, lic));
            }
        }
    }
    results
}

/// Parse requirements.txt — extract package names (no version info, license unknown without pip).
fn parse_python_requirements(path: &Path) -> Vec<(String, String, String)> {
    let req = path.join("requirements.txt");
    let Ok(content) = std::fs::read_to_string(&req) else {
        return vec![];
    };
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let name = l
                .split(&['=', '>', '<', '!', '[', ';'][..])
                .next()
                .unwrap_or(l)
                .trim();
            (name.to_string(), "?".to_string(), "unknown".to_string())
        })
        .collect()
}

fn run(cli: Cli) {
    let root = Path::new(&cli.path);
    let deny_list: Vec<String> = cli
        .deny
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .collect();

    let mut all_packages: Vec<(String, String, String, &'static str)> = Vec::new(); // (name, ver, license, ecosystem)

    // Detect ecosystems
    let has_cargo = root.join("Cargo.lock").exists();
    let has_npm = root.join("package.json").exists();
    let has_python = root.join("requirements.txt").exists() || root.join("Pipfile").exists();

    if has_cargo {
        for (n, v, l) in parse_cargo_lock(root) {
            all_packages.push((n, v, l, "rust"));
        }
    }
    if has_npm {
        for (n, v, l) in parse_npm_licenses(root) {
            all_packages.push((n, v, l, "node"));
        }
    }
    if has_python {
        for (n, v, l) in parse_python_requirements(root) {
            all_packages.push((n, v, l, "python"));
        }
    }

    let mut findings: Vec<LicenseFinding> = Vec::new();
    for (name, ver, lic, eco) in &all_packages {
        let cat = classify_license(lic);
        let lic_upper = lic.trim().to_uppercase();
        let is_denied = deny_list.iter().any(|d| lic_upper.contains(d.as_str()));
        let is_unknown = cat == "unknown";
        let violation = is_denied || (cli.deny_unknown && is_unknown);
        let reason = if is_denied {
            format!("License '{}' is in the deny list", lic)
        } else if cli.deny_unknown && is_unknown {
            format!("Unknown license for package '{}'", name)
        } else {
            String::new()
        };

        if violation || cat == "copyleft-strong" || cat == "copyleft-weak" {
            findings.push(LicenseFinding {
                package: name.clone(),
                version: ver.clone(),
                license: lic.clone(),
                license_category: cat.to_string(),
                violation,
                violation_reason: reason,
                ecosystem: eco.to_string(),
            });
        }
    }

    findings.sort_by(|a, b| {
        let vord = |f: &LicenseFinding| if f.violation { 0u8 } else { 1 };
        vord(a).cmp(&vord(b)).then(a.package.cmp(&b.package))
    });

    let violations = findings.iter().filter(|f| f.violation).count();
    let unknown = all_packages
        .iter()
        .filter(|(_, _, l, _)| classify_license(l) == "unknown")
        .count();
    let copyleft = findings
        .iter()
        .filter(|f| f.license_category.starts_with("copyleft"))
        .count();
    let permissive = all_packages
        .iter()
        .filter(|(_, _, l, _)| classify_license(l) == "permissive")
        .count();

    let summary = LicenseSummary {
        packages_scanned: all_packages.len(),
        violations,
        unknown_licenses: unknown,
        copyleft,
        permissive,
        max_violations_threshold: cli.max_violations,
        deny_list: deny_list.clone(),
    };

    match cli.format.as_str() {
        "json" => {
            let report = LicenseReport { findings, summary };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for f in &findings {
                println!("{}", serde_json::to_string(f).unwrap());
            }
        }
        _ => {
            if findings.is_empty() {
                println!(
                    "No license compliance issues detected ({} packages scanned).",
                    all_packages.len()
                );
            } else {
                let cols = vec![
                    Column {
                        header: "Package",
                        width: 28,
                        align_right: false,
                    },
                    Column {
                        header: "Version",
                        width: 12,
                        align_right: false,
                    },
                    Column {
                        header: "License",
                        width: 22,
                        align_right: false,
                    },
                    Column {
                        header: "Category",
                        width: 16,
                        align_right: false,
                    },
                    Column {
                        header: "Ecosystem",
                        width: 9,
                        align_right: false,
                    },
                    Column {
                        header: "Status",
                        width: 10,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for f in &findings {
                    let status = if f.violation { "VIOLATION" } else { "review" };
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&f.package, 28),
                            &truncate(&f.version, 12),
                            &truncate(&f.license, 22),
                            &truncate(&f.license_category, 16),
                            &f.ecosystem,
                            status,
                        ],
                    );
                }
            }
            let status = if violations <= cli.max_violations {
                "PASS"
            } else {
                "FAIL"
            };
            println!(
                "\nSummary: {} packages  |  {} violations  |  {} copyleft  |  {} unknown  — {}",
                all_packages.len(),
                violations,
                copyleft,
                unknown,
                status
            );
        }
    }

    if violations > cli.max_violations {
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
    fn test_classify_permissive() {
        assert_eq!(classify_license("MIT"), "permissive");
        assert_eq!(classify_license("Apache-2.0"), "permissive");
        assert_eq!(classify_license("BSD-3-Clause"), "permissive");
        assert_eq!(classify_license("ISC"), "permissive");
    }

    #[test]
    fn test_classify_copyleft() {
        assert_eq!(classify_license("GPL-3.0"), "copyleft-strong");
        assert_eq!(classify_license("AGPL-3.0"), "copyleft-strong");
        assert_eq!(classify_license("GPL-2.0"), "copyleft-strong");
    }

    #[test]
    fn test_classify_weak_copyleft() {
        assert_eq!(classify_license("LGPL-2.1"), "copyleft-weak");
        assert_eq!(classify_license("MPL-2.0"), "copyleft-weak");
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(classify_license(""), "unknown");
        assert_eq!(classify_license("unknown"), "unknown");
    }

    use super::*;

    // ── parse_cargo_lock ──

    #[test]
    fn test_parse_cargo_lock_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let pkgs = parse_cargo_lock(dir.path());
        assert!(pkgs.is_empty());
    }

    #[test]
    fn test_parse_cargo_lock_with_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), r#"
[[package]]
name = "serde"
version = "1.0.0"

[[package]]
name = "tokio"
version = "0.2.0"
"#).unwrap();
        let pkgs = parse_cargo_lock(dir.path());
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].0, "serde");
        assert_eq!(pkgs[1].0, "tokio");
    }

    // ── parse_python_requirements ──

    #[test]
    fn test_parse_python_requirements_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let pkgs = parse_python_requirements(dir.path());
        assert!(pkgs.is_empty());
    }

    #[test]
    fn test_parse_python_requirements_with_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests>=2.0.0\nflask==1.0\n# comment\n").unwrap();
        let pkgs = parse_python_requirements(dir.path());
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].0, "requests");
        assert_eq!(pkgs[1].0, "flask");
    }

    #[test]
    fn test_parse_python_requirements_empty_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "pkg1\n\n# comment\npkg2\n").unwrap();
        let pkgs = parse_python_requirements(dir.path());
        assert_eq!(pkgs.len(), 2);
    }

    // ── collect_workspace_licenses ──

    #[test]
    fn test_collect_workspace_licenses_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut map = std::collections::HashMap::new();
        collect_workspace_licenses(dir.path(), &mut map);
        assert!(map.is_empty());
    }

    #[test]
    fn test_collect_workspace_licenses_with_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), r#"
[package]
name = "my-crate"
license = "MIT"
"#).unwrap();
        let mut map = std::collections::HashMap::new();
        collect_workspace_licenses(dir.path(), &mut map);
        assert_eq!(map.get("my-crate").map(|s| s.as_str()), Some("MIT"));
    }

    // ── parse_npm_licenses ──

    #[test]
    fn test_parse_npm_licenses_no_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        // No node_modules, no package.json -> empty
        let pkgs = parse_npm_licenses(dir.path());
        assert!(pkgs.is_empty());
    }

    #[test]
    fn test_parse_npm_licenses_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{
  "dependencies": {"lodash": "^4.17.0"},
  "devDependencies": {"mocha": "^9.0.0"}
}"#).unwrap();
        let pkgs = parse_npm_licenses(dir.path());
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].0, "lodash");
    }

    // ── classify_license extensions ──

    #[test]
    fn test_classify_license_copyleft_weak_lgpl() {
        assert_eq!(classify_license("LGPL-3.0"), "copyleft-weak");
    }

    #[test]
    fn test_classify_license_other() {
        assert_eq!(classify_license("Proprietary"), "other");
    }

    #[test]
    fn test_classify_license_case_insensitive_matches() {
        assert_eq!(classify_license("mit"), "permissive");
        assert_eq!(classify_license("gpl-3.0"), "copyleft-strong");
    }
}
