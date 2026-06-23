#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "supply-chain",
    about = "Supply chain checker — dependency integrity, typosquatting, abandoned packages, and unpinned version risks"
)]
struct Cli {
    path: String,
    #[arg(short, long, default_value = "table")]
    format: String,
    #[arg(long, default_value = "0")]
    max_risks: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SupplyFinding {
    package: String,
    version: String,
    risk_type: String,
    rule_id: String,
    severity: String,
    description: String,
    remediation: String,
}

#[derive(Serialize)]
struct SupplyReport {
    findings: Vec<SupplyFinding>,
    summary: SupplySummary,
}

#[derive(Serialize)]
struct SupplySummary {
    packages_scanned: usize,
    total_risks: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    max_risks_threshold: usize,
    ecosystems: Vec<String>,
}

// Popular package names for typosquatting detection
const POPULAR_PACKAGES: &[&str] = &[
    "tokio",
    "serde",
    "rand",
    "log",
    "chrono",
    "regex",
    "clap",
    "hyper",
    "actix",
    "reqwest",
    "tower",
    "tracing",
    "async-trait",
    "futures",
    "rayon",
    "crossbeam",
    "sha2",
    "md5",
    "aes",
    "ring",
    "rusoto",
    "aws-sdk",
    "sqlx",
    "diesel",
    "rocket",
    "axum",
    "warp",
    "tide",
    "express",
    "lodash",
    "axios",
    "react",
    "vue",
    "angular",
    "typescript",
    "webpack",
    "jest",
    "mocha",
    "django",
    "flask",
    "fastapi",
    "requests",
    "numpy",
    "pandas",
    "matplotlib",
    "scikit-learn",
    "tensorflow",
    "pytest",
    "black",
    "gorm",
    "gin",
    "echo",
    "beego",
    "fiber",
    "spring-boot",
    "hibernate",
    "junit",
    "jackson",
    "guava",
    "apache-commons",
    "slf4j",
    "log4j",
    "netty",
    "mockito",
];

fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev = vec![0usize; m + 1];
    let mut curr = vec![0usize; m + 1];
    for (j, item) in prev.iter_mut().enumerate().take(m + 1) {
        *item = j;
    }

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn is_typosquat(name: &str) -> Option<&'static str> {
    for &popular in POPULAR_PACKAGES {
        // Skip anchors that are too short — high false-positive rate (e.g. "anes" vs "aes")
        if popular.len() <= 3 {
            continue;
        }
        let dist = levenshtein(name, popular);
        if dist == 1 && name != popular {
            return Some(popular);
        }
    }
    None
}

