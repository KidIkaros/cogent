#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "halstead",
    about = "Halstead metrics — Volume, Difficulty, Effort, and estimated bugs per file"
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

    /// Fail if estimated bugs per file exceeds this (default: 1.0)
    #[arg(long, default_value = "1.0")]
    max_bugs: f64,

    /// Show only files exceeding max_bugs
    #[arg(long)]
    violations_only: bool,
}

/// Halstead operator/operand token classification for a source file.
/// We use a language-agnostic tokenizer that classifies tokens into:
/// - operators: keywords, punctuation, math/logic/comparison symbols
/// - operands: identifiers, literals
#[derive(Debug, Clone, Serialize)]
struct HalsteadMetrics {
    file: String,
    /// η1: distinct operators
    distinct_operators: usize,
    /// η2: distinct operands
    distinct_operands: usize,
    /// N1: total operators
    total_operators: usize,
    /// N2: total operands
    total_operands: usize,
    /// Vocabulary: η = η1 + η2
    vocabulary: usize,
    /// Length: N = N1 + N2
    length: usize,
    /// Volume: V = N * log2(η)
    volume: f64,
    /// Difficulty: D = (η1/2) * (N2/η2)
    difficulty: f64,
    /// Effort: E = D * V
    effort: f64,
    /// Estimated bugs: B = V / 3000
    bugs_estimated: f64,
    /// Estimated time to implement (seconds): T = E / 18
    time_secs: f64,
}

#[derive(Serialize)]
struct HalsteadReport {
    files: Vec<HalsteadMetrics>,
    summary: HalsteadSummary,
}

#[derive(Serialize)]
struct HalsteadSummary {
    files_scanned: usize,
    files_exceeding_bugs_threshold: usize,
    max_bugs_threshold: f64,
    total_bugs_estimated: f64,
    avg_volume: f64,
    avg_difficulty: f64,
}

/// Language-agnostic Halstead tokenizer.
/// Classifies source tokens as operators or operands.
fn compute_halstead(source: &str) -> (usize, usize, usize, usize) {
    // Operator tokens: keywords + punctuation/symbols
    const OPERATORS: &[&str] = &[
        // common keywords across Rust/Python/JS/Go/C
        "if",
        "else",
        "for",
        "while",
        "loop",
        "match",
        "switch",
        "case",
        "return",
        "break",
        "continue",
        "fn",
        "func",
        "function",
        "def",
        "let",
        "var",
        "const",
        "mut",
        "pub",
        "use",
        "import",
        "from",
        "class",
        "struct",
        "enum",
        "impl",
        "trait",
        "type",
        "interface",
        "new",
        "delete",
        "await",
        "async",
        "yield",
        "try",
        "catch",
        "finally",
        "throw",
        "raise",
        "with",
        "in",
        "as",
        "is",
        "not",
        "and",
        "or",
        "true",
        "false",
        "nil",
        "null",
        "None",
        "True",
        "False",
        // single-char operators (we scan for these below)
    ];

    let operator_syms: HashSet<char> = [
        '+', '-', '*', '/', '%', '=', '<', '>', '!', '&', '|', '^', '~', '?', ':', ';', ',', '.',
        '(', ')', '[', ']', '{', '}', '@',
    ]
    .iter()
    .cloned()
    .collect();

    let mut op_set: HashSet<String> = HashSet::new();
    let mut opd_set: HashSet<String> = HashSet::new();
    let mut n1 = 0usize;
    let mut n2 = 0usize;

    // Tokenize: split on whitespace, strip comments (single-line), extract tokens
    for line in source.lines() {
        // strip single-line comments
        let line = if let Some(pos) = line.find("//") {
            &line[..pos]
        } else if let Some(pos) = line.find('#') {
            &line[..pos]
        } else {
            line
        };

        // Extract word tokens
        let mut word = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
            } else {
                if !word.is_empty() {
                    let tok = word.clone();
                    word.clear();
                    if OPERATORS.contains(&tok.as_str()) {
                        op_set.insert(tok.clone());
                        n1 += 1;
                    } else {
                        opd_set.insert(tok.clone());
                        n2 += 1;
                    }
                }
                // Symbol operators
                if operator_syms.contains(&c) {
                    // Combine multi-char ops like ==, !=, <=, >=, ->, =>
                    let mut sym = c.to_string();
                    if let Some(&next) = chars.peek() {
                        if "=><!+-".contains(next) && "=>".contains(next) {
                            sym.push(next);
                            chars.next();
                        }
                    }
                    op_set.insert(sym.clone());
                    n1 += 1;
                }
                // String literals — treat the whole literal as one operand
                if c == '"' || c == '\'' {
                    let mut lit = c.to_string();
                    for lc in chars.by_ref() {
                        if lc == c {
                            break;
                        }
                        lit.push(lc);
                    }
                    opd_set.insert("__literal__".into());
                    n2 += 1;
                }
            }
        }
        if !word.is_empty() {
            if OPERATORS.contains(&word.as_str()) {
                op_set.insert(word.clone());
                n1 += 1;
            } else {
                opd_set.insert(word.clone());
                n2 += 1;
            }
        }
    }

    (op_set.len(), opd_set.len(), n1, n2)
}

