#![deny(clippy::all)]

use ast_parse_ts::Language;
use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "taint",
    about = "Taint analysis — detect sensitive data flow to sinks like logging or public outputs"
)]
struct Cli {
    /// Path to scan (file or directory)
    #[arg(default_value = ".")]
    path: String,

    /// Recursive scan
    #[arg(short, long)]
    recursive: bool,

    /// Output format: table (default) or json
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Attribute that marks sensitive variables (default: sensitive)
    #[arg(long, default_value = "sensitive")]
    attribute: String,

    /// Minimum severity to report: low, medium, high (default: low)
    #[arg(long, default_value = "low")]
    severity: String,

    /// Run built-in fixture tests and report precision/recall (does not scan files)
    #[arg(long)]
    self_test: bool,
}

#[derive(Debug, Clone, Serialize)]
struct Violation {
    file: String,
    line: usize,
    variable: String,
    violation_type: String,
    severity: String,
    context: String,
    /// Confidence level in the taint violation detection (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f64>,
    /// Suggested fix for the taint violation
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct TaintReport {
    violations: Vec<Violation>,
    summary: TaintSummary,
}

#[derive(Debug, Clone, Serialize)]
struct TaintSummary {
    total_files_scanned: usize,
    sensitive_variables_found: usize,
    violations_count: usize,
    high_severity: usize,
    medium_severity: usize,
    low_severity: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

const TAINT_EXTS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "cxx", "hpp", "cs",
    "java", "php", "rb", "swift",
];

fn resolve_source_files(path: &str, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let target_path = Path::new(path);
    let files: Vec<PathBuf> = if target_path.is_file() {
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if TAINT_EXTS.contains(&ext) {
            vec![target_path.to_path_buf()]
        } else {
            return Err(format!("Unsupported file type: {}", path));
        }
    } else if target_path.is_dir() {
        find_source_files(path, recursive, TAINT_EXTS)
            .into_iter()
            .map(PathBuf::from)
            .collect()
    } else {
        return Err(format!("No source files found at {}", path));
    };
    if files.is_empty() {
        return Err("No supported source files found to analyze.".to_string());
    }
    Ok(files)
}

fn scan_source_files(
    files: &[PathBuf],
    attr: &str,
    severity_filter: &str,
) -> (Vec<Violation>, usize) {
    // Single-threaded rayon to reduce memory pressure (prevents OOM on 16GB/32GB systems)
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let results: Vec<(Vec<Violation>, usize)> = pool.install(|| {
        files
            .par_iter()
            .filter_map(|file_path| {
                let source = std::fs::read_to_string(file_path).ok()?;
                let file_str = file_path.to_string_lossy().to_string();
                let lang = Language::from_extension(&file_str);
                Some(analyze_file_multilang(&source, &file_str, attr, lang))
            })
            .collect()
    });

    let mut all_violations = Vec::new();
    let mut total_sensitive = 0usize;
    for (violations, sensitive_count) in results {
        total_sensitive += sensitive_count;
        for v in violations {
            let include = match v.severity.as_str() {
                "high" => true,
                "medium" => severity_filter != "high",
                "low" => severity_filter == "low",
                _ => true,
            };
            if include {
                all_violations.push(v);
            }
        }
    }
    (all_violations, total_sensitive)
}

fn build_taint_report(
    violations: Vec<Violation>,
    sensitive_count: usize,
    files_scanned: usize,
) -> TaintReport {
    let high = violations.iter().filter(|v| v.severity == "high").count();
    let medium = violations.iter().filter(|v| v.severity == "medium").count();
    let low = violations.iter().filter(|v| v.severity == "low").count();

    TaintReport {
        violations: violations.clone(),
        summary: TaintSummary {
            total_files_scanned: files_scanned,
            sensitive_variables_found: sensitive_count,
            violations_count: violations.len(),
            high_severity: high,
            medium_severity: medium,
            low_severity: low,
        },
    }
}

fn emit_report(report: &TaintReport, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        "json" => output_json(report),
        _ => {
            output_table(report);
            Ok(())
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let source_files = resolve_source_files(&cli.path, cli.recursive)?;
    let severity_filter = cli.severity.to_lowercase();
    let (violations, sensitive_count) =
        scan_source_files(&source_files, &cli.attribute, &severity_filter);
    let report = build_taint_report(violations, sensitive_count, source_files.len());
    let _ = emit_report(&report, &cli.format);
    Ok(())
}

fn output_table(report: &TaintReport) {
    println!("TAINT ANALYSIS");
    println!("{}", "─".repeat(70));
    println!();
    println!(
        "  Files scanned:            {}",
        report.summary.total_files_scanned
    );
    println!(
        "  Sensitive variables:      {}",
        report.summary.sensitive_variables_found
    );
    println!(
        "  Violations:               {}",
        report.summary.violations_count
    );
    println!();
    if !report.violations.is_empty() {
        println!("  VIOLATIONS:");
        let columns = [
            Column::left("SEV", 8),
            Column::left("TYPE", 12),
            Column::left("VAR", 15),
            Column::left("FILE", 15),
            Column::right("LINE", 5),
            Column::left("HINT", 40),
        ];
        print_table_header(&columns);
        for v in &report.violations {
            let sev_icon = match v.severity.as_str() {
                "high" => "!!",
                "medium" => "!",
                _ => "•",
            };
            let hint = v.suggested_fix.as_deref().unwrap_or("");
            let hint_truncated = if hint.len() > 37 { &hint[0..37] } else { hint };
            print_table_row(
                &columns,
                &[
                    &format!("{} {}", sev_icon, v.severity),
                    &v.violation_type,
                    &v.variable,
                    &truncate(&v.file, 14),
                    &v.line.to_string(),
                    hint_truncated,
                ],
            );
        }
    } else {
        println!("  No taint violations detected. Clean data flow!");
    }
    println!();
}

fn output_json(report: &TaintReport) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}

fn analyze_file_multilang(
    source: &str,
    file: &str,
    _attr: &str,
    lang: Language,
) -> (Vec<Violation>, usize) {
    let mut violations = Vec::new();
    let mut sensitive_vars: HashSet<String> = HashSet::new();

    // Detect sensitive variable markers per language:
    //  Python: `# @sensitive` or `# sensitive:` above an assignment
    //  JS/TS:  `// @sensitive` or `/* @sensitive */` above a const/let/var
    //  Go:     `// @sensitive` above a var declaration
    let sensitive_comment = match lang {
        Language::Python => "# @sensitive",
        Language::JavaScript | Language::TypeScript => "// @sensitive",
        Language::Go => "// @sensitive",
        _ => "// @sensitive",
    };

    // Also detect by common sensitive keywords in variable names (LHS only)
    let sensitive_keywords = [
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "credential",
        "auth_key",
        "access_key",
    ];

    let lines: Vec<&str> = source.lines().collect();
    let mut in_sensitive_block = false;
    let mut in_test_function = false;
    let mut prev_was_sensitive_attr = false;

    for (idx, &line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Skip test functions in Rust
        if lang == Language::Rust {
            if trimmed.starts_with("#[test]") {
                in_test_function = true;
                continue;
            }
            if in_test_function && trimmed.starts_with("}") {
                in_test_function = false;
                continue;
            }
            if in_test_function {
                continue;
            }
        }

        // Rust: detect #[sensitive] attribute on its own line
        if lang == Language::Rust
            && (trimmed == "#[sensitive]" || trimmed.starts_with("#[sensitive"))
        {
            prev_was_sensitive_attr = true;
            continue;
        }
        if prev_was_sensitive_attr {
            in_sensitive_block = true;
            prev_was_sensitive_attr = false;
        }

        // Detect marker comment
        if trimmed.contains(sensitive_comment) || trimmed.contains("@sensitive") {
            in_sensitive_block = true;
            continue;
        }

        // Detect assignment line (Python: `x = ...`, JS: `const/let/var x = ...`, Go: `x :=` or `var x`)
        let lhs = extract_lhs_identifier(trimmed, lang);

        if let Some(ref var_name) = lhs {
            let lhs_lower = var_name.to_lowercase();
            let is_keyword_sensitive = sensitive_keywords.iter().any(|k| lhs_lower.contains(k));
            // Detect Secret wrapper types in the RHS: `Secret::new(...)` / `SecretString::new(...)`
            let is_secret_rhs = trimmed.contains("Secret::");

            if in_sensitive_block || is_keyword_sensitive || is_secret_rhs {
                sensitive_vars.insert(var_name.clone());
                in_sensitive_block = false;
                continue;
            }
        }

        if in_sensitive_block && !trimmed.is_empty() {
            in_sensitive_block = false;
        }

        // Check for violations: does this line reference any sensitive var in a sink?
        for var in &sensitive_vars {
            if line.contains(var.as_str()) {
                if is_logging_sink_multilang(trimmed, lang) {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: line_num,
                        variable: var.clone(),
                        violation_type: "LOG_LEAK".to_string(),
                        severity: "high".to_string(),
                        context: truncate(trimmed, 60).to_string(),
                        confidence: None,
                        suggested_fix: Some(get_taint_hint("LOG_LEAK", var, trimmed)),
                        auto_fix_available: Some(false),
                    });
                } else if is_print_sink_multilang(trimmed, lang) {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: line_num,
                        variable: var.clone(),
                        violation_type: "PRINT_LEAK".to_string(),
                        severity: "high".to_string(),
                        context: truncate(trimmed, 60).to_string(),
                        confidence: None,
                        suggested_fix: Some(get_taint_hint("PRINT_LEAK", var, trimmed)),
                        auto_fix_available: Some(false),
                    });
                } else if is_file_write_sink_multilang(trimmed, lang) {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: line_num,
                        variable: var.clone(),
                        violation_type: "FILE_WRITE".to_string(),
                        severity: "medium".to_string(),
                        context: truncate(trimmed, 60).to_string(),
                        confidence: None,
                        suggested_fix: Some(get_taint_hint("FILE_WRITE", var, trimmed)),
                        auto_fix_available: Some(false),
                    });
                } else if is_public_return_multilang(trimmed, var, lang) {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: line_num,
                        variable: var.clone(),
                        violation_type: "UNFILTERED_RETURN".to_string(),
                        severity: "medium".to_string(),
                        context: truncate(trimmed, 60).to_string(),
                        confidence: None,
                        suggested_fix: Some(get_taint_hint("UNFILTERED_RETURN", var, trimmed)),
                        auto_fix_available: Some(false),
                    });
                } else if is_debug_sink_multilang(trimmed, lang) {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: line_num,
                        variable: var.clone(),
                        violation_type: "DEBUG_LEAK".to_string(),
                        severity: "low".to_string(),
                        context: truncate(trimmed, 60).to_string(),
                        confidence: None,
                        suggested_fix: Some(get_taint_hint("DEBUG_LEAK", var, trimmed)),
                        auto_fix_available: Some(false),
                    });
                }
            }
        }
    }

    (violations, sensitive_vars.len())
}

