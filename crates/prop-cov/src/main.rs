#![deny(clippy::all)]

use ast_parse_ts::Language;
use clap::Parser;
use cogent_common::{
    find_source_files, print_table_header, print_table_row, separator, truncate, Column,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "propcov",
    about = "Property-based testing coverage — scan for proptest/quickcheck macros and calculate coverage"
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

    /// Only scan test files (files in tests/ or with #[test] attribute)
    #[arg(long, default_value = "false")]
    only_tests: bool,

    /// Minimum coverage percentage to report (0-100)
    #[arg(long, default_value = "0")]
    min_coverage: u32,
}

#[derive(Debug, Clone, Serialize)]
struct PropertyTest {
    name: String,
    file: String,
    line: usize,
    framework: String, // "proptest", "quickcheck", "custom"
    functions_tested: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FunctionCoverage {
    name: String,
    file: String,
    line: usize,
    has_property_test: bool,
    has_unit_test: bool,
    property_tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PropCovReport {
    property_tests: Vec<PropertyTest>,
    function_coverage: Vec<FunctionCoverage>,
    summary: PropCovSummary,
}

#[derive(Debug, Clone, Serialize)]
struct PropCovSummary {
    total_functions: usize,
    with_property_tests: usize,
    with_unit_tests: usize,
    property_test_count: usize,
    unit_test_count: usize,
    coverage_percentage: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

const PROP_COV_EXTS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "java", "cs", "php", "rb", "swift",
];

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let target_path = Path::new(&cli.path);

    let source_files = if target_path.is_dir() {
        find_source_files(cli.path.as_str(), cli.recursive, PROP_COV_EXTS)
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| !cli.only_tests || is_test_file(p))
            .collect()
    } else if target_path.is_file() {
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if PROP_COV_EXTS.contains(&ext) {
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

    let mut all_property_tests: Vec<PropertyTest> = Vec::new();
    let mut total_unit_tests = 0usize;
    let mut function_coverage: HashMap<String, FunctionCoverage> = HashMap::new();

    for file_path in &source_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let file_str = file_path.to_string_lossy().to_string();
        let lang = Language::from_extension(&file_str);
        let (props, units, funcs) = analyze_file(&source, &file_str, lang);
        all_property_tests.extend(props);
        total_unit_tests += units;

        for (name, cov) in funcs {
            function_coverage
                .entry(name.clone())
                .and_modify(|existing| {
                    existing.has_property_test =
                        existing.has_property_test || cov.has_property_test;
                    existing.has_unit_test = existing.has_unit_test || cov.has_unit_test;
                    let props: Vec<String> = cov.property_tests.clone();
                    existing.property_tests.extend(props);
                })
                .or_insert(cov);
        }
    }

    let total_functions = function_coverage.len();
    let with_property_tests = function_coverage
        .values()
        .filter(|f| f.has_property_test)
        .count();
    let with_unit_tests = function_coverage
        .values()
        .filter(|f| f.has_unit_test)
        .count();

    let coverage_percentage = if total_functions > 0 {
        with_property_tests as f64 / total_functions as f64 * 100.0
    } else {
        0.0
    };

    let report = PropCovReport {
        property_tests: all_property_tests.clone(),
        function_coverage: function_coverage.values().cloned().collect(),
        summary: PropCovSummary {
            total_functions,
            with_property_tests,
            with_unit_tests,
            property_test_count: all_property_tests.len(),
            unit_test_count: total_unit_tests,
            coverage_percentage,
        },
    };

    match cli.format.as_str() {
        "json" => output_json(&report),
        _ => {
            output_table(&report, cli.min_coverage);
            Ok(())
        }
    }
}

fn analyze_file(
    source: &str,
    file: &str,
    lang: Language,
) -> (Vec<PropertyTest>, usize, HashMap<String, FunctionCoverage>) {
    match lang {
        Language::Rust => analyze_file_rust(source, file),
        Language::Python => analyze_file_python(source, file),
        Language::JavaScript | Language::TypeScript => analyze_file_js(source, file),
        Language::Go => analyze_file_go(source, file),
        _ => (Vec::new(), 0, HashMap::new()),
    }
}

fn analyze_file_rust(
    source: &str,
    file: &str,
) -> (Vec<PropertyTest>, usize, HashMap<String, FunctionCoverage>) {
    let mut property_tests = Vec::new();
    let mut unit_tests = 0usize;
    let mut functions = HashMap::new();
    let mut line_num = 0;

    for line in source.lines() {
        line_num += 1;
        let trimmed = line.trim();

        if trimmed.starts_with("#[test]") || trimmed.contains("# [ test ]") {
            unit_tests += 1;
        }

        if trimmed.contains("proptest!") {
            let props = extract_proptest_names(line, line_num, file);
            property_tests.extend(props);
        }

        if trimmed.contains("quickcheck!")
            || trimmed.contains("#[quickcheck]")
            || trimmed.contains("# [ quickcheck ]")
        {
            let props = extract_quickcheck_names(line, line_num, file);
            property_tests.extend(props);
        }

        if trimmed.starts_with("fn ") && trimmed.contains("prop") {
            let fn_name = extract_fn_name(trimmed);
            if let Some(name) = fn_name {
                let pt = PropertyTest {
                    name: name.clone(),
                    file: file.to_string(),
                    line: line_num,
                    framework: "proptest_inline".to_string(),
                    functions_tested: vec![name.clone()],
                };
                property_tests.push(pt);
            }
        }

        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            if let Some(name) = extract_fn_name(trimmed) {
                functions.insert(
                    name.clone(),
                    FunctionCoverage {
                        name,
                        file: file.to_string(),
                        line: line_num,
                        has_property_test: false,
                        has_unit_test: false,
                        property_tests: Vec::new(),
                    },
                );
            }
        }
    }