fn halstead_from_counts(
    file: &str,
    eta1: usize,
    eta2: usize,
    n1: usize,
    n2: usize,
) -> HalsteadMetrics {
    let vocabulary = eta1 + eta2;
    let length = n1 + n2;

    let volume = if vocabulary > 1 {
        length as f64 * (vocabulary as f64).log2()
    } else {
        0.0
    };

    let difficulty = if eta2 > 0 {
        (eta1 as f64 / 2.0) * (n2 as f64 / eta2 as f64)
    } else {
        0.0
    };

    let effort = difficulty * volume;
    let bugs_estimated = volume / 3000.0;
    let time_secs = effort / 18.0;

    HalsteadMetrics {
        file: file.to_string(),
        distinct_operators: eta1,
        distinct_operands: eta2,
        total_operators: n1,
        total_operands: n2,
        vocabulary,
        length,
        volume,
        difficulty,
        effort,
        bugs_estimated,
        time_secs,
    }
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "hpp", "cs",
        "java", "rb", "swift",
    ];

    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut metrics: Vec<HalsteadMetrics> = Vec::new();

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        let (eta1, eta2, n1, n2) = compute_halstead(&source);
        let m = halstead_from_counts(file, eta1, eta2, n1, n2);
        if !cli.violations_only || m.bugs_estimated > cli.max_bugs {
            metrics.push(m);
        }
    }

    metrics.sort_by(|a, b| {
        b.bugs_estimated
            .partial_cmp(&a.bugs_estimated)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let exceeding = metrics
        .iter()
        .filter(|m| m.bugs_estimated > cli.max_bugs)
        .count();
    let total_bugs: f64 = metrics.iter().map(|m| m.bugs_estimated).sum();
    let avg_vol = if metrics.is_empty() {
        0.0
    } else {
        metrics.iter().map(|m| m.volume).sum::<f64>() / metrics.len() as f64
    };
    let avg_diff = if metrics.is_empty() {
        0.0
    } else {
        metrics.iter().map(|m| m.difficulty).sum::<f64>() / metrics.len() as f64
    };

    let summary = HalsteadSummary {
        files_scanned: files.len(),
        files_exceeding_bugs_threshold: exceeding,
        max_bugs_threshold: cli.max_bugs,
        total_bugs_estimated: total_bugs,
        avg_volume: avg_vol,
        avg_difficulty: avg_diff,
    };

    match cli.format.as_str() {
        "json" => {
            let report = HalsteadReport {
                files: metrics,
                summary,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        "ndjson" => {
            for m in &metrics {
                println!("{}", serde_json::to_string(m).unwrap());
            }
        }
        _ => {
            let cols = vec![
                Column {
                    header: "File",
                    width: 40,
                    align_right: false,
                },
                Column {
                    header: "Vol",
                    width: 8,
                    align_right: true,
                },
                Column {
                    header: "Diff",
                    width: 7,
                    align_right: true,
                },
                Column {
                    header: "Bugs est.",
                    width: 10,
                    align_right: true,
                },
                Column {
                    header: "Time(h)",
                    width: 8,
                    align_right: true,
                },
            ];
            print_table_header(&cols);
            for m in &metrics {
                let flag = if m.bugs_estimated > cli.max_bugs {
                    "!"
                } else {
                    " "
                };
                print_table_row(
                    &cols,
                    &[
                        &truncate(&m.file, 40),
                        &format!("{:.0}", m.volume),
                        &format!("{:.1}", m.difficulty),
                        &format!("{}{:.2}", flag, m.bugs_estimated),
                        &format!("{:.1}", m.time_secs / 3600.0),
                    ],
                );
            }
            println!(
                "\nSummary: {} files  |  {} exceed bugs threshold ({:.2})  |  total estimated bugs: {:.2}",
                summary.files_scanned, summary.files_exceeding_bugs_threshold,
                summary.max_bugs_threshold, summary.total_bugs_estimated
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
    fn test_halstead_simple() {
        let src = "fn foo(x: i32) -> i32 { x + 1 }";
        let (eta1, eta2, n1, n2) = compute_halstead(src);
        assert!(eta1 > 0, "should have operators");
        assert!(eta2 > 0, "should have operands");
        assert!(n1 > 0);
        assert!(n2 > 0);
    }

    #[test]
    fn test_halstead_metrics_nonzero_volume() {
        let m = halstead_from_counts("f.rs", 10, 15, 30, 40);
        assert!(m.volume > 0.0);
        assert!(m.difficulty > 0.0);
        assert!(m.bugs_estimated > 0.0);
    }

    #[test]
    fn test_halstead_zero_operands_no_panic() {
        let m = halstead_from_counts("f.rs", 5, 0, 10, 0);
        assert_eq!(m.difficulty, 0.0);
    }

    #[test]
    fn test_halstead_empty_source() {
        let (eta1, eta2, n1, n2) = compute_halstead("");
        assert_eq!(eta1, 0);
        assert_eq!(eta2, 0);
        assert_eq!(n1, 0);
        assert_eq!(n2, 0);
    }
}
