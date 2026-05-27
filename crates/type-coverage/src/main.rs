#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "typecov",
    about = "Type coverage checker — detect missing type annotations in Python/JS/TS files"
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

    /// Minimum type coverage percentage (default: 80)
    #[arg(long, default_value = "80.0")]
    min_pct: f64,

    /// Show only files below the threshold
    #[arg(long)]
    violations_only: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FileTypeCov {
    file: String,
    language: String,
    total_functions: usize,
    typed_functions: usize,
    coverage_pct: f64,
    below_threshold: bool,
    /// List of untyped function names (first 10)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    untyped_functions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct TypeCovReport {
    files: Vec<FileTypeCov>,
    summary: TypeCovSummary,
}

#[derive(Serialize)]
struct TypeCovSummary {
    files_scanned: usize,
    files_below_threshold: usize,
    min_pct_threshold: f64,
    overall_coverage_pct: f64,
    total_functions: usize,
    typed_functions: usize,
}

/// Analyze a Python file for function/method type annotations.
/// Returns (total_functions, typed_functions, untyped_names)
fn analyze_python(source: &str) -> (usize, usize, Vec<String>) {
    let mut total = 0usize;
    let mut typed = 0usize;
    let mut untyped = Vec::new();

    for line in source.lines() {
        let t = line.trim();
        if !t.starts_with("def ") && !t.starts_with("async def ") {
            continue;
        }
        total += 1;

        // Extract function name
        let name = t
            .trim_start_matches("async ")
            .trim_start_matches("def ")
            .split('(')
            .next()
            .unwrap_or("?")
            .to_string();

        // A function is "typed" if it has a return annotation `->` and at least some param annotations `:`
        let has_return = t.contains("->") && !t.ends_with("-> None:");
        // Has parameter type annotation: `param: type`
        let has_param_annot = {
            if let Some(params_start) = t.find('(') {
                if let Some(params_end) = t.rfind(')') {
                    let params = &t[params_start + 1..params_end];
                    // Exclude `self` and `cls`
                    params.split(',').any(|p| {
                        let p = p.trim();
                        !p.is_empty() && p != "self" && p != "cls" && p.contains(':')
                    })
                } else {
                    false
                }
            } else {
                false
            }
        };

        if has_return || has_param_annot {
            typed += 1;
        } else {
            untyped.push(name);
        }
    }

    (total, typed, untyped)
}

/// Analyze a JS/TS file for function type annotations.
/// TS files get credit for type annotations on params/return; plain JS files
/// get credit only for JSDoc @param/@returns annotations.
fn analyze_js_ts(source: &str, is_ts: bool) -> (usize, usize, Vec<String>) {
    let mut total = 0usize;
    let mut typed = 0usize;
    let mut untyped = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim();

        // Detect function declarations
        let is_fn = t.starts_with("function ")
            || t.contains("=> {")
            || t.contains("=> (")
            || (t.starts_with("async ") && t.contains("function "))
            || (t.contains("): ")
                && (t.starts_with("public ")
                    || t.starts_with("private ")
                    || t.starts_with("protected ")));

        if !is_fn {
            continue;
        }
        // Skip arrow functions assigned to variables — too noisy; focus on named ones
        let is_named = t.starts_with("function ")
            || t.starts_with("async function ")
            || (t.contains("): ") && is_ts);
        if !is_named {
            continue;
        }

        total += 1;

        // Extract name
        let name = if t.starts_with("function ") || t.starts_with("async function ") {
            t.trim_start_matches("async ")
                .trim_start_matches("function ")
                .split('(')
                .next()
                .unwrap_or("?")
                .to_string()
        } else {
            t.split('(')
                .next()
                .unwrap_or("?")
                .split_whitespace()
                .last()
                .unwrap_or("?")
                .to_string()
        };

        if is_ts {
            // TypeScript: has param types if any param has `: Type` and return type `: Type {`
            let has_param_types = if let Some(s) = t.find('(') {
                if let Some(e) = t.find(')') {
                    let params = &t[s + 1..e];
                    params.split(',').any(|p| {
                        let p = p.trim();
                        !p.is_empty() && p.contains(':') && p != "..."
                    })
                } else {
                    false
                }
            } else {
                false
            };
            let has_return_type =
                t.contains("): ") && !t.ends_with("): void {") || t.contains("): void");
            if has_param_types || has_return_type {
                typed += 1;
            } else {
                // Check for `any` type usage — counts as untyped
                untyped.push(name);
            }
        } else {
            // Plain JS: check for JSDoc above the function (up to 5 lines back)
            let has_jsdoc = (0..5.min(i)).rev().any(|j| {
                let prev = lines[i - 1 - j].trim();
                prev.contains("@param") || prev.contains("@returns") || prev.contains("@type")
            });
            if has_jsdoc {
                typed += 1;
            } else {
                untyped.push(name);
            }
        }
    }

    (total, typed, untyped)
}