    for pt in &property_tests {
        for func_name in &pt.functions_tested {
            if let Some(func) = functions.get_mut(func_name) {
                func.has_property_test = true;
                func.property_tests.push(pt.name.clone());
            }
        }
    }

    for (_name, func) in functions.iter_mut() {
        if unit_tests > 0 {
            func.has_unit_test = true;
        }
    }

    (property_tests, unit_tests, functions)
}

fn analyze_file_python(
    source: &str,
    file: &str,
) -> (Vec<PropertyTest>, usize, HashMap<String, FunctionCoverage>) {
    let mut property_tests = Vec::new();
    let mut unit_tests = 0usize;
    let mut functions = HashMap::new();
    let mut line_num = 0;

    for line in source.lines() {
        line_num += 1;
        let trimmed = line.trim();

        if trimmed.starts_with("def test_") || trimmed.starts_with("async def test_") {
            unit_tests += 1;
            if let Some(name) = extract_python_fn_name(trimmed) {
                functions.entry(name.clone()).or_insert(FunctionCoverage {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    has_property_test: false,
                    has_unit_test: true,
                    property_tests: Vec::new(),
                });
            }
        }

        if trimmed.contains("@given") || trimmed.contains("@hypothesis.given") {
            let next_line = source.lines().nth(line_num);
            if let Some(def_line) = next_line {
                if let Some(name) = extract_python_fn_name(def_line.trim()) {
                    property_tests.push(PropertyTest {
                        name: name.clone(),
                        file: file.to_string(),
                        line: line_num + 1,
                        framework: "hypothesis".to_string(),
                        functions_tested: vec![name.clone()],
                    });
                    if let Some(func) = functions.get_mut(&name) {
                        func.has_property_test = true;
                        func.property_tests.push(name.clone());
                    }
                }
            }
        }

        if trimmed.starts_with("def ") {
            if let Some(name) = extract_python_fn_name(trimmed) {
                functions.entry(name.clone()).or_insert(FunctionCoverage {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    has_property_test: false,
                    has_unit_test: false,
                    property_tests: Vec::new(),
                });
            }
        }
    }

    (property_tests, unit_tests, functions)
}

