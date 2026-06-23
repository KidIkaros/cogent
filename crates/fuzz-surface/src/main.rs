#![deny(clippy::all)]

use ast_parse_ts::{parse_complexity, Language};
use clap::Parser;
use cogent_common::{
    find_source_files, print_table_header, print_table_row, separator, truncate, Column,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "fuzz",
    about = "Fuzzing surface analyzer — identify functions ideal for fuzz testing"
)]
struct Cli {
    /// Path to scan (file or directory)
    path: String,

    /// Recursive scan
    #[arg(short, long)]
    recursive: bool,

    /// Output format: table (default) or json
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Only show functions with score >= this value
    #[arg(long, default_value = "0")]
    min_score: u32,

    /// Limit output to top N functions
    #[arg(long, default_value = "20")]
    top: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FuzzableFunction {
    name: String,
    file: String,
    line: usize,
    params: Vec<String>,
    score: u32,
    is_public: bool,
    complexity: u32,
    has_harness: bool,
}

#[derive(Serialize)]
struct FuzzReport {
    functions: Vec<FuzzableFunction>,
    summary: FuzzSummary,
}

#[derive(Serialize)]
struct FuzzSummary {
    total_functions: usize,
    fuzzable_functions: usize,
    functions_with_harnesses: usize,
    avg_score: f64,
}

// ═══════════════════════════════════════════
// LANGUAGE FUNCTION DETECTION TABLE
// ═══════════════════════════════════════════

/// Configuration for detecting function declarations in a given language.
struct FnDetector {
    /// Keywords that start a function declaration (e.g. "fn ", "def ", "function ")
    keywords: &'static [&'static str],
    /// Optional: full signature parser with parameter analysis (score > 10).
    /// When set, this takes precedence over basic extraction.
    #[allow(clippy::type_complexity)]
    parse_sig: Option<fn(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction>>,
    /// Optional: extra line-level check beyond keyword match (e.g. arrow functions).
    extra_check: Option<fn(trimmed: &str) -> bool>,
    /// How to determine if a matched function is public.
    is_public: fn(trimmed: &str) -> bool,
}

/// Extract a function name directly following a keyword prefix.
/// Works for: `fn foo(`, `def foo(`, `func foo(`, `function foo(`, etc.
fn extract_name_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let after = line.strip_prefix(keyword)?;
    let name = after.split('(').next()?.trim();
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_string())
    } else {
        None
    }
}

/// Extract name from C-style declarations: `int foo(` → `foo`
fn extract_c_name(line: &str) -> Option<String> {
    let name = line
        .split('(')
        .next()?
        .split_whitespace()
        .last()
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .map(|n| n.to_string());
    name
}