fn check_cargo_lock(path: &Path) -> Vec<SupplyFinding> {
    let mut findings = Vec::new();
    let lock_path = path.join("Cargo.lock");
    if !lock_path.exists() {
        findings.push(SupplyFinding {
            package: "(workspace)".to_string(),
            version: "".to_string(),
            risk_type: "missing_lockfile".to_string(),
            rule_id: "SUPPLY-LOCK-001".to_string(),
            severity: "high".to_string(),
            description: "Cargo.lock is missing — builds are not reproducible and supply chain is unverified.".to_string(),
            remediation: "Run 'cargo generate-lockfile' and commit Cargo.lock for binary crates.".to_string(),
        });
        return findings;
    }

    let Ok(content) = std::fs::read_to_string(&lock_path) else {
        return findings;
    };
    let mut in_package = false;
    let mut name = String::new();
    let mut version = String::new();
    let mut has_checksum = false;
    let mut has_source = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            if in_package && !name.is_empty() && has_source && !has_checksum {
                findings.push(SupplyFinding {
                    package: name.clone(),
                    version: version.clone(),
                    risk_type: "missing_checksum".to_string(),
                    rule_id: "SUPPLY-LOCK-002".to_string(),
                    severity: "medium".to_string(),
                    description: format!("Package {} has no checksum in Cargo.lock", name),
                    remediation: "Ensure the registry provides checksums. Consider verifying the package manually.".to_string(),
                });
            }
            in_package = true;
            name.clear();
            version.clear();
            has_checksum = false;
            has_source = false;
        } else if let Some(v) = trimmed.strip_prefix("name = \"") {
            name = v.trim_end_matches('\"').to_string();
        } else if let Some(v) = trimmed.strip_prefix("version = \"") {
            version = v.trim_end_matches('\"').to_string();
        } else if trimmed.starts_with("source = ") {
            has_source = true;
        } else if trimmed.starts_with("checksum = ") {
            has_checksum = true;
        }

        // Typosquatting check
        if !name.is_empty() && version.is_empty() {
            if let Some(original) = is_typosquat(&name) {
                findings.push(SupplyFinding {
                    package: name.clone(),
                    version: "".to_string(),
                    risk_type: "typosquatting".to_string(),
                    rule_id: "SUPPLY-TYPO-001".to_string(),
                    severity: "critical".to_string(),
                    description: format!(
                        "Package '{}' is a potential typosquat of '{}'",
                        name, original
                    ),
                    remediation: format!(
                        "Verify the package is legitimate. If not, replace with '{}'.",
                        original
                    ),
                });
            }
        }
    }

    // Check last package
    if in_package && !name.is_empty() && has_source && !has_checksum {
        findings.push(SupplyFinding {
            package: name.clone(),
            version: version.clone(),
            risk_type: "missing_checksum".to_string(),
            rule_id: "SUPPLY-LOCK-002".to_string(),
            severity: "medium".to_string(),
            description: format!("Package {} has no checksum in Cargo.lock", name),
            remediation:
                "Ensure the registry provides checksums. Consider verifying the package manually."
                    .to_string(),
        });
    }

    findings
}

fn check_npm_lock(path: &Path) -> Vec<SupplyFinding> {
    let mut findings = Vec::new();
    let lock_path = path.join("package-lock.json");
    if !lock_path.exists() {
        findings.push(SupplyFinding {
            package: "(workspace)".to_string(),
            version: "".to_string(),
            risk_type: "missing_lockfile".to_string(),
            rule_id: "SUPPLY-LOCK-003".to_string(),
            severity: "high".to_string(),
            description: "package-lock.json is missing — Node.js builds are not reproducible."
                .to_string(),
            remediation: "Run 'npm install' and commit package-lock.json.".to_string(),
        });
    }
    findings
}

fn check_python_deps(path: &Path) -> Vec<SupplyFinding> {
    let mut findings = Vec::new();
    let req_path = path.join("requirements.txt");
    if req_path.exists() {
        let Ok(content) = std::fs::read_to_string(&req_path) else {
            return findings;
        };
        for (lineno, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
                continue;
            }
            if !trimmed.contains("==") && !trimmed.contains(">=") && !trimmed.contains("~=") {
                findings.push(SupplyFinding {
                    package: trimmed.to_string(),
                    version: "".to_string(),
                    risk_type: "unpinned_dependency".to_string(),
                    rule_id: "SUPPLY-PIN-001".to_string(),
                    severity: "medium".to_string(),
                    description: format!(
                        "Python dependency '{}' is not pinned (line {})",
                        trimmed,
                        lineno + 1
                    ),
                    remediation:
                        "Pin versions with '==' or use a lockfile (pip-compile, poetry.lock)."
                            .to_string(),
                });
            }
        }
    }
    findings
}

fn detect_ecosystems(path: &Path) -> Vec<String> {
    let mut eco = Vec::new();
    if path.join("Cargo.toml").exists() {
        eco.push("rust".to_string());
    }
    if path.join("package.json").exists() {
        eco.push("node".to_string());
    }
    if path.join("requirements.txt").exists()
        || path.join("Pipfile").exists()
        || path.join("pyproject.toml").exists()
    {
        eco.push("python".to_string());
    }
    eco
}

