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

    /// Exclude paths matching these patterns (comma-separated substrings)
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
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

/// True if the line is a description, pattern definition, or code example
/// rather than an actual hardcoded secret assignment.
fn is_false_positive_pattern(line: &str) -> bool {
    let lower = line.to_lowercase();
    let trimmed = line.trim();

    // Lines that describe what a pattern detects (tool infrastructure)
    if lower.contains("description:")
        || lower.contains("detects ")
        || lower.contains("flagging")
        || lower.contains("pattern:")
        || lower.contains("replaces ")
        || lower.contains("move `")
        || lower.contains("hardcoded ")
        || lower.contains("never use")
        || lower.contains("restrict ")
        || lower.contains("enforce ")
        || lower.contains("remediation:")
    {
        return true;
    }

    // Lines that are pattern/regex definitions (keyword inside a string literal used as a pattern)
    // e.g., pattern: "password = \""
    if (trimmed.contains("pattern:") && trimmed.contains('"'))
        || (trimmed.starts_with("(\"") && trimmed.contains("_assign"))
    {
        return true;
    }

    // Sudoers / config file pattern strings (not actual passwords)
    if lower.contains("sudoers") || lower.contains("nopasswd") {
        return true;
    }

    false
}

/// True if the value side of a keyword assignment looks like an actual secret
/// rather than a description, pattern name, or code example.
fn value_looks_like_secret(line: &str) -> bool {
    // Find the value after = or :
    let value = if let Some(pos) = line.find('=') {
        line[pos + 1..].trim()
    } else if let Some(pos) = line.find(':') {
        line[pos + 1..].trim()
    } else {
        return false;
    };

    // Empty value is not a secret
    if value.is_empty() {
        return false;
    }

    // Value is just a bare keyword or identifier (no actual secret content)
    let value_lower = value.to_lowercase();
    if value_lower == "password"
        || value_lower == "secret"
        || value_lower == "key"
        || value_lower == "token"
        || value_lower == "api_key"
        || value_lower == "api_secret"
        || value_lower == "auth_token"
        || value_lower == "access_token"
        || value_lower == "private_key"
    {
        return false;
    }

    // Value is a quoted string --- check if it is a pattern definition, not a real secret
    if value.starts_with('"') || value.starts_with('\'') {
        let inner = value.trim_matches(|c: char| c == '"' || c == ';' || c == ',');
        let inner_lower = inner.to_lowercase();
        // The quoted value IS the keyword itself --- pattern definition
        if inner_lower == "password"
            || inner_lower == "secret"
            || inner_lower == "key"
            || inner_lower == "token"
            || inner_lower == "api_key"
            || inner_lower == "api_secret"
            || inner_lower == "auth_token"
            || inner_lower == "access_token"
            || inner_lower == "private_key"
        {
            return false;
        }
    }

    // Value is a description sentence (starts with capital, contains natural language)
    if value.starts_with('"') && value.len() > 40 {
        let inner = value.trim_matches(|c: char| c == '"' || c == ';' || c == ',');
        if inner.starts_with("Hardcoded")
            || inner.starts_with("Replace")
            || inner.starts_with("Move ")
            || inner.starts_with("Never ")
            || inner.starts_with("Restrict")
            || inner.starts_with("Detects")
            || inner.starts_with("Default")
            || inner.starts_with("Literal")
            || inner.starts_with("Sudoers")
        {
            return false;
        }
    }

    true
}

/// True if a string literal looks like HTML/CSS template content rather than a secret.
fn is_html_or_css_string(s: &str) -> bool {
    let lower = s.to_lowercase();
    // Require at least two HTML/CSS indicators to avoid false filtering of URLs
    let indicators = [
        lower.contains("<span"),
        lower.contains("<div"),
        lower.contains("<p>"),
        lower.contains("<tr"),
        lower.contains("<td"),
        lower.contains("<table"),
        lower.contains("<link"),
        lower.contains("font-weight"),
        lower.contains("font-size"),
        lower.contains("font-family"),
        lower.contains("padding:"),
        lower.contains("line-height"),
        lower.contains("text-align"),
        lower.contains("style="),
        lower.contains("border-"),
    ];
    indicators.iter().filter(|&&b| b).count() >= 2
}