/// All function detectors keyed by Language.
fn fn_detector(lang: Language) -> Option<FnDetector> {
    match lang {
        // JS/TS: function, const, let, and arrow functions
        Language::JavaScript | Language::TypeScript => Some(FnDetector {
            keywords: &["function ", "const ", "let "],
            parse_sig: Some(parse_js_fn_sig),
            extra_check: Some(|trimmed| trimmed.contains("=>")),
            is_public: |_| true,
        }),

        // Go: func Foo(
        Language::Go => Some(FnDetector {
            keywords: &["func "],
            parse_sig: Some(parse_go_fn_sig),
            extra_check: None,
            is_public: |trimmed| {
                trimmed
                    .strip_prefix("func ")
                    .and_then(|s| s.split('(').next())
                    .and_then(|n| n.trim().chars().next())
                    .is_some_and(|c| c.is_uppercase())
            },
        }),

        // C/C++: return_type func_name(
        Language::C | Language::Cpp => Some(FnDetector {
            keywords: &[
                "int ",
                "void ",
                "char ",
                "float ",
                "double ",
                "bool ",
                "size_t ",
                "unsigned ",
                "signed ",
                "long ",
                "short ",
            ],
            parse_sig: None,
            extra_check: None,
            is_public: |_| true,
        }),

        // C#: access_modifier return_type MethodName(
        Language::CSharp => Some(FnDetector {
            keywords: &[
                "public ",
                "private ",
                "protected ",
                "internal ",
                "static ",
                "void ",
                "int ",
                "string ",
                "bool ",
                "var ",
            ],
            parse_sig: None,
            extra_check: None,
            is_public: |trimmed| trimmed.starts_with("public ") || !trimmed.starts_with("private "),
        }),

        // Java: access_modifier return_type methodName(
        Language::Java => Some(FnDetector {
            keywords: &[
                "public ",
                "private ",
                "protected ",
                "static ",
                "void ",
                "int ",
                "String ",
                "boolean ",
                "long ",
                "double ",
            ],
            parse_sig: None,
            extra_check: None,
            is_public: |trimmed| trimmed.starts_with("public ") || !trimmed.starts_with("private "),
        }),

        // PHP: function name(
        Language::Php => Some(FnDetector {
            keywords: &["function "],
            parse_sig: None,
            extra_check: None,
            is_public: |_| true,
        }),

        // Ruby: def name
        Language::Ruby => Some(FnDetector {
            keywords: &["def "],
            parse_sig: None,
            extra_check: None,
            is_public: |_| true,
        }),

        // Swift: func name(
        Language::Swift => Some(FnDetector {
            keywords: &["func "],
            parse_sig: None,
            extra_check: None,
            is_public: |trimmed| trimmed.starts_with("public ") || trimmed.starts_with("open "),
        }),

        // Kotlin: fun name(
        Language::Kotlin => Some(FnDetector {
            keywords: &["fun "],
            parse_sig: None,
            extra_check: None,
            is_public: |trimmed| {
                trimmed.starts_with("public ")
                    || trimmed.starts_with("internal ")
                    || !trimmed.starts_with("private ")
            },
        }),

        // Solidity: function name(
        Language::Solidity => Some(FnDetector {
            keywords: &["function "],
            parse_sig: None,
            extra_check: None,
            is_public: |trimmed| {
                trimmed.starts_with("public ")
                    || trimmed.starts_with("external ")
                    || !trimmed.starts_with("private ")
            },
        }),

        _ => None,
    }
}

/// Generic single-line function detector. Scans lines for keyword matches
/// and either runs a full signature parser or does basic name extraction.
fn detect_functions(source: &str, file: &str, lang: Language) -> Vec<FuzzableFunction> {
    let detector = match fn_detector(lang) {
        Some(d) => d,
        None => return vec![],
    };
    let mut functions = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
            continue;
        }
        // Skip attributes/annotations
        if trimmed.starts_with('[') || trimmed.starts_with('@') || trimmed.starts_with('*') {
            continue;
        }

        let has_keyword = detector.keywords.iter().any(|kw| trimmed.starts_with(kw));
        let has_arrow = detector.extra_check.is_some_and(|check| check(trimmed));
        if !has_keyword && !has_arrow {
            continue;
        }
        // Require parens for arrow-function-only matches (no keyword), but
        // not for keyword matches — some languages (e.g. Ruby: `def helper`)
        // declare parameterless functions without parentheses. The
        // name-extraction logic in `extract_name_after_keyword` already
        // rejects false positives (e.g. `int x = 5;` → None).
        if !has_keyword && !trimmed.contains('(') {
            continue;
        }

        if let Some(parse_fn) = detector.parse_sig {
            // Full signature parser with parameter analysis
            if let Some(f) = parse_fn(trimmed, file, line_num) {
                let actual = parse_complexity(source, file, lang)
                    .into_iter()
                    .find(|func| func.name == f.name)
                    .map_or(f.complexity, |func| func.complexity);
                functions.push(FuzzableFunction {
                    complexity: actual,
                    ..f
                });
            }
        } else {
            // Basic name extraction for languages without parameter analysis
            let name = detector
                .keywords
                .iter()
                .find_map(|kw| {
                    if trimmed.starts_with(kw) {
                        extract_name_after_keyword(trimmed, kw)
                    } else {
                        None
                    }
                })
                .or_else(|| extract_c_name(trimmed)); // Fallback for C-style

            if let Some(name) = name {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: (detector.is_public)(trimmed),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// ═══════════════════════════════════════════
// RUST-SPECIFIC ANALYSIS (multi-line + harness)
// ═══════════════════════════════════════════

fn analyze_rust_file(
    source: &str,
    file: &str,
    harnesses: &HashSet<String>,
) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let mut in_fn = false;
    let mut fn_sig = String::new();
    let mut fn_start_line = 0;
    let mut brace_depth = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if in_fn {
            fn_sig.push(' ');
            fn_sig.push_str(trimmed);
            brace_depth += trimmed.matches('{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());

            if brace_depth == 0 && trimmed.contains('}') {
                if let Some(mut f) = parse_rust_fn_sig(&fn_sig, file, fn_start_line, harnesses) {
                    f.complexity = parse_complexity(source, file, Language::Rust)
                        .into_iter()
                        .find(|func| func.name == f.name)
                        .map_or(10, |func| func.complexity);
                    functions.push(f);
                }
                in_fn = false;
                fn_sig.clear();
            }
        } else {
            if (trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("async fn "))
                && trimmed.contains('(')
            {
                in_fn = true;
                fn_start_line = line_num;
                fn_sig = trimmed.to_string();
                brace_depth = trimmed.matches('{').count() - trimmed.matches('}').count();
                if brace_depth > 0 {
                    if let Some(f) = parse_rust_fn_sig(&fn_sig, file, fn_start_line, harnesses) {
                        functions.push(f);
                    }
                    in_fn = false;
                    fn_sig.clear();
                }
            }
        }
    }

    functions
}

// ═══════════════════════════════════════════
// PYTHON-SPECIFIC ANALYSIS (indentation tracking)
// ═══════════════════════════════════════════

fn analyze_python_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let mut in_fn = false;
    let mut fn_sig = String::new();
    let mut fn_start_line = 0;
    let mut indent_level = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let current_indent = line.len() - line.trim_start().len();

        if in_fn {
            if current_indent <= indent_level && !trimmed.is_empty() {
                if let Some(mut f) = parse_python_fn_sig(&fn_sig, file, fn_start_line) {
                    f.complexity = parse_complexity(source, file, Language::Python)
                        .into_iter()
                        .find(|func| func.name == f.name)
                        .map_or(10, |func| func.complexity);
                    functions.push(f);
                }
                in_fn = false;
                fn_sig.clear();
            } else {
                fn_sig.push(' ');
                fn_sig.push_str(trimmed);
            }
        } else {
            if (trimmed.starts_with("def ") || trimmed.starts_with("async def "))
                && trimmed.contains(':')
            {
                in_fn = true;
                fn_start_line = line_num;
                fn_sig = trimmed.to_string();
                indent_level = current_indent;

                if let Some(pos) = trimmed.find(':') {
                    if pos + 1 < trimmed.len() {
                        if let Some(f) = parse_python_fn_sig(&fn_sig, file, fn_start_line) {
                            functions.push(f);
                        }
                        in_fn = false;
                        fn_sig.clear();
                    }
                }
            }
        }
    }

    functions
}