/// Extract the identifier on the LHS of an assignment for a given language.
fn extract_lhs_identifier(line: &str, lang: Language) -> Option<String> {
    match lang {
        Language::Python => extract_lhs_python(line),
        Language::JavaScript | Language::TypeScript => extract_lhs_js(line),
        Language::Go => extract_lhs_go(line),
        Language::Rust => extract_lhs_rust(line),
        _ => None,
    }
}

fn extract_lhs_rust(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if !trimmed.starts_with("let ")
        && !trimmed.starts_with("const ")
        && !trimmed.starts_with("static ")
    {
        return None;
    }

    // Get the part after the keyword
    let after_kw = if let Some(stripped) = trimmed.strip_prefix("let ") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("const ") {
        stripped
    } else {
        trimmed.strip_prefix("static ").unwrap_or(trimmed)
    };

    let rest = after_kw.trim_start();
    let rest = if let Some(stripped) = rest.strip_prefix("mut ") {
        stripped
    } else {
        rest
    };

    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_lhs_python(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let eq_pos = trimmed.find('=')?;
    if trimmed[..eq_pos].contains('(') || trimmed.starts_with('#') {
        return None;
    }
    let lhs = trimmed[..eq_pos].trim().to_string();
    if !lhs.is_empty() && lhs.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(lhs)
    } else {
        None
    }
}