fn analyze_file_js(
    source: &str,
    file: &str,
) -> (Vec<PropertyTest>, usize, HashMap<String, FunctionCoverage>) {
    let mut property_tests = Vec::new();
    let mut unit_tests = 0usize;
    let mut functions = HashMap::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("it(")
            || trimmed.starts_with("test(")
            || trimmed.starts_with("describe(")
        {
            unit_tests += 1;
        }

        if trimmed.contains("fc.assert")
            || trimmed.contains("fastCheck")
            || trimmed.contains("property(")
        {
            let fn_name = if trimmed.contains("function ") {
                trimmed
                    .split("function ")
                    .nth(1)
                    .and_then(|s| s.split(|c: char| c == '(' || c.is_whitespace()).next())
                    .map(|s| s.to_string())
            } else if trimmed.contains("const ") && trimmed.contains("=") {
                trimmed
                    .split("const ")
                    .nth(1)
                    .and_then(|s| s.split(|c: char| c == '=' || c.is_whitespace()).next())
                    .map(|s| s.to_string())
            } else {
                None
            };
            if let Some(name) = fn_name {
                property_tests.push(PropertyTest {
                    name: name.clone(),
                    file: file.to_string(),
                    line: line_num,
                    framework: "fast-check".to_string(),
                    functions_tested: vec![name.clone()],
                });
            }
        }

        if trimmed.starts_with("function ")
            || (trimmed.contains("const ") && trimmed.contains("=>"))
        {
            let fn_name = if trimmed.starts_with("function ") {
                trimmed
                    .split("function ")
                    .nth(1)
                    .and_then(|s| s.split(|c: char| c == '(' || c.is_whitespace()).next())
                    .map(|s| s.to_string())
            } else {
                trimmed
                    .split("const ")
                    .nth(1)
                    .and_then(|s| s.split(|c: char| c == '=' || c.is_whitespace()).next())
                    .map(|s| s.to_string())
            };
            if let Some(name) = fn_name {
                functions.insert(
                    name.clone(),
                    FunctionCoverage {
                        name,
                        file: file.to_string(),
                        line: line_num,
                        has_property_test: false,
                        has_unit_test: false,
                        property_tests: Vec::new(),
                    },
                );
            }
        }
    }

    (property_tests, unit_tests, functions)
}

fn analyze_file_go(
    source: &str,
    file: &str,
) -> (Vec<PropertyTest>, usize, HashMap<String, FunctionCoverage>) {
    let property_tests = Vec::new();
    let mut unit_tests = 0usize;
    let mut functions = HashMap::new();

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("func Test") {
            unit_tests += 1;
            if let Some(name) = extract_go_fn_name(trimmed) {
                functions.entry(name.clone()).or_insert(FunctionCoverage {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    has_property_test: false,
                    has_unit_test: true,
                    property_tests: Vec::new(),
                });
            }
        }

        if trimmed.starts_with("func ") {
            if let Some(name) = extract_go_fn_name(trimmed) {
                functions.entry(name.clone()).or_insert(FunctionCoverage {
                    name,
                    file: file.to_string(),
                    line: line_num,
                    has_property_test: false,
                    has_unit_test: false,
                    property_tests: Vec::new(),
                });
            }
        }
    }

    (property_tests, unit_tests, functions)
}

fn extract_proptest_names(line: &str, line_num: usize, file: &str) -> Vec<PropertyTest> {
    let mut tests = Vec::new();
    for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.len() > 3 && !is_keyword(word) {
            tests.push(PropertyTest {
                name: format!("proptest_{}", word),
                file: file.to_string(),
                line: line_num,
                framework: "proptest".to_string(),
                functions_tested: vec![word.to_string()],
            });
        }
    }
    tests
}

fn extract_quickcheck_names(line: &str, line_num: usize, file: &str) -> Vec<PropertyTest> {
    let mut tests = Vec::new();
    for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.len() > 3 && !is_keyword(word) {
            tests.push(PropertyTest {
                name: format!("quickcheck_{}", word),
                file: file.to_string(),
                line: line_num,
                framework: "quickcheck".to_string(),
                functions_tested: vec![word.to_string()],
            });
        }
    }
    tests
}

fn extract_fn_name(line: &str) -> Option<String> {
    let after_fn = line.find("fn ")?;
    let rest = &line[after_fn + 3..];
    let name_end = rest.find(|c: char| c == '(' || c.is_whitespace())?;
    let name = rest[..name_end].trim();
    if name.is_empty() || is_keyword(name) {
        None
    } else {
        Some(name.to_string())
    }
}

fn is_keyword(word: &str) -> bool {
    let keywords: HashSet<&str> = [
        "if", "else", "while", "for", "loop", "match", "fn", "let", "mut", "pub", "use", "mod",
        "struct", "enum", "impl", "trait", "const", "static", "type", "where", "return", "break",
        "continue", "move", "ref", "self", "Self", "super", "crate", "async", "await", "dyn", "as",
        "in", "true", "false", "none", "some", "ok", "err", "result", "option", "vec", "string",
        "str", "u8", "u16", "u32", "u64", "i32", "i64", "f32", "f64", "bool", "char", "box", "rc",
        "arc", "cell", "refcell", "mutex", "rwlock", "thread", "spawn", "join", "main",
    ]
    .iter()
    .cloned()
    .collect();
    keywords.contains(word.to_lowercase().as_str())
}