/// True if a string literal looks like a code snippet or diff rather than a secret.
/// Requires diff markers or multiple code indicators to avoid filtering real secrets.
fn is_code_snippet(s: &str) -> bool {
    let lower = s.to_lowercase();
    // Diff markers are strong indicators of embedded code examples
    if lower.contains("\\n+") || lower.contains("\\n-") {
        return true;
    }
    // Diff-style line prefixes
    if (lower.contains("+ let ") || lower.contains("- let "))
        || (lower.contains("+ if ") || lower.contains("- if "))
        || (lower.contains("+ Command") || lower.contains("- Command"))
    {
        return true;
    }
    // Code patterns that are very unlikely in real secrets
    if lower.contains("api_key = std") || lower.contains("env::var") {
        return true;
    }
    false
}

/// True if a string is a format string with placeholders or a help/description text.
fn is_format_or_description_string(s: &str) -> bool {
    let trimmed = s.trim_matches('"');
    // Format strings with {} or {:?}
    if trimmed.contains("{}") || trimmed.contains("{:?}") || trimmed.contains("{ }") {
        return true;
    }
    // Very long strings (100+ chars) with natural language words and punctuation are descriptions
    if trimmed.len() > 100 {
        let lower = trimmed.to_lowercase();
        let has_common_words = lower.contains("the ") || lower.contains(" and ")
            || lower.contains(" is ") || lower.contains(" for ") || lower.contains(" to ");
        let has_punct = trimmed.contains('.') || trimmed.contains(',') || trimmed.contains(';');
        if has_common_words && has_punct {
            return true;
        }
    }
    false
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

                // Skip false positives: descriptions, pattern definitions, code examples
                if is_false_positive_pattern(line) {
                    continue;
                }

                // Verify the value side looks like an actual secret, not a keyword repetition
                if !value_looks_like_secret(line) {
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
            // Skip strings that are clearly HTML/CSS templates
            if is_html_or_css_string(&literal) {
                continue;
            }
            // Skip strings that are code snippets or diffs
            if is_code_snippet(&literal) {
                continue;
            }
            // Skip format strings and description text
            if is_format_or_description_string(&literal) {
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

fn is_excluded(path: &str, exclude_patterns: &[String]) -> bool {
    if exclude_patterns.is_empty() {
        return false;
    }
    for pattern in exclude_patterns {
        // Skip empty patterns — empty string matches everything in Rust
        if pattern.is_empty() {
            continue;
        }
        if path.contains(pattern.as_str()) {
            return true;
        }
    }
    false
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

    // Apply path exclusions
    let files: Vec<String> = files
        .into_iter()
        .filter(|f| !is_excluded(f, &cli.exclude))
        .collect();

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
                    if is_false_positive_pattern(line) {
                        continue;
                    }
                    if !value_looks_like_secret(line) {
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
                if is_html_or_css_string(&literal)
                    || is_code_snippet(&literal)
                    || is_format_or_description_string(&literal)
                {
                    continue;
                }
                let entropy = shannon_entropy(&literal);
                if entropy >= min_entropy {
                    findings.push(SecretFinding {
                        file: path.to_string(),
                        line: lineno + 1,
                        kind: "entropy".to_string(),
                        pattern: "high_entropy_string".to_string(),
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

    #[test]
    fn test_false_positive_pattern_detection() {
        assert!(is_false_positive_pattern(
            r#"pattern: "password = \"\""#
        ));
        assert!(is_false_positive_pattern(
            r#"description: "Hardcoded password detected.""#
        ));
        assert!(is_false_positive_pattern(
            "remediation: \"Never use literal strings as passwords.\""
        ));
        assert!(is_false_positive_pattern(
            "sudoers: ALL=(ALL) NOPASSWD: ALL"
        ));
    }

    #[test]
    fn test_value_looks_like_secret() {
        // Real secret: actual password value assigned
        assert!(value_looks_like_secret(r#"password = "s3cr3tP@ss";"#));
        assert!(value_looks_like_secret(r#"api_key = "sk-1234567890abcdef";"#));
        // Real password that starts with "password" keyword should still be flagged
        assert!(value_looks_like_secret(r#"password = "password123!";"#));
        // False positive: keyword repetition as value
        assert!(!value_looks_like_secret(r#"password = "password";"#));
        assert!(!value_looks_like_secret(r#"api_key = "api_key";"#));
    }

    #[test]
    fn test_html_css_filtered() {
        assert!(is_html_or_css_string(
            r#"<span style="color:#22c55e;font-weight:700">FIXED</span>"#
        ));
        assert!(is_html_or_css_string(
            r#"<p style="padding:10px;font-size:13px">text</p>"#
        ));
        assert!(!is_html_or_css_string("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_code_snippet_filtered() {
        assert!(is_code_snippet(
            r#"- let q = format!("SELECT * FROM users WHERE id = {}", id);"#
        ));
        assert!(is_code_snippet(
            r#"+ let api_key = std::env::var("API_KEY")?;"#
        ));
        assert!(!is_code_snippet("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_format_string_filtered() {
        assert!(is_format_or_description_string(
            r#""Health Score: {}/100""#
        ));
        assert!(is_format_or_description_string(
            "\"Measures how risky a function is to change. Combines cyclomatic complexity and test coverage into a single metric.\""
        ));
        assert!(!is_format_or_description_string("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_description_not_flagged() {
        let findings = scan_file_lines(
            "test.rs",
            &["description: \"Hardcoded password detected.\""],
            4.5,
        );
        assert!(findings.is_empty(), "description lines should not be flagged");
    }

    #[test]
    fn test_pattern_definition_not_flagged() {
        let findings = scan_file_lines(
            "test.rs",
            &[r#"pattern: "password = \"\""#],
            4.5,
        );
        assert!(findings.is_empty(), "pattern definitions should not be flagged");
    }

    #[test]
    fn test_html_template_not_flagged() {
        let findings = scan_file_lines(
            "test.rs",
            &[r#""<span style=\"color:#22c55e;font-weight:700\">FIXED</span>""#],
            4.5,
        );
        assert!(findings.is_empty(), "HTML templates should not be flagged");
    }

    #[test]
    fn test_real_secret_still_flagged() {
        let findings = scan_file_lines(
            "test.rs",
            &[r#"let password = "s3cr3tP@ss1234!";"#],
            4.5,
        );
        assert!(!findings.is_empty(), "real secrets should still be flagged");
    }
}

    #[test]
    fn test_is_excluded() {
        let patterns = vec!["secrets".to_string(), "tests".to_string()];
        assert!(is_excluded("./crates/secrets/src/main.rs", &patterns));
        assert!(is_excluded("./crates/foo/tests/integration.rs", &patterns));
        assert!(!is_excluded("./crates/cogent-cli/src/main.rs", &patterns));
        assert!(!is_excluded("./src/lib.rs", &patterns));
        assert!(!is_excluded("./crates/secrets/src/main.rs", &[]));
    }

    #[test]
    fn test_is_excluded_edge_cases() {
        // Empty pattern list matches nothing
        assert!(!is_excluded("any/path.rs", &[]));
        // Empty string pattern is skipped (would match everything otherwise)
        assert!(!is_excluded("any/path.rs", &[String::new()]));
        // Substring match
        assert!(is_excluded("src/vendor/lib.rs", &["vendor".to_string()]));
        // Exact substring at start
        assert!(is_excluded("vendor/lib.rs", &["vendor".to_string()]));
        // Partial match in middle of path component
        assert!(is_excluded("src/some_vendor_dir/file.rs", &["vendor".to_string()]));
    }

    /// End-to-end: create temp files with hardcoded secrets, run the scanner
    /// with `--exclude`, and verify the excluded file is suppressed.
    #[test]
    fn test_exclude_suppresses_real_file_scanning() {
        use std::io::Write;

        let dir = std::env::temp_dir().join("cogent_secrets_e2e_test");
        let _ = std::fs::create_dir_all(&dir);

        // File A: should be scanned (contains a real secret)
        let file_a = dir.join("app_config.rs");
        {
            let mut f = std::fs::File::create(&file_a).unwrap();
            writeln!(f, "pub const API_KEY: &str = \"sk-proj-abc1234567890xyz\";").unwrap();
        }

        // File B: should be excluded (also contains a real secret)
        let file_b = dir.join("vendor_secret.rs");
        {
            let mut f = std::fs::File::create(&file_b).unwrap();
            writeln!(f, "pub const DB_PASSWORD: &str = \"p@ssw0rd_9876543210\";").unwrap();
        }

        // Scan WITHOUT exclude — both files should produce findings
        let findings_no_exclude = scan_file(file_a.to_str().unwrap(), 4.5);
        assert!(!findings_no_exclude.is_empty(), "file_a should have findings");
        let findings_b_no_exclude = scan_file(file_b.to_str().unwrap(), 4.5);
        assert!(!findings_b_no_exclude.is_empty(), "file_b should have findings");

        // Verify is_excluded would suppress file_b but not file_a
        let exclude_patterns = vec!["vendor".to_string()];
        assert!(!is_excluded(file_a.to_str().unwrap(), &exclude_patterns),
            "app_config.rs should NOT be excluded");
        assert!(is_excluded(file_b.to_str().unwrap(), &exclude_patterns),
            "vendor_secret.rs SHOULD be excluded");

        // Simulate the filtering pipeline: collect files, apply is_excluded, scan remaining
        let all_files = vec![
            file_a.to_str().unwrap().to_string(),
            file_b.to_str().unwrap().to_string(),
        ];
        let filtered: Vec<&String> = all_files
            .iter()
            .filter(|f| !is_excluded(f, &exclude_patterns))
            .collect();
        assert_eq!(filtered.len(), 1, "only file_a should remain after exclusion");
        assert!(filtered[0].ends_with("app_config.rs"));

        // Clean up
        let _ = std::fs::remove_file(&file_a);
        let _ = std::fs::remove_file(&file_b);
        let _ = std::fs::remove_dir(&dir);
    }

    /// End-to-end: verify that exclude with multiple patterns
    /// correctly filters from a realistic file set.
    #[test]
    fn test_is_excluded_path_traversal_patterns() {
        // Path traversal with ../
        assert!(is_excluded("../../vendor/secret.rs", &["vendor".to_string()]));
        // Absolute paths
        assert!(is_excluded("/home/user/vendor/lib.rs", &["vendor".to_string()]));
        // Windows-style paths
        assert!(is_excluded("C:\\Users\\vendor\\file.rs", &["vendor".to_string()]));
        // Empty path should not match non-empty patterns
        assert!(!is_excluded("", &["vendor".to_string()]));
        // Path that is exactly the pattern (no slashes)
        assert!(is_excluded("vendor", &["vendor".to_string()]));
        // Pattern is a substring at start of a path component
        assert!(is_excluded("vendorlib/main.rs", &["vendor".to_string()]));
    }

    #[test]
    fn test_exclude_multiple_empty_patterns() {
        // Multiple empty patterns should all be skipped
        assert!(!is_excluded("any/path.rs", &[String::new(), String::new()]));
        // Mix of empty and valid
        assert!(is_excluded("src/vendor/lib.rs", &[String::new(), "vendor".to_string()]));
    }

    #[test]
    fn test_exclude_unicode_patterns() {
        assert!(is_excluded("src/日本語テスト/main.rs", &["日本語".to_string()]));
        assert!(!is_excluded("src/main.rs", &["日本語".to_string()]));
    }

    #[test]
    fn test_exclude_multi_pattern_filtering() {
        let paths = vec![
            "src/main.rs".to_string(),
            "vendor/lib.rs".to_string(),
            "tests/integration.rs".to_string(),
            "docs/README.md".to_string(),
            "src/vendor_helper.rs".to_string(),
        ];
        let excludes = vec!["vendor".to_string(), "tests".to_string()];

        let filtered: Vec<&String> = paths
            .iter()
            .filter(|p| !is_excluded(p, &excludes))
            .collect();

        // vendor/ and vendor_helper.rs both contain "vendor" substring
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|p| p.ends_with("main.rs")));
        assert!(filtered.iter().any(|p| p.ends_with("README.md")));
        assert!(!filtered.iter().any(|p| p.contains("vendor")));
        assert!(!filtered.iter().any(|p| p.contains("tests")));
    }