fn extract_lhs_js(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for kw in &["const ", "let ", "var "] {
        if let Some(rest) = trimmed.strip_prefix(kw) {
            let ident = rest.split(['=', ':', ';', ' ']).next().unwrap_or("").trim();
            if !ident.is_empty() {
                return Some(ident.to_string());
            }
        }
    }
    None
}

fn extract_lhs_go(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("var ") {
        let ident = rest.split_whitespace().next().unwrap_or("").to_string();
        if !ident.is_empty() {
            return Some(ident);
        }
    }
    let pos = trimmed.find(":=")?;
    let lhs = trimmed[..pos].trim().to_string();
    if !lhs.is_empty() && !lhs.contains(',') {
        Some(lhs)
    } else {
        None
    }
}

/// Generate guided hints for taint violations
fn get_taint_hint(violation_type: &str, variable: &str, _context: &str) -> String {
    match violation_type {
        "LOG_LEAK" => format!(
            "Sensitive data '{}' leaked via logging. Fix: 1) Remove sensitive fields before logging, 2) Use redaction (e.g., '***'), 3) Implement structured logging with field-level privacy.",
            variable
        ),
        "PRINT_LEAK" => format!(
            "Sensitive data '{}' printed to stdout/stderr. Fix: 1) Remove print statements in production, 2) Use proper logging framework, 3) Redact sensitive values.",
            variable
        ),
        "FILE_WRITE" => format!(
            "Sensitive data '{}' written to file. Fix: 1) Ensure file permissions (600), 2) Encrypt sensitive data at rest, 3) Use secure file APIs.",
            variable
        ),
        "UNFILTERED_RETURN" => format!(
            "Sensitive data '{}' returned unfiltered. Fix: 1) Sanitize return value, 2) Use DTOs without sensitive fields, 3) Add output validation.",
            variable
        ),
        "DEBUG_LEAK" => format!(
            "Sensitive data '{}' in debug output. Fix: 1) Guard with debug flags, 2) Remove from production builds, 3) Use conditional compilation.",
            variable
        ),
        _ => format!(
            "Potential taint violation with '{}'. Review data flow and apply appropriate sanitization or access controls.",
            variable
        ),
    }
}