fn analyze_file(path: &str, min_pct: f64) -> Option<FileTypeCov> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return None;
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let (lang, total, typed, mut untyped) = match ext {
        "py" | "pyi" => {
            let (t, ty, u) = analyze_python(&source);
            ("Python", t, ty, u)
        }
        "ts" | "tsx" | "mts" => {
            let (t, ty, u) = analyze_js_ts(&source, true);
            ("TypeScript", t, ty, u)
        }
        "js" | "mjs" | "cjs" => {
            let (t, ty, u) = analyze_js_ts(&source, false);
            ("JavaScript", t, ty, u)
        }
        _ => return None, // Rust is 100% typed by definition
    };

    if total == 0 {
        return None;
    }

    let pct = typed as f64 / total as f64 * 100.0;
    let below = pct < min_pct;
    untyped.truncate(10); // cap the list

    Some(FileTypeCov {
        file: path.to_string(),
        language: lang.to_string(),
        total_functions: total,
        typed_functions: typed,
        coverage_pct: pct,
        below_threshold: below,
        untyped_functions: untyped,
        suggested_fix: if below {
            Some(format!(
                "Add type annotations to reach {:.0}% coverage (currently {:.1}%).",
                min_pct, pct
            ))
        } else {
            None
        },
    })
}

fn run(cli: Cli) {
    let extensions = ["py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts"];
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut stats: Vec<FileTypeCov> = Vec::new();
    for file in &files {
        if let Some(s) = analyze_file(file, cli.min_pct) {
            if !cli.violations_only || s.below_threshold {
                stats.push(s);
            }
        }
    }

    stats.sort_by(|a, b| {
        a.coverage_pct
            .partial_cmp(&b.coverage_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let below = stats.iter().filter(|s| s.below_threshold).count();
    let total_fns: usize = stats.iter().map(|s| s.total_functions).sum();
    let typed_fns: usize = stats.iter().map(|s| s.typed_functions).sum();
    let overall = if total_fns == 0 {
        100.0
    } else {
        typed_fns as f64 / total_fns as f64 * 100.0
    };

    let summary = TypeCovSummary {
        files_scanned: files.len(),
        files_below_threshold: below,
        min_pct_threshold: cli.min_pct,
        overall_coverage_pct: overall,
        total_functions: total_fns,
        typed_functions: typed_fns,
    };

    match cli.format.as_str() {
        "json" => {
            let report = TypeCovReport {
                files: stats,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for s in &stats {
                println!("{}", serde_json::to_string(s).unwrap());
            }
        }
        _ => {
            if stats.is_empty() {
                println!("No Python/JS/TS files found to analyze.");
            } else {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 40,
                        align_right: false,
                    },
                    Column {
                        header: "Lang",
                        width: 11,
                        align_right: false,
                    },
                    Column {
                        header: "Fns",
                        width: 5,
                        align_right: true,
                    },
                    Column {
                        header: "Typed",
                        width: 7,
                        align_right: true,
                    },
                    Column {
                        header: "Coverage",
                        width: 9,
                        align_right: true,
                    },
                    Column {
                        header: "Status",
                        width: 7,
                        align_right: false,
                    },
                ];
                print_table_header(&cols);
                for s in &stats {
                    let status = if s.below_threshold { "LOW" } else { "ok" };
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&s.file, 40),
                            &s.language,
                            &s.total_functions.to_string(),
                            &s.typed_functions.to_string(),
                            &format!("{:.1}%", s.coverage_pct),
                            status,
                        ],
                    );
                }
            }
            println!(
                "\nSummary: {} files  |  overall {:.1}% typed  |  {} files below {:.0}% threshold",
                summary.files_scanned,
                summary.overall_coverage_pct,
                summary.files_below_threshold,
                summary.min_pct_threshold
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
    fn test_python_typed_function() {
        let src = "def foo(x: int, y: str) -> bool:\n    return True\n";
        let (total, typed, untyped) = analyze_python(src);
        assert_eq!(total, 1);
        assert_eq!(typed, 1);
        assert!(untyped.is_empty());
    }

    #[test]
    fn test_python_untyped_function() {
        let src = "def bar(x, y):\n    return x + y\n";
        let (total, typed, untyped) = analyze_python(src);
        assert_eq!(total, 1);
        assert_eq!(typed, 0);
        assert_eq!(untyped, vec!["bar"]);
    }

    #[test]
    fn test_python_partial_annotation() {
        // Has return annotation but no param types
        let src = "def baz(x) -> int:\n    return x\n";
        let (total, typed, _) = analyze_python(src);
        assert_eq!(total, 1);
        assert_eq!(typed, 1); // return annotation counts
    }

    #[test]
    fn test_ts_typed() {
        let src = "function add(a: number, b: number): number { return a + b; }\n";
        let (total, typed, untyped) = analyze_js_ts(src, true);
        assert_eq!(total, 1);
        assert_eq!(typed, 1);
        assert!(untyped.is_empty());
    }
}