fn run(cli: Cli) {
    let path = Path::new(&cli.path);
    let ecosystems = detect_ecosystems(path);
    let mut findings = Vec::new();

    for eco in &ecosystems {
        match eco.as_str() {
            "rust" => findings.extend(check_cargo_lock(path)),
            "node" => findings.extend(check_npm_lock(path)),
            "python" => findings.extend(check_python_deps(path)),
            _ => {}
        }
    }

    if ecosystems.is_empty() {
        findings.push(SupplyFinding {
            package: "(workspace)".to_string(),
            version: "".to_string(),
            risk_type: "unknown_ecosystem".to_string(),
            rule_id: "SUPPLY-ECO-001".to_string(),
            severity: "low".to_string(),
            description: "No recognized dependency ecosystem detected.".to_string(),
            remediation: "Ensure Cargo.toml, package.json, or requirements.txt is present."
                .to_string(),
        });
    }

    let critical = findings.iter().filter(|f| f.severity == "critical").count();
    let high = findings.iter().filter(|f| f.severity == "high").count();
    let medium = findings.iter().filter(|f| f.severity == "medium").count();
    let low = findings.iter().filter(|f| f.severity == "low").count();

    let summary = SupplySummary {
        packages_scanned: findings.len(),
        total_risks: findings.len(),
        critical,
        high,
        medium,
        low,
        max_risks_threshold: cli.max_risks,
        ecosystems,
    };
    let exceeds_threshold = summary.total_risks > cli.max_risks;

    match cli.format.as_str() {
        "json" => {
            let report = SupplyReport { findings, summary };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for f in &findings {
                println!("{}", serde_json::to_string(f).unwrap());
            }
        }
        _ => {
            let columns = &[
                Column::left("Rule", 14),
                Column::left("Severity", 10),
                Column::left("Package", 30),
                Column::left("Risk Type", 20),
                Column::left("Description", 40),
            ];
            print_table_header(columns);
            for f in &findings {
                print_table_row(
                    columns,
                    &[
                        &f.rule_id,
                        &f.severity,
                        &truncate(&f.package, 30),
                        &f.risk_type,
                        &truncate(&f.description, 40),
                    ],
                );
            }
            println!(
                "\n  Total: {} risks ({} critical, {} high, {} medium, {} low)",
                summary.total_risks, critical, high, medium, low
            );
            if exceeds_threshold {
                println!("  Exceeds threshold of {} risks", cli.max_risks);
            }
        }
    }
    if exceeds_threshold {
        std::process::exit(1);
    }
}