fn extract_python_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let after_def = trimmed.strip_prefix("def ")?;
    let name = after_def
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_go_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let after_func = trimmed.strip_prefix("func ")?;
    let name = after_func
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn is_test_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    path_str.contains("/tests/")
        || path_str.contains("\\tests\\")
        || path_str.ends_with(&format!("_test.{}", ext))
        || path_str.ends_with(&format!("_tests.{}", ext))
}

fn output_table(report: &PropCovReport, min_coverage: u32) {
    println!("PROPERTY-BASED TESTING COVERAGE");
    println!("{}", separator(95));

    // Property tests section
    if !report.property_tests.is_empty() {
        println!();
        println!("PROPERTY TESTS FOUND:");
        let columns = [
            Column::left("NAME", 30),
            Column::left("FRAMEWORK", 12),
            Column::left("FILE", 30),
            Column::right("LINE", 5),
        ];
        print_table_header(&columns);
        for pt in report.property_tests.iter().take(20) {
            let line_str = pt.line.to_string();
            print_table_row(
                &columns,
                &[
                    &truncate(&pt.name, 28),
                    &pt.framework,
                    &truncate(&pt.file, 28),
                    &line_str,
                ],
            );
        }
        if report.property_tests.len() > 20 {
            println!("  ... and {} more", report.property_tests.len() - 20);
        }
    }

    // Functions needing coverage
    let uncovered: Vec<_> = report
        .function_coverage
        .iter()
        .filter(|f| !f.has_property_test && f.has_unit_test)
        .collect();

    if !uncovered.is_empty() {
        println!();
        println!("FUNCTIONS WITH UNIT TESTS BUT NO PROPERTY TESTS:");
        let columns = [
            Column::left("FUNCTION", 35),
            Column::left("FILE", 35),
            Column::right("LINE", 5),
        ];
        print_table_header(&columns);
        for f in uncovered.iter().take(15) {
            let line_str = f.line.to_string();
            print_table_row(
                &columns,
                &[&truncate(&f.name, 33), &truncate(&f.file, 33), &line_str],
            );
        }
        if uncovered.len() > 15 {
            println!("  ... and {} more", uncovered.len() - 15);
        }
    }

    println!("{}", separator(95));
    println!();
    println!("  SUMMARY");
    println!(
        "    Total functions:          {}",
        report.summary.total_functions
    );
    println!(
        "    With property tests:        {}",
        report.summary.with_property_tests
    );
    println!(
        "    With unit tests only:       {}",
        report.summary.with_unit_tests - report.summary.with_property_tests
    );
    println!(
        "    Property test count:        {}",
        report.summary.property_test_count
    );
    println!(
        "    Unit test count:            {}",
        report.summary.unit_test_count
    );
    println!();
    println!(
        "    Property coverage:          {:.1}%",
        report.summary.coverage_percentage
    );

    let status = if report.summary.coverage_percentage >= 50.0 {
        "Good property coverage"
    } else if report.summary.coverage_percentage >= 20.0 {
        "Moderate — consider adding proptest/quickcheck for edge cases"
    } else {
        "Low — significant gap in property-based testing"
    };
    println!("    Status:                     {}", status);

    if report.summary.coverage_percentage < min_coverage as f64 {
        println!();
        println!("  ⚠ Coverage below threshold of {}%", min_coverage);
    }
}

fn output_json(report: &PropCovReport) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_proptest() {
        let source = r#"
#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn test_add_commutative(a in 0..100i32, b in 0..100i32) {
            prop_assert_eq!(add(a, b), add(b, a));
        }
    }
}
"#;
        let (props, _units, _funcs) = analyze_file(source, "test.rs", Language::Rust);
        assert!(!props.is_empty(), "Should detect proptest");
        assert!(props.iter().any(|p| p.framework == "proptest"));
    }

    #[test]
    fn test_no_property_tests() {
        let source = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn test_simple() {
        assert_eq!(add(2, 2), 4);
    }
}
"#;
        let (props, units, _funcs) = analyze_file(source, "test.rs", Language::Rust);
        assert!(
            props.is_empty(),
            "Should not detect property tests in simple unit test file"
        );
        assert_eq!(units, 1, "Should count unit test");
    }

    #[test]
    fn test_quickcheck_detection() {
        let source = r#"
#[quickcheck]
fn prop_reverse_reverse(xs: Vec<u32>) -> bool {
    xs == reverse(reverse(xs))
}
"#;
        let (props, _units, _funcs) = analyze_file(source, "test.rs", Language::Rust);
        assert!(!props.is_empty(), "Should detect quickcheck attribute");
        assert!(props.iter().any(|p| p.framework == "quickcheck"));
    }
}
