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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let target_path = Path::new(&cli.path);

    // Supported languages for fuzzing analysis
    let supported_exts = [
        "rs", "py", "js", "ts", "go", "c", "cpp", "h", "cs", "java", "php", "rb", "swift", "kt",
        "kts", "sol", // Solidity smart contracts
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

    // Check for existing fuzz harnesses (Rust only for now)
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

    // Filter by min score and sort by score descending
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
    // Look for fuzz_target! macro invocations: fuzz_target!(|data: &[u8]| { ... })
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

fn analyze_file(
    source: &str,
    file_path: &Path,
    harnesses: &HashSet<String>,
    lang: Language,
) -> Vec<FuzzableFunction> {
    let file_str = file_path.to_string_lossy().to_string();

    // Use language-specific analysis
    match lang {
        Language::Rust => analyze_rust_file(source, &file_str, harnesses),
        Language::Python => analyze_python_file(source, &file_str),
        Language::JavaScript | Language::TypeScript => analyze_js_file(source, &file_str),
        Language::Go => analyze_go_file(source, &file_str),
        Language::C | Language::Cpp => analyze_c_file(source, &file_str),
        Language::CSharp => analyze_csharp_file(source, &file_str),
        Language::Java => analyze_java_file(source, &file_str),
        Language::Php => analyze_php_file(source, &file_str),
        Language::Ruby => analyze_ruby_file(source, &file_str),
        Language::Swift => analyze_swift_file(source, &file_str),
        Language::Kotlin => analyze_kotlin_file(source, &file_str),
        Language::Solidity => analyze_solidity_file(source, &file_str),
        _ => Vec::new(),
    }
}

fn analyze_rust_file(
    source: &str,
    file: &str,
    harnesses: &HashSet<String>,
) -> Vec<FuzzableFunction> {
    // Simple heuristic-based analysis (string-based, no full AST parse)
    let mut functions = Vec::new();
    let mut in_fn = false;
    let mut fn_sig = String::new();
    let mut fn_start_line = 0;
    let mut brace_depth = 0;
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if in_fn {
            fn_sig.push(' ');
            fn_sig.push_str(trimmed);
            brace_depth += trimmed.matches('{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());

            if brace_depth == 0 && trimmed.contains('}') {
                // End of function - process signature
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
            // Check for function signature
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

fn analyze_python_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let mut in_fn = false;
    let mut fn_sig = String::new();
    let mut fn_start_line = 0;
    let mut indent_level = 0;
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Track indentation level for Python
        let current_indent = line.len() - line.trim_start().len();

        if in_fn {
            if current_indent <= indent_level && !trimmed.is_empty() {
                // End of function (dedent)
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
            // Check for function signature
            if (trimmed.starts_with("def ") || trimmed.starts_with("async def "))
                && trimmed.contains(':')
            {
                in_fn = true;
                fn_start_line = line_num;
                fn_sig = trimmed.to_string();
                indent_level = current_indent;

                // Handle single-line functions
                if let Some(pos) = trimmed.find(':') {
                    if pos + 1 < trimmed.len() {
                        // Has code after colon - single line
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

fn analyze_js_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Look for function definitions
        if (trimmed.starts_with("function ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.contains("=>"))
            && trimmed.contains('(')
        {
            if let Some(mut f) = parse_js_fn_sig(trimmed, file, line_num) {
                f.complexity = parse_complexity(source, file, Language::JavaScript)
                    .into_iter()
                    .find(|func| func.name == f.name)
                    .map_or(10, |func| func.complexity);
                functions.push(f);
            }
        }
    }

    functions
}

fn analyze_go_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Look for Go function definitions
        if trimmed.starts_with("func ") && trimmed.contains('(') {
            if let Some(mut f) = parse_go_fn_sig(trimmed, file, line_num) {
                f.complexity = parse_complexity(source, file, Language::Go)
                    .into_iter()
                    .find(|func| func.name == f.name)
                    .map_or(10, |func| func.complexity);
                functions.push(f);
            }
        }
    }

    functions
}

// C/C++ function analysis (simplified)
fn analyze_c_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Detect C/C++ function declarations
        let is_func = (trimmed.starts_with("int ")
            || trimmed.starts_with("void ")
            || trimmed.starts_with("char ")
            || trimmed.starts_with("float ")
            || trimmed.starts_with("double ")
            || trimmed.starts_with("bool ")
            || trimmed.starts_with("size_t ")
            || trimmed.starts_with("uint")
            || trimmed.starts_with("int"))
            && trimmed.contains('(')
            && trimmed.contains(')');

        if is_func {
            let name = trimmed
                .split_whitespace()
                .find(|t| t.contains('('))
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
                .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'));

            if let Some(name) = name {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: true,
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// C# function analysis (simplified)
fn analyze_csharp_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments and attributes
        if trimmed.starts_with("//") || trimmed.starts_with("[") || trimmed.starts_with("*") {
            continue;
        }

        // Detect C# methods
        let is_func = trimmed.contains('(')
            && (trimmed.starts_with("public ")
                || trimmed.starts_with("private ")
                || trimmed.starts_with("protected ")
                || trimmed.starts_with("internal ")
                || trimmed.starts_with("static ")
                || trimmed.starts_with("void ")
                || trimmed.starts_with("int ")
                || trimmed.starts_with("string ")
                || trimmed.starts_with("bool ")
                || trimmed.starts_with("var "));

        if is_func {
            let name = trimmed
                .split_whitespace()
                .find(|t| t.contains('('))
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
                .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'));

            if let Some(name) = name {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: trimmed.starts_with("public "),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// Java function analysis (simplified)
fn analyze_java_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("@") {
            continue;
        }

        // Detect Java methods
        let is_func = trimmed.contains('(')
            && (trimmed.starts_with("public ")
                || trimmed.starts_with("private ")
                || trimmed.starts_with("protected ")
                || trimmed.starts_with("static ")
                || trimmed.starts_with("void ")
                || trimmed.starts_with("int ")
                || trimmed.starts_with("String ")
                || trimmed.starts_with("boolean ")
                || trimmed.starts_with("long ")
                || trimmed.starts_with("double "));

        if is_func {
            let name = trimmed
                .split_whitespace()
                .find(|t| t.contains('('))
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
                .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'));

            if let Some(name) = name {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: trimmed.starts_with("public "),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// PHP function analysis (simplified)
fn analyze_php_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Detect PHP functions
        if trimmed.starts_with("function ") && trimmed.contains('(') {
            let name = trimmed
                .strip_prefix("function ")
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: true,
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// Ruby function analysis (simplified)
fn analyze_ruby_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("def ") {
            let name = trimmed
                .strip_prefix("def ")
                .unwrap_or("")
                .split(|c: char| ['(', ' '].contains(&c))
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '?')
            {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: true,
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// Swift function analysis (simplified)
fn analyze_swift_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Detect Swift functions
        if trimmed.starts_with("func ") && trimmed.contains('(') {
            let name = trimmed
                .strip_prefix("func ")
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: trimmed.starts_with("public ") || trimmed.starts_with("open "),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// Kotlin function analysis (simplified)
fn analyze_kotlin_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Detect Kotlin functions
        if trimmed.starts_with("fun ") && trimmed.contains('(') {
            let name = trimmed
                .strip_prefix("fun ")
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: trimmed.starts_with("public ")
                        || !trimmed.starts_with("private ")
                        || trimmed.starts_with("internal "),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

// Solidity function analysis (simplified)
fn analyze_solidity_file(source: &str, file: &str) -> Vec<FuzzableFunction> {
    let mut functions = Vec::new();
    let _line_num = 0;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Detect Solidity functions
        if trimmed.starts_with("function ") && trimmed.contains('(') {
            let name = trimmed
                .strip_prefix("function ")
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                functions.push(FuzzableFunction {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    params: vec![],
                    score: 10,
                    is_public: trimmed.starts_with("public ")
                        || !trimmed.starts_with("private ")
                        || trimmed.starts_with("internal "),
                    complexity: 1,
                    has_harness: false,
                });
            }
        }
    }

    functions
}

fn parse_rust_fn_sig(
    sig: &str,
    file: &str,
    line: usize,
    harnesses: &HashSet<String>,
) -> Option<FuzzableFunction> {
    // Extract function name
    let after_fn = if let Some(pos) = sig.find("fn ") {
        &sig[pos + 3..]
    } else {
        return None;
    };

    let name_end = after_fn
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(after_fn.len());
    let name = after_fn[..name_end].trim().to_string();

    // Extract parameters
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

    // Check visibility
    let is_public = sig.trim_start().starts_with("pub ");

    // Calculate fuzzability score
    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        // Raw byte input is very fuzzable
        if param_lower.contains("&[u8]") || param_lower.contains("bytes") {
            score += 30;
            fuzzable_params.push(param.clone());
        }
        // String inputs are good fuzz targets
        else if param_lower.contains("string") || param_lower.contains("&str") {
            score += 20;
            fuzzable_params.push(param.clone());
        }
        // Vec<u8> is also fuzzable
        else if param_lower.contains("vec<u8>") {
            score += 25;
            fuzzable_params.push(param.clone());
        }
        // Path/IO types can be fuzz targets
        else if param_lower.contains("path")
            || param_lower.contains("reader")
            || param_lower.contains("stream")
        {
            score += 10;
            fuzzable_params.push(param.clone());
        }
    }

    // No fuzzable params = not worth fuzzing
    if score == 0 {
        return None;
    }

    // Public functions are more valuable targets (higher impact)
    if is_public {
        score += 10;
    }

    // More parameters = more combinations to explore
    score += params.len() as u32 * 2;

    // Functions with more complexity are more likely to have bugs
    let complexity = estimate_rust_complexity(sig);
    if complexity > 5 {
        score += 5;
    }

    let has_harness = harnesses.contains(&name);
    if has_harness {
        // Already has a harness, reduce score (not a gap)
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
    // Extract function name
    let after_def = if let Some(pos) = sig.find("def ") {
        &sig[pos + 4..]
    } else {
        return None;
    };

    let name_end = after_def.find('(')?;
    let name = after_def[..name_end].trim().to_string();

    // Extract parameters
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

    // Calculate fuzzability score
    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        // Raw byte input is very fuzzable
        if param_lower.contains("bytes") || param_lower.contains("bytearray") {
            score += 30;
            fuzzable_params.push(param.clone());
        }
        // String inputs are good fuzz targets
        else if param_lower.contains("str") {
            score += 20;
            fuzzable_params.push(param.clone());
        }
        // Lists can be fuzz targets
        else if param_lower.contains("list") {
            score += 15;
            fuzzable_params.push(param.clone());
        }
    }

    // No fuzzable params = not worth fuzzing
    if score == 0 {
        return None;
    }

    // Python functions don't have explicit visibility, but we can infer from name
    let is_public = !name.starts_with('_');

    // More parameters = more combinations to explore
    score += params.len() as u32 * 2;

    // Functions with more complexity are more likely to have bugs
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
        has_harness: false, // No harness tracking for Python yet
    })
}

fn parse_js_fn_sig(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction> {
    // Extract function name
    let name = if let Some(after_func) = sig.strip_prefix("function ") {
        let name_end = after_func.find('(')?;
        after_func[..name_end].trim().to_string()
    } else if sig.starts_with("const ") || sig.starts_with("let ") {
        // Handle arrow functions: const foo = (a, b) => {}
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

    // Extract parameters
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

    // Calculate fuzzability score
    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        // Raw byte input is very fuzzable (Uint8Array)
        if param_lower.contains("uint8array") || param_lower.contains("buffer") {
            score += 30;
            fuzzable_params.push(param.clone());
        }
        // String inputs are good fuzz targets
        else if param_lower.contains("string") {
            score += 20;
            fuzzable_params.push(param.clone());
        }
        // Arrays can be fuzz targets
        else if param_lower.contains("array") || param_lower.contains("[]") {
            score += 15;
            fuzzable_params.push(param.clone());
        }
    }

    // No fuzzable params = not worth fuzzing
    if score == 0 {
        return None;
    }

    // JavaScript functions don't have explicit visibility
    let is_public = true;

    // More parameters = more combinations to explore
    score += params.len() as u32 * 2;

    // Functions with more complexity are more likely to have bugs
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
        has_harness: false, // No harness tracking for JS yet
    })
}

fn parse_go_fn_sig(sig: &str, file: &str, line: usize) -> Option<FuzzableFunction> {
    // Extract function name
    let after_func = if let Some(pos) = sig.find("func ") {
        &sig[pos + 5..]
    } else {
        return None;
    };

    let name_end = after_func.find('(')?;
    let name = after_func[..name_end].trim().to_string();

    // Extract parameters
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

    // Calculate fuzzability score
    let mut score = 0u32;
    let mut fuzzable_params = Vec::new();

    for param in &params {
        let param_lower = param.to_lowercase();
        // Raw byte input is very fuzzable
        if param_lower.contains("[]byte") {
            score += 30;
            fuzzable_params.push(param.clone());
        }
        // String inputs are good fuzz targets
        else if param_lower.contains("string") {
            score += 20;
            fuzzable_params.push(param.clone());
        }
        // Interfaces can be fuzz targets
        else if param_lower.contains("interface") {
            score += 10;
            fuzzable_params.push(param.clone());
        }
    }

    // No fuzzable params = not worth fuzzing
    if score == 0 {
        return None;
    }

    // Go functions starting with uppercase are exported (public)
    let is_public = name.chars().next().is_some_and(|c| c.is_uppercase());

    // More parameters = more combinations to explore
    score += params.len() as u32 * 2;

    // Functions with more complexity are more likely to have bugs
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
        has_harness: false, // No harness tracking for Go yet
    })
}

fn estimate_rust_complexity(sig: &str) -> u32 {
    // Simple heuristic: count control flow keywords in signature
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
    // Simple heuristic: count control flow keywords
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
    // Simple heuristic: count control flow keywords
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
    // Simple heuristic: count control flow keywords
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

/// Generate guided hints for fuzzable functions
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
}
