#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "comments",
    about = "Comment ratio analyzer — measure inline comment density per file (distinct from doc coverage)"
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

    /// Minimum required inline comment ratio (default: 0.10 = 10%)
    #[arg(long, default_value = "0.10")]
    min_ratio: f64,

    /// Show only files below the threshold
    #[arg(long)]
    violations_only: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FileCommentStats {
    file: String,
    code_lines: usize,
    comment_lines: usize,
    blank_lines: usize,
    total_lines: usize,
    /// Ratio of comment lines to (comment + code) lines
    comment_ratio: f64,
    below_threshold: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct CommentReport {
    files: Vec<FileCommentStats>,
    summary: CommentSummary,
}

#[derive(Serialize)]
struct CommentSummary {
    files_scanned: usize,
    files_below_threshold: usize,
    min_ratio_threshold: f64,
    overall_comment_ratio: f64,
    total_code_lines: usize,
    total_comment_lines: usize,
}

/// Detect if a line is a comment for a given file extension.
fn is_comment_line(line: &str, ext: &str) -> bool {
    let t = line.trim();
    match ext {
        "rs" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "cs" | "java" | "js" | "ts" | "tsx"
        | "mjs" | "go" | "swift" => {
            t.starts_with("//") || t.starts_with("/*") || t.starts_with("*") || t.starts_with("*/")
        }
        "py" | "pyi" | "rb" => t.starts_with('#'),
        "php" => {
            t.starts_with("//") || t.starts_with('#') || t.starts_with("/*") || t.starts_with("*")
        }
        _ => t.starts_with("//") || t.starts_with('#'),
    }
}

/// Doc comments (///, /**, #!) are excluded — those are measured by doccov.
fn is_doc_comment(line: &str, ext: &str) -> bool {
    let t = line.trim();
    match ext {
        "rs" => t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**"),
        "js" | "ts" | "tsx" | "java" | "cs" => {
            t.starts_with("/**") || t.starts_with("* ") || t.starts_with("*/")
        }
        "py" | "pyi" => t.starts_with("\"\"\"") || t.starts_with("'''"),
        _ => false,
    }
}

fn analyze_file(path: &str, min_ratio: f64) -> Option<FileCommentStats> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return None;
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let mut code_lines = 0usize;
    let mut comment_lines = 0usize;
    let mut blank_lines = 0usize;

    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() {
            blank_lines += 1;
        } else if is_comment_line(line, ext) && !is_doc_comment(line, ext) {
            comment_lines += 1;
        } else {
            code_lines += 1;
        }
    }

    let total = code_lines + comment_lines;
    let ratio = if total == 0 {
        1.0
    } else {
        comment_lines as f64 / total as f64
    };
    let below = ratio < min_ratio && total > 10; // Skip tiny files

    Some(FileCommentStats {
        file: path.to_string(),
        code_lines,
        comment_lines,
        blank_lines,
        total_lines: code_lines + comment_lines + blank_lines,
        comment_ratio: ratio,
        below_threshold: below,
        suggested_fix: if below {
            Some(format!(
                "Comment ratio is {:.1}% (need {:.0}%). Add inline comments explaining non-obvious logic.",
                ratio * 100.0, min_ratio * 100.0
            ))
        } else {
            None
        },
    })
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "hpp", "java",
        "rb", "swift", "php", "cs",
    ];

    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut stats: Vec<FileCommentStats> = Vec::new();
    for file in &files {
        if let Some(s) = analyze_file(file, cli.min_ratio) {
            if !cli.violations_only || s.below_threshold {
                stats.push(s);
            }
        }
    }

    stats.sort_by(|a, b| {
        a.comment_ratio
            .partial_cmp(&b.comment_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let below = stats.iter().filter(|s| s.below_threshold).count();
    let total_code: usize = stats.iter().map(|s| s.code_lines).sum();
    let total_comments: usize = stats.iter().map(|s| s.comment_lines).sum();
    let overall_ratio = if total_code + total_comments == 0 {
        0.0
    } else {
        total_comments as f64 / (total_code + total_comments) as f64
    };

    let summary = CommentSummary {
        files_scanned: files.len(),
        files_below_threshold: below,
        min_ratio_threshold: cli.min_ratio,
        overall_comment_ratio: overall_ratio,
        total_code_lines: total_code,
        total_comment_lines: total_comments,
    };

    match cli.format.as_str() {
        "json" => {
            let report = CommentReport {
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
            let cols = vec![
                Column {
                    header: "File",
                    width: 45,
                    align_right: false,
                },
                Column {
                    header: "Code",
                    width: 6,
                    align_right: true,
                },
                Column {
                    header: "Comments",
                    width: 9,
                    align_right: true,
                },
                Column {
                    header: "Ratio",
                    width: 7,
                    align_right: true,
                },
                Column {
                    header: "Status",
                    width: 8,
                    align_right: false,
                },
            ];
            print_table_header(&cols);
            for s in &stats {
                let status = if s.below_threshold { "LOW" } else { "ok" };
                print_table_row(
                    &cols,
                    &[
                        &truncate(&s.file, 45),
                        &s.code_lines.to_string(),
                        &s.comment_lines.to_string(),
                        &format!("{:.1}%", s.comment_ratio * 100.0),
                        status,
                    ],
                );
            }
            println!(
                "\nSummary: {} files  |  overall ratio {:.1}%  |  {} files below {:.0}% threshold",
                summary.files_scanned,
                summary.overall_comment_ratio * 100.0,
                summary.files_below_threshold,
                summary.min_ratio_threshold * 100.0
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
    fn test_is_comment_line_rust() {
        assert!(is_comment_line("  // a comment", "rs"));
        assert!(!is_comment_line("  let x = 1;", "rs"));
        assert!(!is_comment_line("", "rs"));
    }

    #[test]
    fn test_is_comment_line_python() {
        assert!(is_comment_line("# a comment", "py"));
        assert!(!is_comment_line("def foo():", "py"));
    }

    #[test]
    fn test_doc_comment_excluded_rust() {
        assert!(is_doc_comment("  /// doc comment", "rs"));
        assert!(!is_doc_comment("  // regular comment", "rs"));
    }

    #[test]
    fn test_ratio_calculation() {
        // 5 comment lines, 15 code lines → ratio = 5/20 = 0.25
        let code = 15;
        let comm = 5;
        let ratio = comm as f64 / (code + comm) as f64;
        assert!((ratio - 0.25).abs() < 1e-9);
    }
}
