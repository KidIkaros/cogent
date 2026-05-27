#![deny(clippy::all)]

use ast_parse_ts::{parse_complexity_file, Language};
use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "linelen",
    about = "Line length checker — flag functions and files exceeding size thresholds"
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

    /// Max allowed function body length in lines (default: 40)
    #[arg(long, default_value = "40")]
    max_fn_lines: usize,

    /// Max allowed file length in lines (default: 500)
    #[arg(long, default_value = "500")]
    max_file_lines: usize,

    /// Only show violations (skip passing items)
    #[arg(long)]
    violations_only: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FnViolation {
    file: String,
    function: String,
    start_line: usize,
    end_line: usize,
    lines: usize,
    threshold: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FileViolation {
    file: String,
    lines: usize,
    threshold: usize,
}

#[derive(Serialize)]
struct LineLenReport {
    fn_violations: Vec<FnViolation>,
    file_violations: Vec<FileViolation>,
    summary: LineLenSummary,
}

#[derive(Serialize)]
struct LineLenSummary {
    files_scanned: usize,
    fn_violations: usize,
    file_violations: usize,
    max_fn_lines_threshold: usize,
    max_file_lines_threshold: usize,
}

fn count_file_lines(path: &str) -> usize {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp", "cc",
        "cxx", "hpp", "cs", "java", "php", "rb", "swift",
    ];

    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut fn_violations: Vec<FnViolation> = Vec::new();
    let mut file_violations: Vec<FileViolation> = Vec::new();

    for file in &files {
        let file_lines = count_file_lines(file);
        if file_lines > cli.max_file_lines {
            file_violations.push(FileViolation {
                file: file.clone(),
                lines: file_lines,
                threshold: cli.max_file_lines,
            });
        }

        let lang = Language::from_extension(file);
        if matches!(lang, Language::Unknown) {
            continue;
        }

        let fns = parse_complexity_file(file);
        for f in fns {
            let body_lines = f.end_line.saturating_sub(f.line) + 1;
            if body_lines > cli.max_fn_lines {
                fn_violations.push(FnViolation {
                    file: file.clone(),
                    function: f.name.clone(),
                    start_line: f.line,
                    end_line: f.end_line,
                    lines: body_lines,
                    threshold: cli.max_fn_lines,
                    suggested_fix: Some(format!(
                        "Extract sub-functions from `{}` to reduce its {} lines to <= {}.",
                        f.name, body_lines, cli.max_fn_lines
                    )),
                });
            }
        }
    }

    fn_violations.sort_by_key(|b| std::cmp::Reverse(b.lines));
    file_violations.sort_by_key(|b| std::cmp::Reverse(b.lines));

    let summary = LineLenSummary {
        files_scanned: files.len(),
        fn_violations: fn_violations.len(),
        file_violations: file_violations.len(),
        max_fn_lines_threshold: cli.max_fn_lines,
        max_file_lines_threshold: cli.max_file_lines,
    };

    match cli.format.as_str() {
        "json" => {
            let report = LineLenReport {
                fn_violations,
                file_violations,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for v in &fn_violations {
                println!("{}", serde_json::to_string(v).unwrap());
            }
            for v in &file_violations {
                println!("{}", serde_json::to_string(v).unwrap());
            }
        }
        _ => {
            if !fn_violations.is_empty() {
                let cols = vec![
                    Column {
                        header: "File",
                        width: 40,
                        align_right: false,
                    },
                    Column {
                        header: "Function",
                        width: 25,
                        align_right: false,
                    },
                    Column {
                        header: "Lines",
                        width: 7,
                        align_right: true,
                    },
                    Column {
                        header: "Limit",
                        width: 7,
                        align_right: true,
                    },
                ];
                print_table_header(&cols);
                for v in &fn_violations {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&v.file, 40),
                            &truncate(&v.function, 25),
                            &v.lines.to_string(),
                            &v.threshold.to_string(),
                        ],
                    );
                }
            }
            if !file_violations.is_empty() {
                println!();
                let cols = vec![
                    Column {
                        header: "File (too long)",
                        width: 55,
                        align_right: false,
                    },
                    Column {
                        header: "Lines",
                        width: 7,
                        align_right: true,
                    },
                    Column {
                        header: "Limit",
                        width: 7,
                        align_right: true,
                    },
                ];
                print_table_header(&cols);
                for v in &file_violations {
                    print_table_row(
                        &cols,
                        &[
                            &truncate(&v.file, 55),
                            &v.lines.to_string(),
                            &v.threshold.to_string(),
                        ],
                    );
                }
            }
            if fn_violations.is_empty() && file_violations.is_empty() {
                println!("All functions and files within size thresholds.");
            }
            println!(
                "\nSummary: {} fn violations, {} file violations (scanned {} files)",
                summary.fn_violations, summary.file_violations, summary.files_scanned
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
    fn test_count_file_lines_empty() {
        // Inline test: empty string = 0 lines
        let count = "".lines().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_file_lines_multiline() {
        let s = "a\nb\nc\n";
        assert_eq!(s.lines().count(), 3);
    }

    #[test]
    fn test_fn_violation_sorting() {
        let mut v = vec![
            FnViolation {
                file: "a".into(),
                function: "a".into(),
                start_line: 1,
                end_line: 50,
                lines: 50,
                threshold: 40,
                suggested_fix: None,
            },
            FnViolation {
                file: "b".into(),
                function: "b".into(),
                start_line: 1,
                end_line: 100,
                lines: 100,
                threshold: 40,
                suggested_fix: None,
            },
        ];
        v.sort_by(|a, b| b.lines.cmp(&a.lines));
        assert_eq!(v[0].lines, 100);
    }
}