/// Logging sink patterns for all languages.
fn is_logging_sink_multilang(line: &str, lang: Language) -> bool {
    let patterns: &[&str] = match lang {
        Language::Python => &["logging.", "logger.", "log."],
        Language::JavaScript | Language::TypeScript => &[
            "console.log",
            "console.warn",
            "console.error",
            "logger.",
            "log.info",
            "winston.",
            "bunyan.",
        ],
        Language::Go => &[
            "log.Print",
            "log.Fatal",
            "log.Panic",
            "fmt.Fprintf",
            "logrus.",
            "zap.",
        ],
        Language::Rust => &[
            "log::info!",
            "log::warn!",
            "log::error!",
            "log::debug!",
            "log::trace!",
        ],
        _ => return false,
    };
    patterns.iter().any(|p| line.contains(p))
}

/// Print/output sink patterns for all languages.
fn is_print_sink_multilang(line: &str, lang: Language) -> bool {
    let patterns: &[&str] = match lang {
        Language::Python => &["print("],
        Language::JavaScript | Language::TypeScript => &["process.stdout", "process.stderr"],
        Language::Go => &["fmt.Print", "fmt.Sprintf"],
        Language::Rust => &["println!", "print!", "eprintln!", "eprint!"],
        _ => return false,
    };
    patterns.iter().any(|p| line.contains(p))
}

/// File-write sink patterns for all languages.
fn is_file_write_sink_multilang(line: &str, lang: Language) -> bool {
    let patterns: &[&str] = match lang {
        Language::Rust => &[
            "std::fs::write",
            "File::create",
            "fs::File::create",
            "std::fs::OpenOptions",
        ],
        _ => return false,
    };
    patterns.iter().any(|p| line.contains(p))
}

/// Public return sink patterns for all languages.
fn is_public_return_multilang(line: &str, _var: &str, lang: Language) -> bool {
    match lang {
        Language::Rust => line.trim_start().starts_with("pub fn") && line.contains(_var),
        _ => false,
    }
}