// ═══════════════════════════════════════════
// MAIN ENTRY POINT
// ═══════════════════════════════════════════

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let target_path = Path::new(&cli.path);

    let supported_exts = [
        "rs", "py", "js", "ts", "go", "c", "cpp", "h", "cs", "java", "php", "rb", "swift", "kt",
        "kts", "sol",
    ];

    let source_files = if target_path.is_dir() {
        find_source_files(&cli.path, cli.recursive, &supported_exts)
            .into_iter()
            .map(PathBuf::from)
            .collect()
    } else if target_path.is_file() {
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if supported_exts.contains(&ext) {
            vec![target_path.to_path_buf()]
        } else {
            return Err(format!("Unsupported file type: {}", cli.path).into());
        }
    } else {
        return Err(format!("No source files found at {}", cli.path).into());
    };

    if source_files.is_empty() {
        return Err("No supported source files found to analyze."
            .to_string()
            .into());
    }

    let harnesses = find_fuzz_harnesses(target_path);
    let mut all_functions: Vec<FuzzableFunction> = Vec::new();

    for file_path in &source_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let file_str = file_path.to_string_lossy().to_string();
        let lang = Language::from_extension(&file_str);
        let functions = analyze_file(&source, file_path, &harnesses, lang);
        all_functions.extend(functions);
    }

    all_functions.retain(|f| f.score >= cli.min_score);
    all_functions.sort_by_key(|b| std::cmp::Reverse(b.score));

    let display_count = cli.top.min(all_functions.len());
    let display = &all_functions[..display_count];

    match cli.format.as_str() {
        "json" => output_json(display, &all_functions),
        _ => {
            output_table(display, &all_functions);
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════
// FILE DISPATCH
// ═══════════════════════════════════════════

fn analyze_file(
    source: &str,
    file_path: &Path,
    harnesses: &HashSet<String>,
    lang: Language,
) -> Vec<FuzzableFunction> {
    let file_str = file_path.to_string_lossy().to_string();

    // Rust: needs multi-line brace tracking + harness detection
    if lang == Language::Rust {
        return analyze_rust_file(source, &file_str, harnesses);
    }
    // Python: needs indentation-based dedent tracking
    if lang == Language::Python {
        return analyze_python_file(source, &file_str);
    }
    // All other languages: data-driven single-line detection
    detect_functions(source, &file_str, lang)
}

// ═══════════════════════════════════════════
// FUZZ HARNESS DETECTION
// ═══════════════════════════════════════════

fn find_fuzz_harnesses(base: &Path) -> HashSet<String> {
    let fuzz_dir = base.join("fuzz");
    let mut harnesses = HashSet::new();

    if fuzz_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&fuzz_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        extract_harness_names(&content, &mut harnesses);
                    }
                }
            }
        }
    }

    harnesses
}

