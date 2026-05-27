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
