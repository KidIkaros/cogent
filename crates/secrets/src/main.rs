#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "secrets",
    about = "Hardcoded secrets scanner — detect API keys, passwords, and high-entropy strings"
)]
struct Cli {
    /// Path to scan (file or directory)
    path: String,

    /// Recursive scan
    #[arg(short, long)]
    recursive: bool,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Minimum Shannon entropy threshold for string literal flagging (default: 4.5)
    #[arg(long, default_value = "4.5")]
    min_entropy: f64,

    /// Also scan binary/config files (.env, .json, .yaml, .toml, .xml)
    #[arg(long)]
    include_config: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SecretFinding {
    file: String,
    line: usize,
    kind: String,
    pattern: String,
    context: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    entropy: Option<f64>,
    severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct SecretsReport {
    findings: Vec<SecretFinding>,
    summary: SecretsSummary,
}

#[derive(Serialize)]
struct SecretsSummary {
    files_scanned: usize,
    findings_count: usize,
    high_severity: usize,
    medium_severity: usize,
}

/// Secret detection patterns: (pattern_name, substring/keyword to search for, severity)
const PATTERNS: &[(&str, &str, &str)] = &[
    ("password_assign", "password", "high"),
    ("passwd_assign", "passwd", "high"),
    ("secret_key", "secret_key", "high"),
    ("api_key", "api_key", "high"),
    ("apikey", "apikey", "high"),
    ("api_secret", "api_secret", "high"),
    ("auth_token", "auth_token", "high"),
    ("access_token", "access_token", "high"),
    ("private_key", "private_key", "high"),
    ("aws_secret", "aws_secret", "high"),
    ("aws_access_key", "aws_access_key", "high"),
    ("gh_token", "gh_token", "high"),
    ("github_token", "github_token", "high"),
    ("slack_token", "slack_token", "high"),
    ("stripe_key", "stripe_key", "high"),
    ("twilio_auth", "twilio_auth", "high"),
    ("pem_header", "-----BEGIN", "high"),
    ("database_url", "database_url", "medium"),
    ("db_password", "db_password", "medium"),
    ("connection_string", "connection_string", "medium"),
    ("jdbc_url", "jdbc:", "medium"),
    ("smtp_password", "smtp_password", "medium"),
    ("bearer_token", "bearer ", "medium"),
];

/// Compute Shannon entropy of a string.
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let len = s.len() as f64;
    let mut freq = [0usize; 256];
    for b in s.bytes() {
        freq[b as usize] += 1;
    }
    freq.iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Extract string literal contents from a line (single/double quoted).
fn extract_string_literals(line: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut lit = String::new();
            while i < chars.len() {
                let lc = chars[i];
                i += 1;
                if lc == quote {
                    break;
                }
                if lc == '\\' {
                    i += 1;
                    continue;
                }
                lit.push(lc);
            }
            if lit.len() >= 8 {
                literals.push(lit);
            }
        } else {
            i += 1;
        }
    }
    literals
}

/// True if line looks like a test, example, or comment context to skip.
fn is_likely_test_or_example(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("example")
        || lower.contains("placeholder")
        || lower.contains("your_")
        || lower.contains("<your")
        || lower.contains("changeme")
        || lower.contains("test_")
        || lower.trim_start().starts_with("//")
        || lower.trim_start().starts_with('#')
        || lower.trim_start().starts_with('*')
}

fn scan_file(path: &str, min_entropy: f64) -> Vec<SecretFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let mut findings = Vec::new();

    for (lineno, line) in source.lines().enumerate() {
        let line_lower = line.to_lowercase();

        if is_likely_test_or_example(line) {
            continue;
        }

        // Pattern-based detection: keyword present AND a non-empty assignment/value follows
        for &(pattern_name, keyword, severity) in PATTERNS {
            if line_lower.contains(keyword) {
                // Must look like an assignment or key-value, not just a variable name in code
                let has_value = line.contains('=') || line.contains(':');
                if !has_value {
                    continue;
                }

                let context = line.trim().chars().take(80).collect::<String>();
                findings.push(SecretFinding {
                    file: path.to_string(),
                    line: lineno + 1,
                    kind: "pattern".to_string(),
                    pattern: pattern_name.to_string(),
                    context,
                    entropy: None,
                    severity: severity.to_string(),
                    suggested_fix: Some(format!(
                        "Move `{}` value to an environment variable or secret manager.",
                        keyword
                    )),
                });
                break; // one finding per line from patterns
            }
        }

        // Entropy-based detection: high-entropy string literals
        for literal in extract_string_literals(line) {
            if literal.len() < 12 {
                continue;
            }
            let entropy = shannon_entropy(&literal);
            if entropy >= min_entropy {
                let context = line.trim().chars().take(80).collect::<String>();
                findings.push(SecretFinding {
                    file: path.to_string(),
                    line: lineno + 1,
                    kind: "entropy".to_string(),
                    pattern: format!("high_entropy_string (entropy={:.2})", entropy),
                    context,
                    entropy: Some(entropy),
                    severity: if entropy >= 5.5 { "high" } else { "medium" }.to_string(),
                    suggested_fix: Some(
                        "Replace hardcoded high-entropy string with an environment variable or config injection.".to_string()
                    ),
                });
            }
        }
    }

    findings
}