fn extract_harness_names(content: &str, harnesses: &mut HashSet<String>) {
    for line in content.lines() {
        if line.contains("fuzz_target!") {
            for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if !word.is_empty() && word != "fuzz_target" && word != "libfuzzer" {
                    harnesses.insert(word.to_string());
                }
            }
        }
    }
}

// ═══════════════════════════════════════════
// SIGNATURE PARSERS (with parameter analysis)
// ═══════════════════════════════════════════

fn parse_rust_fn_sig(
    sig: &str,
    file: &str,
    line: usize,
    harnesses: &HashSet<String>,
) -> Option<FuzzableFunction> {
    let after_fn = if let Some(pos) = sig.find("fn ") {
        &sig[pos + 3..]
    } else {
        return None;
    };

    let name_end = after_fn
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(after_fn.len());
    let name = after_fn[..name_end].trim().to_string();

    let params_start = after_fn.find('(')?;
    let params_end = after_fn.rfind(')')?;
    let params_str = &after_fn[params_start + 1..params_end];

    let params: Vec<String> = if params_str.is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    let is_public = sig.trim_start().starts_with("pub ");

    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        if param_lower.contains("&[u8]") || param_lower.contains("bytes") {
            score += 30;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("string") || param_lower.contains("&str") {
            score += 20;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("vec<u8>") {
            score += 25;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("path")
            || param_lower.contains("reader")
            || param_lower.contains("stream")
        {
            score += 10;
            fuzzable_params.push(param.clone());
        }
    }

    if score == 0 {
        return None;
    }

    if is_public {
        score += 10;
    }
    score += params.len() as u32 * 2;

    let complexity = estimate_rust_complexity(sig);
    if complexity > 5 {
        score += 5;
    }

    let has_harness = harnesses.contains(&name);
    if has_harness {
        score = score.saturating_sub(5);
    }

    Some(FuzzableFunction {
        name,
        file: file.to_string(),
        line,
        params: fuzzable_params,
        score,
        is_public,
        complexity,
        has_harness,
    })
}

fn parse_python_fn_sig(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction> {
    let after_def = if let Some(pos) = sig.find("def ") {
        &sig[pos + 4..]
    } else {
        return None;
    };

    let name_end = after_def.find('(')?;
    let name = after_def[..name_end].trim().to_string();

    let params_start = sig.find('(')?;
    let params_end = sig.rfind(')')?;
    let params_str = &sig[params_start + 1..params_end];

    let params: Vec<String> = if params_str.is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        if param_lower.contains("bytes") || param_lower.contains("bytearray") {
            score += 30;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("str") {
            score += 20;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("list") {
            score += 15;
            fuzzable_params.push(param.clone());
        }
    }

    if score == 0 {
        return None;
    }

    let is_public = !name.starts_with('_');
    score += params.len() as u32 * 2;

    let complexity = estimate_python_complexity(sig);
    if complexity > 5 {
        score += 5;
    }

    Some(FuzzableFunction {
        name,
        file: file.to_string(),
        line,
        params: fuzzable_params,
        score,
        is_public,
        complexity,
        has_harness: false,
    })
}

fn parse_js_fn_sig(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction> {
    let name = if let Some(after_func) = sig.strip_prefix("function ") {
        let name_end = after_func.find('(')?;
        after_func[..name_end].trim().to_string()
    } else if sig.starts_with("const ") || sig.starts_with("let ") {
        let after_kw = sig.split_whitespace().nth(1)?;
        let name_part = after_kw.split('=').next()?.trim();
        let name_end = name_part.find('(')?;
        name_part[..name_end].trim().to_string()
    } else {
        return None;
    };

    if name.is_empty() {
        return None;
    }

    let params_start = sig.find('(')?;
    let params_end = sig.rfind(')')?;
    let params_str = &sig[params_start + 1..params_end];

    let params: Vec<String> = if params_str.is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        if param_lower.contains("uint8array") || param_lower.contains("buffer") {
            score += 30;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("string") {
            score += 20;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("array") || param_lower.contains("[]") {
            score += 15;
            fuzzable_params.push(param.clone());
        }
    }

    if score == 0 {
        return None;
    }

    let is_public = true;
    score += params.len() as u32 * 2;

    let complexity = estimate_js_complexity(sig);
    if complexity > 5 {
        score += 5;
    }

    Some(FuzzableFunction {
        name,
        file: file.to_string(),
        line,
        params: fuzzable_params,
        score,
        is_public,
        complexity,
        has_harness: false,
    })
}

fn parse_go_fn_sig(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction> {
    let after_func = if let Some(pos) = sig.find("func ") {
        &sig[pos + 5..]
    } else {
        return None;
    };

    let name_end = after_func.find('(')?;
    let name = after_func[..name_end].trim().to_string();

    let params_start = sig.find('(')?;
    let params_end = sig.rfind(')')?;
    let params_str = &sig[params_start + 1..params_end];

    let params: Vec<String> = if params_str.is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        if param_lower.contains("[]byte") {
            score += 30;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("string") {
            score += 20;
            fuzzable_params.push(param.clone());
        } else if param_lower.contains("interface") {
            score += 10;
            fuzzable_params.push(param.clone());
        }
    }

    if score == 0 {
        return None;
    }

    let is_public = name.chars().next().is_some_and(|c| c.is_uppercase());
    score += params.len() as u32 * 2;

    let complexity = estimate_go_complexity(sig);
    if complexity > 5 {
        score += 5;
    }

    Some(FuzzableFunction {
        name,
        file: file.to_string(),
        line,
        params: fuzzable_params,
        score,
        is_public,
        complexity,
        has_harness: false,
    })
}

// ═══════════════════════════════════════════
// COMPLEXITY ESTIMATORS
// ═══════════════════════════════════════════

fn estimate_rust_complexity(sig: &str) -> u32 {
    let mut complexity = 1;
    if sig.contains("if ") {
        complexity += 1;
    }
    if sig.contains("match ") {
        complexity += 1;
    }
    if sig.contains("for ") {
        complexity += 1;
    }
    if sig.contains("while ") {
        complexity += 1;
    }
    complexity
}

fn estimate_python_complexity(sig: &str) -> u32 {
    let mut complexity = 1;
    if sig.contains("if ") {
        complexity += 1;
    }
    if sig.contains("for ") {
        complexity += 1;
    }
    if sig.contains("while ") {
        complexity += 1;
    }
    if sig.contains("except ") {
        complexity += 1;
    }
    complexity
}

fn estimate_js_complexity(sig: &str) -> u32 {
    let mut complexity = 1;
    if sig.contains("if") {
        complexity += 1;
    }
    if sig.contains("for") {
        complexity += 1;
    }
    if sig.contains("while") {
        complexity += 1;
    }
    if sig.contains("switch") {
        complexity += 1;
    }
    complexity
}

fn estimate_go_complexity(sig: &str) -> u32 {
    let mut complexity = 1;
    if sig.contains("if ") {
        complexity += 1;
    }
    if sig.contains("for ") {
        complexity += 1;
    }
    if sig.contains("switch ") {
        complexity += 1;
    }
    if sig.contains("select ") {
        complexity += 1;
    }
    complexity
}

// ═══════════════════════════════════════════
// HINT GENERATION
// ═══════════════════════════════════════════

fn get_fuzz_hint(f: &FuzzableFunction) -> String {
    if f.has_harness {
        "Already has fuzz harness. Consider expanding test cases.".to_string()
    } else if f.score >= 30 {
        format!(
            "High priority: Add fuzz harness (score: {}). Use cargo-fuzz or similar framework. Start with boundary values.",
            f.score
        )
    } else if f.score >= 20 {
        format!(
            "Medium priority: Consider fuzzing (score: {}). Good candidate for property-based testing.",
            f.score
        )
    } else if !f.is_public {
        "Private function. Consider if it should be public for testing.".to_string()
    } else {
        format!(
            "Low priority: Score {} is acceptable. Monitor for complexity growth.",
            f.score
        )
    }
}

// ═══════════════════════════════════════════
// OUTPUT FORMATTERS
// ═══════════════════════════════════════════

fn output_table(display: &[FuzzableFunction], all: &[FuzzableFunction]) {
    println!("FUZZING SURFACE ANALYSIS");
    println!("{}", separator(95));

    let columns = [
        Column::left("FUNCTION", 25),
        Column::left("FILE", 20),
        Column::right("LINE", 5),
        Column::right("SCORE", 6),
        Column::left("PARAMS", 15),
        Column::left("HINT", 40),
    ];
    print_table_header(&columns);

    for f in display {
        let params_str = f.params.join(", ");
        let harness_icon = if f.has_harness { "✓" } else { "·" };
        let pub_icon = if f.is_public { "[pub]" } else { "[priv]" };
        let name_with_icons = format!("{} {} {}", harness_icon, pub_icon, f.name);
        let line_str = f.line.to_string();
        let score_str = f.score.to_string();
        let file_short = truncate(&f.file, 19);
        let hint = get_fuzz_hint(f);
        let hint_truncated = if hint.len() > 37 { &hint[0..37] } else { &hint };

        print_table_row(
            &columns,
            &[
                &name_with_icons,
                &file_short,
                &line_str,
                &score_str,
                &truncate(&params_str, 14),
                hint_truncated,
            ],
        );
    }

    println!("{}", separator(95));

    let fuzzable_count = all.len();
    let with_harnesses = all.iter().filter(|f| f.has_harness).count();
    let avg_score = if fuzzable_count > 0 {
        all.iter().map(|f| f.score).sum::<u32>() as f64 / fuzzable_count as f64
    } else {
        0.0
    };

    println!();
    println!("  Total functions analyzed: {}", all.len());
    println!("  Fuzzable functions:     {}", fuzzable_count);
    println!("  With harnesses:           {}", with_harnesses);
    println!(
        "  Without harnesses:        {}",
        fuzzable_count - with_harnesses
    );
    println!("  Avg fuzzability score:    {:.1}", avg_score);

    if fuzzable_count > with_harnesses {
        println!();
        println!(
            "  {} function(s) could benefit from fuzzing harnesses.",
            fuzzable_count - with_harnesses
        );
    }
}

fn output_json(
    display: &[FuzzableFunction],
    all: &[FuzzableFunction],
) -> Result<(), Box<dyn std::error::Error>> {
    let fuzzable_count = all.len();
    let with_harnesses = all.iter().filter(|f| f.has_harness).count();
    let avg_score = if fuzzable_count > 0 {
        all.iter().map(|f| f.score).sum::<u32>() as f64 / fuzzable_count as f64
    } else {
        0.0
    };

    let report = FuzzReport {
        functions: display.to_vec(),
        summary: FuzzSummary {
            total_functions: all.len(),
            fuzzable_functions: fuzzable_count,
            functions_with_harnesses: with_harnesses,
            avg_score,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_fn_sig_fuzzable() {
        let harnesses = HashSet::new();
        let f = parse_rust_fn_sig(
            "pub fn parse_data(data: &[u8]) -> Result<String, Error> { }",
            "test.rs",
            1,
            &harnesses,
        )
        .unwrap();
        assert_eq!(f.name, "parse_data");
        assert!(f.is_public);
        assert_eq!(f.score, 42); // 30 for &[u8] + 10 for pub + 1 param*2
        assert!(f.params.iter().any(|p| p.contains("u8")));
    }

    #[test]
    fn test_parse_rust_fn_sig_not_fuzzable() {
        let harnesses = HashSet::new();
        let f = parse_rust_fn_sig(
            "fn internal_helper(x: i32) -> i32 { }",
            "test.rs",
            1,
            &harnesses,
        );
        assert!(f.is_none(), "No fuzzable params should return None");
    }

    #[test]
    fn test_parse_python_fn_sig_fuzzable() {
        let f = parse_python_fn_sig("def process_data(data: bytes) -> str:", "test.py", 1).unwrap();
        assert_eq!(f.name, "process_data");
        assert!(f.is_public);
        assert_eq!(f.score, 32); // 30 for bytes + 1 param*2
        assert!(f.params.iter().any(|p| p.contains("bytes")));
    }

    #[test]
    fn test_parse_js_fn_sig_fuzzable() {
        let f =
            parse_js_fn_sig("function parseData(data: string): string {", "test.js", 1).unwrap();
        assert_eq!(f.name, "parseData");
        assert!(f.is_public);
        assert_eq!(f.score, 22); // 20 for string + 1 param*2
        assert!(f.params.iter().any(|p| p.contains("string")));
    }

    #[test]
    fn test_parse_go_fn_sig_fuzzable() {
        let f = parse_go_fn_sig("func ParseData(data []byte) string {", "test.go", 1).unwrap();
        assert_eq!(f.name, "ParseData");
        assert!(f.is_public);
        assert_eq!(f.score, 32); // 30 for []byte + 1 param*2
        assert!(f.params.iter().any(|p| p.contains("[]byte")));
    }

    #[test]
    fn test_harness_detection() {
        let mut harnesses = HashSet::new();
        harnesses.insert("parse_data".to_string());
        let f = parse_rust_fn_sig(
            "pub fn parse_data(data: &[u8]) -> Result<String, Error> { }",
            "test.rs",
            1,
            &harnesses,
        )
        .unwrap();
        assert!(f.has_harness);
        assert_eq!(f.score, 37); // 30 + 10 + 1 param*2 - 5 for having harness
    }

    // ── Generic detector tests ─────────────────────

    #[test]
    fn test_detect_functions_go() {
        let src = "package main\nfunc ParseData(data []byte) string {\n    return string(data)\n}\nfunc helper(x int) int { return x }\n";
        let funcs = detect_functions(src, "test.go", Language::Go);
        assert_eq!(funcs.len(), 1, "Only ParseData has fuzzable params");
        assert_eq!(funcs[0].name, "ParseData");
    }

    #[test]
    fn test_detect_functions_c() {
        let src =
            "int process_data(char *data, int len) {\n    return 0;\n}\nvoid helper(void) {}\n";
        let funcs = detect_functions(src, "test.c", Language::C);
        assert_eq!(funcs.len(), 2, "C should detect both functions");
        assert_eq!(funcs[0].name, "process_data");
        assert_eq!(funcs[1].name, "helper");
    }

    #[test]
    fn test_detect_functions_php() {
        let src = "<?php\nfunction process_data($data) { return $data; }\nfunction helper() { return 1; }\n";
        let funcs = detect_functions(src, "test.php", Language::Php);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "process_data");
    }

    #[test]
    fn test_detect_functions_ruby() {
        let src = "def process_data(data)\n  data\nend\ndef helper\n  1\nend\n";
        let funcs = detect_functions(src, "test.rb", Language::Ruby);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "process_data");
    }

    #[test]
    fn test_detect_functions_swift() {
        let src = "func processData(data: String) -> String { return data }\nfunc helper() -> Int { return 1 }\n";
        let funcs = detect_functions(src, "test.swift", Language::Swift);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "processData");
    }

    #[test]
    fn test_extract_name_after_keyword() {
        assert_eq!(
            extract_name_after_keyword("fn foo(x: i32)", "fn "),
            Some("foo".to_string())
        );
        assert_eq!(
            extract_name_after_keyword("def bar():", "def "),
            Some("bar".to_string())
        );
        assert_eq!(
            extract_name_after_keyword("function baz() {", "function "),
            Some("baz".to_string())
        );
        assert_eq!(
            extract_name_after_keyword("func qux()", "func "),
            Some("qux".to_string())
        );
    }

    // ── FnDetector table tests ──────────────

    #[test]
    fn test_fn_detector_go_keywords() {
        let d = fn_detector(Language::Go).unwrap();
        assert_eq!(d.keywords, &["func "]);
        assert!(d.parse_sig.is_some());
        assert!(d.extra_check.is_none());
    }

    #[test]
    fn test_fn_detector_js_has_arrow_check() {
        let d = fn_detector(Language::JavaScript).unwrap();
        assert!(d.keywords.contains(&"function "));
        assert!(d.keywords.contains(&"const "));
        assert!(d.parse_sig.is_some());
        assert!(d.extra_check.is_some());
    }

    #[test]
    fn test_fn_detector_ruby_keywords() {
        let d = fn_detector(Language::Ruby).unwrap();
        assert_eq!(d.keywords, &["def "]);
        assert!(d.parse_sig.is_none());
        assert!((d.is_public)("def foo"));
    }

    #[test]
    fn test_fn_detector_c_keywords() {
        let d = fn_detector(Language::C).unwrap();
        assert!(d.keywords.contains(&"int "));
        assert!(d.keywords.contains(&"void "));
        assert!(d.parse_sig.is_none());
        assert!((d.is_public)("int foo()"));
    }

    #[test]
    fn test_fn_detector_csharp_public_check() {
        let d = fn_detector(Language::CSharp).unwrap();
        assert!((d.is_public)("public void Foo()"));
        assert!(!(d.is_public)("private void Foo()"));
    }

    #[test]
    fn test_fn_detector_java_public_check() {
        let d = fn_detector(Language::Java).unwrap();
        assert!((d.is_public)("public void Foo()"));
        assert!(!(d.is_public)("private void Foo()"));
    }

    #[test]
    fn test_fn_detector_swift_public_check() {
        let d = fn_detector(Language::Swift).unwrap();
        assert!((d.is_public)("public func foo()"));
        assert!((d.is_public)("open func foo()"));
        assert!(!(d.is_public)("func foo()"));
    }

    #[test]
    fn test_fn_detector_kotlin_keywords() {
        let d = fn_detector(Language::Kotlin).unwrap();
        assert_eq!(d.keywords, &["fun "]);
    }

    #[test]
    fn test_fn_detector_solidity_keywords() {
        let d = fn_detector(Language::Solidity).unwrap();
        assert_eq!(d.keywords, &["function "]);
    }

    #[test]
    fn test_fn_detector_rust_python_return_none() {
        // Rust and Python are special-cased (multi-line), not in the table
        assert!(fn_detector(Language::Rust).is_none());
        assert!(fn_detector(Language::Python).is_none());
    }

    #[test]
    fn test_fn_detector_unknown_return_none() {
        assert!(fn_detector(Language::Unknown).is_none());
    }

    // ── Edge case detection tests ───────────

    #[test]
    fn test_detect_functions_csharp() {
        let src = "public void ProcessData(string data) { }\nprivate void Helper() { }\n";
        let funcs = detect_functions(src, "test.cs", Language::CSharp);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "ProcessData");
        assert!(funcs[0].is_public);
        assert!(!funcs[1].is_public);
    }

    #[test]
    fn test_detect_functions_java() {
        let src = "public void processData(String data) { }\nprivate void helper() { }\n";
        let funcs = detect_functions(src, "test.java", Language::Java);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "processData");
        assert!(funcs[0].is_public);
        assert!(!funcs[1].is_public);
    }

    #[test]
    fn test_detect_functions_kotlin() {
        let src = "fun processData(data: String) { }\nfun helper() { }\n";
        let funcs = detect_functions(src, "test.kt", Language::Kotlin);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "processData");
    }

    #[test]
    fn test_detect_functions_solidity() {
        let src =
            "function processData(bytes memory data) public { }\nfunction helper() private { }\n";
        let funcs = detect_functions(src, "test.sol", Language::Solidity);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "processData");
    }

    #[test]
    fn test_detect_functions_cpp() {
        let src = "int process_data(char *data, int len) {\n    return 0;\n}\nvoid helper() {}\n";
        let funcs = detect_functions(src, "test.cpp", Language::Cpp);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "process_data");
    }

    #[test]
    fn test_detect_functions_js_fuzzable() {
        // parse_js_fn_sig requires fuzzable params (string, buffer, uint8array, array)
        let src = "function parseBuffer(data: Buffer) { return data; }\n";
        let funcs = detect_functions(src, "test.js", Language::JavaScript);
        assert_eq!(funcs.len(), 1, "Only parseBuffer has fuzzable params");
        assert_eq!(funcs[0].name, "parseBuffer");
    }

    #[test]
    fn test_detect_functions_ignores_comments() {
        let src = "// fn unused()\n/* fn commented() */\n# fn hash_comment()\nfn real() { }\n";
        let funcs = detect_functions(src, "test.rs", Language::Rust);
        // Rust is special-cased, so detect_functions returns empty for it
        assert!(funcs.is_empty());
    }
}