/// Debug/trace sink patterns for all languages.
fn is_debug_sink_multilang(line: &str, lang: Language) -> bool {
    let patterns: &[&str] = match lang {
        Language::Rust => &["dbg!", "debug!", "trace!"],
        Language::Python => &["pprint", "repr("],
        Language::JavaScript | Language::TypeScript => &["console.debug", "console.trace"],
        Language::Go => &["fmt.Printf", "log.Printf"],
        _ => return false,
    };
    patterns.iter().any(|p| line.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_log_leak() {
        let source = r#"
#[sensitive]
let my_credential = load_secret();

fn do_something() {
    log::info!("credential: {}", my_credential);
}
"#;
        let (violations, sensitive_count) =
            analyze_file_multilang(source, "test.rs", "sensitive", ast_parse_ts::Language::Rust);
        assert_eq!(sensitive_count, 1, "Should detect 1 sensitive variable");
        assert!(!violations.is_empty(), "Should detect violation");
        assert!(violations.iter().any(|v| v.violation_type == "LOG_LEAK"));
        assert!(violations.iter().any(|v| v.severity == "high"));
    }

    #[test]
    fn test_no_violation_safe_usage() {
        let source = r#"
#[sensitive]
let password = load_password();

fn hash_password() {
    let hashed = bcrypt_hash(&password);
    store_hash(hashed);
}
"#;
        let (violations, sensitive_count) =
            analyze_file_multilang(source, "test.rs", "sensitive", ast_parse_ts::Language::Rust);
        assert_eq!(sensitive_count, 1);
        assert!(
            violations.is_empty(),
            "Safe usage should not trigger violation"
        );
    }

    #[test]
    fn test_detect_secret_type() {
        let source = r#"
let my_value = Secret::new("my_value");
println!("value: {}", my_value);
"#;
        let (violations, sensitive_count) =
            analyze_file_multilang(source, "test.rs", "sensitive", ast_parse_ts::Language::Rust);
        assert_eq!(sensitive_count, 1, "Should detect secret type variable");
        assert!(!violations.is_empty(), "Should detect print leak");
    }

    #[test]
    fn test_python_log_leak() {
        let source = r#"
# @sensitive
api_key = os.getenv("API_KEY")
logging.info("Using key: %s", api_key)
"#;
        let (violations, sensitive_count) = analyze_file_multilang(
            source,
            "test.py",
            "sensitive",
            ast_parse_ts::Language::Python,
        );
        assert_eq!(sensitive_count, 1, "Should detect 1 sensitive variable");
        assert!(!violations.is_empty(), "Should detect Python logging leak");
        assert!(violations.iter().any(|v| v.violation_type == "LOG_LEAK"));
    }

    #[test]
    fn test_js_console_leak() {
        let source = r#"
// @sensitive
const token = getAuthToken();
console.log("Token:", token);
"#;
        let (violations, sensitive_count) = analyze_file_multilang(
            source,
            "test.js",
            "sensitive",
            ast_parse_ts::Language::JavaScript,
        );
        assert_eq!(sensitive_count, 1, "Should detect 1 sensitive variable");
        assert!(!violations.is_empty(), "Should detect JS console.log leak");
        assert!(violations.iter().any(|v| v.violation_type == "LOG_LEAK"));
    }

    #[test]
    fn test_go_log_leak() {
        let source = r#"
// @sensitive
var apiKey = os.Getenv("API_KEY")
log.Printf("Using key: %s", apiKey)
"#;
        let (violations, sensitive_count) =
            analyze_file_multilang(source, "test.go", "sensitive", ast_parse_ts::Language::Go);
        assert_eq!(sensitive_count, 1, "Should detect 1 sensitive variable");
        assert!(!violations.is_empty(), "Should detect Go log leak");
        assert!(violations.iter().any(|v| v.violation_type == "LOG_LEAK"));
    }

    #[test]
    fn test_python_safe_usage_no_violation() {
        let source = r#"
# @sensitive
password = load_password()
hashed = bcrypt_hash(password)
store_hash(hashed)
"#;
        let (violations, sensitive_count) = analyze_file_multilang(
            source,
            "test.py",
            "sensitive",
            ast_parse_ts::Language::Python,
        );
        assert_eq!(sensitive_count, 1);
        assert!(
            violations.is_empty(),
            "Safe Python usage should not trigger violation"
        );
    }
}