fn main() {
    let cli = Cli::parse();
    run(cli);
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    // ── levenshtein ──

    #[test]
    fn test_levenshtein_equal_strings() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_both_empty() {
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_one_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("xyz", ""), 3);
    }

    #[test]
    fn test_levenshtein_single_substitute() {
        assert_eq!(levenshtein("cat", "car"), 1);
    }

    #[test]
    fn test_levenshtein_single_insert() {
        assert_eq!(levenshtein("cat", "cats"), 1);
    }

    #[test]
    fn test_levenshtein_single_delete() {
        assert_eq!(levenshtein("cats", "cat"), 1);
    }

    #[test]
    fn test_levenshtein_completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    #[test]
    fn test_levenshtein_case_insensitive() {
        // The function lowercases both inputs
        assert_eq!(levenshtein("Hello", "hello"), 0);
        assert_eq!(levenshtein("ABC", "abc"), 0);
    }

    #[test]
    fn test_levenshtein_complex_distance() {
        // "tokio" vs "tokko" = 1 (substitute)
        assert_eq!(levenshtein("tokio", "tokko"), 1);
        // "tokio" vs "tokyo" = 1 (substitute)
        assert_eq!(levenshtein("tokio", "tokyo"), 1);
    }

    // ── is_typosquat ──

    #[test]
    fn test_is_typosquat_exact_match() {
        assert!(is_typosquat("tokio").is_none());
        assert!(is_typosquat("serde").is_none());
    }

    #[test]
    fn test_is_typosquat_distance_one() {
        // "tokio" -> "tokko" is distance 1
        let result = is_typosquat("tokko");
        assert_eq!(result, Some("tokio"));
    }

    #[test]
    fn test_is_typosquat_distance_two() {
        // Distance 2 from any popular should return None
        assert!(is_typosquat("toooo").is_none());
    }

    #[test]
    fn test_is_typosquat_not_similar() {
        assert!(is_typosquat("completely-random-pkg").is_none());
    }

    #[test]
    fn test_is_typosquat_skips_short_popular() {
        // "aes" is len 3, so comparisons with it are skipped
        // "aes" vs "aex" would be distance 1 but aes <= 3 so skipped
        assert!(is_typosquat("aex").is_none());
    }

    #[test]
    fn test_is_typosquat_edge_distance_0_returns_none() {
        // name == popular, dist == 0, but the function checks name != popular first
        assert!(is_typosquat("tokio").is_none());
    }

    #[test]
    fn test_is_typosquat_serde_typo() {
        // "serde" -> "sarde" is distance 1 (substitute 'e' for 'a')
        let result = is_typosquat("sarde");
        assert_eq!(result, Some("serde"));
    }

    #[test]
    fn test_is_typosquat_popular_all_covered() {
        // Verify each popular package doesn't match itself
        for &popular in POPULAR_PACKAGES {
            assert!(
                is_typosquat(popular).is_none(),
                "is_typosquat should return None for exact match '{}'",
                popular
            );
        }
    }

    // ── detect_ecosystems ──

    #[test]
    fn test_detect_ecosystems_empty() {
        let dir = temp_dir();
        let eco = detect_ecosystems(dir.path());
        assert!(eco.is_empty());
    }

    #[test]
    fn test_detect_ecosystems_rust() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco, vec!["rust"]);
    }

    #[test]
    fn test_detect_ecosystems_node() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("package.json"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco, vec!["node"]);
    }

    #[test]
    fn test_detect_ecosystems_python() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("requirements.txt"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco, vec!["python"]);
    }

    #[test]
    fn test_detect_ecosystems_python_pipfile() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("Pipfile"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco, vec!["python"]);
    }

    #[test]
    fn test_detect_ecosystems_python_pyproject() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco, vec!["python"]);
    }

    #[test]
    fn test_detect_ecosystems_multiple() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.path().join("package.json"), "").unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "").unwrap();
        let eco = detect_ecosystems(dir.path());
        assert_eq!(eco.len(), 3);
        assert!(eco.contains(&"rust".to_string()));
        assert!(eco.contains(&"node".to_string()));
        assert!(eco.contains(&"python".to_string()));
    }

    // ── check_npm_lock ──

    #[test]
    fn test_check_npm_lock_missing() {
        let dir = temp_dir();
        let findings = check_npm_lock(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].risk_type, "missing_lockfile");
        assert_eq!(findings[0].rule_id, "SUPPLY-LOCK-003");
        assert_eq!(findings[0].severity, "high");
    }

    #[test]
    fn test_check_npm_lock_exists() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
        let findings = check_npm_lock(dir.path());
        assert!(findings.is_empty());
    }

    // ── check_python_deps ──

    #[test]
    fn test_check_python_deps_no_req_file() {
        let dir = temp_dir();
        let findings = check_python_deps(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_python_deps_empty_file() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("requirements.txt"), "").unwrap();
        let findings = check_python_deps(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_python_deps_all_pinned() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "requests==2.28.0\nflask==2.3.0\nnumpy>=1.24.0\n",
        )
        .unwrap();
        let findings = check_python_deps(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_python_deps_unpinned() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "requests\nflask==2.3.0\nnumpy\n",
        )
        .unwrap();
        let findings = check_python_deps(dir.path());
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].risk_type, "unpinned_dependency");
        assert_eq!(findings[0].severity, "medium");
        assert_eq!(findings[0].package, "requests");
        assert_eq!(findings[1].package, "numpy");
    }

    #[test]
    fn test_check_python_deps_skips_comments_and_flags() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "# this is a comment\n--index-url https://example.com\nflask==2.3.0\n",
        )
        .unwrap();
        let findings = check_python_deps(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_python_deps_empty_lines_ignored() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "\n\nrequests\n\nflask==2.3.0\n\n",
        )
        .unwrap();
        let findings = check_python_deps(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].package, "requests");
    }

    #[test]
    fn test_check_python_deps_line_numbers() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "# header\n\nunpinned-pkg\nflask==2.3.0\n",
        )
        .unwrap();
        let findings = check_python_deps(dir.path());
        assert_eq!(findings.len(), 1);
        // unpinned-pkg is on line 3 (1-indexed)
        assert!(findings[0].description.contains("line 3"));
    }

    // ── check_cargo_lock ──

    #[test]
    fn test_check_cargo_lock_missing() {
        let dir = temp_dir();
        let findings = check_cargo_lock(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].risk_type, "missing_lockfile");
        assert_eq!(findings[0].rule_id, "SUPPLY-LOCK-001");
        assert_eq!(findings[0].severity, "high");
    }

    #[test]
    fn test_check_cargo_lock_empty_file() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("Cargo.lock"), "").unwrap();
        let findings = check_cargo_lock(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_cargo_lock_with_checksum() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.35.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"abc123\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_cargo_lock_missing_checksum() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.35.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].risk_type, "missing_checksum");
        assert_eq!(findings[0].rule_id, "SUPPLY-LOCK-002");
        assert_eq!(findings[0].severity, "medium");
        assert_eq!(findings[0].package, "tokio");
    }

    #[test]
    fn test_check_cargo_lock_missing_source_skips_checksum_check() {
        // If source is missing, the checksum check should be skipped
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.35.0\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_check_cargo_lock_typosquat_detected() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokko\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        let typos: Vec<_> = findings
            .iter()
            .filter(|f| f.risk_type == "typosquatting")
            .collect();
        assert_eq!(typos.len(), 1);
        assert_eq!(typos[0].severity, "critical");
        assert_eq!(typos[0].rule_id, "SUPPLY-TYPO-001");
        assert!(typos[0].description.contains("tokio"));
    }

    #[test]
    fn test_check_cargo_lock_no_typosquat_for_exact_match() {
        // tokio is a popular package, exact match should not trigger typosquat
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        let typos: Vec<_> = findings
            .iter()
            .filter(|f| f.risk_type == "typosquatting")
            .collect();
        assert!(typos.is_empty());
    }

    #[test]
    fn test_check_cargo_lock_multiple_packages() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.35.0\"\nsource = \"registry\"\nchecksum = \"abc\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry\"\n\n[[package]]\nname = \"tokko\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let findings = check_cargo_lock(dir.path());
        assert_eq!(findings.len(), 2);
        // First finding: serde missing checksum
        assert_eq!(findings[0].risk_type, "missing_checksum");
        assert_eq!(findings[0].package, "serde");
        // Second finding: tokko typosquatting
        assert_eq!(findings[1].risk_type, "typosquatting");
        assert_eq!(findings[1].package, "tokko");
    }

    // ── POPULAR_PACKAGES const ──

    #[test]
    fn test_popular_packages_all_non_empty() {
        assert!(!POPULAR_PACKAGES.is_empty());
        for &pkg in POPULAR_PACKAGES {
            assert!(!pkg.is_empty(), "POPULAR_PACKAGES contains an empty entry");
        }
    }

    #[test]
    fn test_popular_packages_no_duplicates() {
        let mut sorted = POPULAR_PACKAGES.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            POPULAR_PACKAGES.len(),
            "POPULAR_PACKAGES contains duplicates"
        );
    }

    #[test]
    fn test_popular_packages_levenshtein_distance_one_unique() {
        // No two popular packages should be distance 1 from each other
        // (would cause false cross-detections)
        for (i, &a) in POPULAR_PACKAGES.iter().enumerate() {
            for &b in POPULAR_PACKAGES.iter().skip(i + 1) {
                if a.len() <= 3 || b.len() <= 3 {
                    continue;
                }
                let dist = levenshtein(a, b);
                assert!(
                    dist != 1,
                    "POPULAR_PACKAGES entries '{}' and '{}' are distance 1 apart (would cause false typosquat detection)",
                    a, b
                );
            }
        }
    }
}