fn run(cli: Cli) {
    let mut extensions: Vec<&str> = vec![
        "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "hpp", "java",
        "rb", "swift", "php",
    ];
    if cli.include_config {
        extensions.extend_from_slice(&["env", "json", "yaml", "yml", "toml", "xml", "ini", "cfg"]);
    }

    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut all_findings: Vec<SecretFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file, cli.min_entropy));
    }

    all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let high = all_findings.iter().filter(|f| f.severity == "high").count();
    let medium = all_findings
        .iter()
        .filter(|f| f.severity == "medium")
        .count();

    let summary = SecretsSummary {
        files_scanned: files.len(),
        findings_count: all_findings.len(),
        high_severity: high,
        medium_severity: medium,
    };

    match cli.format.as_str() {
        "json" => {
            let report = SecretsReport {
                findings: all_findings,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for f in &all_findings {
                println!("{}", serde_json::to_string(f).unwrap());
            }
        }
        _ => {
            if all_findings.is_empty() {
                println!("No hardcoded secrets detected.");
            } else {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 35,
                        align_right: false,
                    },
                    Column {
                        header: "Line",
                        width: 6,
                        align_right: true,
                    },
                    Column {
                        header: "Sev",
                        width: 7,
                        align_right: false,
                    },
                    Column {
                        header: "Pattern",
                        width: 22,
                        align_right: false,
                    },
                    Column {
                        header: "Context",
                        width: 50,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for f in &all_findings {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&f.file, 35),
                            &f.line.to_string(),
                            &f.severity,
                            &truncate(&f.pattern, 22),
                            &truncate(&f.context, 50),
                        ],
                    );
                }
            }
            println!(
                "\nSummary: {} findings ({} high, {} medium) in {} files scanned",
                summary.findings_count,
                summary.high_severity,
                summary.medium_severity,
                summary.files_scanned
            );
        }
    }
}

fn main() {
    run(Cli::parse());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_high() {
        // a truly random-looking string should have high entropy
        let s = "xK9#mP2$nQ7@wR4!";
        assert!(shannon_entropy(s) > 3.0);
    }

    #[test]
    fn test_entropy_low() {
        let s = "aaaaaaaaaaaaaaaa";
        assert!(shannon_entropy(s) < 0.1);
    }

    #[test]
    fn test_extract_literals() {
        let line = r#"let key = "supersecretvalue123";"#;
        let lits = extract_string_literals(line);
        assert!(lits.iter().any(|l| l.contains("supersecret")));
    }

    #[test]
    fn test_pattern_detection() {
        let findings =
            scan_file_lines("test.rs", &["let api_key = \"AKIA1234567890ABCDEF\";"], 4.5);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].severity, "high");
    }

    #[test]
    fn test_skip_comment_lines() {
        let findings = scan_file_lines("test.rs", &["// api_key = \"some_value\""], 4.5);
        assert!(findings.is_empty(), "comment lines should be skipped");
    }

    // Helper for tests: scan in-memory lines
    fn scan_file_lines(path: &str, lines: &[&str], min_entropy: f64) -> Vec<SecretFinding> {
        let source = lines.join("\n");
        let mut findings = Vec::new();
        for (lineno, line) in source.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if is_likely_test_or_example(line) {
                continue;
            }
            for &(pattern_name, keyword, severity) in PATTERNS {
                if line_lower.contains(keyword) {
                    let has_value = line.contains('=') || line.contains(':');
                    if !has_value {
                        continue;
                    }
                    findings.push(SecretFinding {
                        file: path.to_string(),
                        line: lineno + 1,
                        kind: "pattern".to_string(),
                        pattern: pattern_name.to_string(),
                        context: line.trim().to_string(),
                        entropy: None,
                        severity: severity.to_string(),
                        suggested_fix: None,
                    });
                    break;
                }
            }
            for literal in extract_string_literals(line) {
                if literal.len() < 12 {
                    continue;
                }
                let entropy = shannon_entropy(&literal);
                if entropy >= min_entropy {
                    findings.push(SecretFinding {
                        file: path.to_string(),
                        line: lineno + 1,
                        kind: "entropy".to_string(),
                        pattern: format!("high_entropy_string"),
                        context: line.trim().to_string(),
                        entropy: Some(entropy),
                        severity: if entropy >= 5.5 { "high" } else { "medium" }.to_string(),
                        suggested_fix: None,
                    });
                }
            }
        }
        findings
    }
}
