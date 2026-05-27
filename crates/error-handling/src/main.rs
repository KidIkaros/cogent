#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "errhandle",
    about = "Error handling checker — detect unwrap(), expect(), silently-discarded Results"
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

    /// Max allowed unwrap/expect calls per codebase (default: 0)
    #[arg(long, default_value = "0")]
    max_unwraps: usize,

    /// Include test files in scan (default: false)
    #[arg(long)]
    include_tests: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ErrFinding {
    file: String,
    line: usize,
    kind: String,
    context: String,
    severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct ErrReport {
    findings: Vec<ErrFinding>,
    summary: ErrSummary,
}

#[derive(Serialize)]
struct ErrSummary {
    files_scanned: usize,
    total_findings: usize,
    unwrap_count: usize,
    expect_count: usize,
    discard_count: usize,
    panic_count: usize,
    max_unwraps_threshold: usize,
}

/// Patterns to detect and their severity/fix metadata.
const ERR_PATTERNS: &[(&str, &str, &str, &str)] = &[
    // (substring, kind, severity, fix_hint)
    (
        ".unwrap()",
        "unwrap",
        "medium",
        "Replace `.unwrap()` with `?`, `.unwrap_or_default()`, or proper error handling.",
    ),
    (
        ".expect(",
        "expect",
        "low",
        "Replace `.expect(msg)` with `?` or match the error explicitly.",
    ),
    (
        "panic!(",
        "panic",
        "high",
        "Replace `panic!()` with a `Result` return or logged error.",
    ),
    (
        "panic!(\"",
        "panic",
        "high",
        "Replace `panic!()` with a `Result` return or logged error.",
    ),
    (
        "unreachable!(",
        "unreachable",
        "low",
        "Ensure this branch is truly unreachable or handle it explicitly.",
    ),
    (
        "todo!()",
        "todo",
        "medium",
        "Replace `todo!()` with a real implementation before production.",
    ),
    (
        "unimplemented!()",
        "unimplemented",
        "medium",
        "Replace `unimplemented!()` with a real implementation.",
    ),
];

/// Detect `let _ = expr` patterns that silently discard Results/Options.
const DISCARD_PATTERN: &str = "let _ =";

fn scan_file(path: &str, include_tests: bool) -> Vec<ErrFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Only scan Rust files for now (language-specific patterns)
    if !matches!(ext, "rs" | "go" | "py" | "js" | "ts" | "tsx") {
        return vec![];
    }

    let mut findings = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_test_block = false;

    for (i, &line) in lines.iter().enumerate() {
        let lineno = i + 1;
        let trimmed = line.trim();

        // Track test blocks
        if trimmed.contains("#[cfg(test)]") || trimmed.contains("mod tests") {
            in_test_block = true;
        }
        if in_test_block && !include_tests {
            continue;
        }

        // Skip comment lines
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        // Language-specific patterns
        match ext {
            "rs" => {
                // Check each error pattern
                for &(pattern, kind, severity, fix) in ERR_PATTERNS {
                    if line.contains(pattern) && !trimmed.starts_with("//") {
                        // Deduplicate: don't add both unwrap and expect for same line
                        if findings
                            .iter()
                            .any(|f: &ErrFinding| f.line == lineno && f.file == path)
                        {
                            continue;
                        }
                        findings.push(ErrFinding {
                            file: path.to_string(),
                            line: lineno,
                            kind: kind.to_string(),
                            context: truncate(trimmed, 80).to_string(),
                            severity: severity.to_string(),
                            suggested_fix: Some(fix.to_string()),
                        });
                        break;
                    }
                }
                // let _ = discard
                if trimmed.starts_with(DISCARD_PATTERN) {
                    findings.push(ErrFinding {
                        file: path.to_string(),
                        line: lineno,
                        kind: "discard".to_string(),
                        context: truncate(trimmed, 80).to_string(),
                        severity: "low".to_string(),
                        suggested_fix: Some(
                            "Handle or log the discarded value instead of `let _ = ...`."
                                .to_string(),
                        ),
                    });
                }
            }
            "py" if trimmed == "except:"
                || trimmed == "except Exception:"
                || trimmed == "except Exception as e:" =>
            {
                let next = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                if next == "pass" || next.is_empty() {
                    findings.push(ErrFinding {
                        file: path.to_string(),
                        line: lineno,
                        kind: "bare_except".to_string(),
                        context: truncate(trimmed, 80).to_string(),
                        severity: "high".to_string(),
                        suggested_fix: Some(
                            "Catch specific exceptions and handle or log them properly."
                                .to_string(),
                        ),
                    });
                }
            }
            "js" | "ts" | "tsx" => {
                // JS/TS: `.catch(() => {})` or empty catch blocks
                if trimmed.contains(".catch(")
                    && (trimmed.contains("{}") || trimmed.ends_with(".catch()"))
                {
                    findings.push(ErrFinding {
                        file: path.to_string(),
                        line: lineno,
                        kind: "empty_catch".to_string(),
                        context: truncate(trimmed, 80).to_string(),
                        severity: "medium".to_string(),
                        suggested_fix: Some(
                            "Handle or log errors in `.catch()` instead of swallowing them."
                                .to_string(),
                        ),
                    });
                }
                // console.error that immediately ignores the error
                if trimmed == "} catch (e) {" {
                    let next = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                    if next == "}" {
                        findings.push(ErrFinding {
                            file: path.to_string(),
                            line: lineno,
                            kind: "empty_catch".to_string(),
                            context: truncate(trimmed, 80).to_string(),
                            severity: "high".to_string(),
                            suggested_fix: Some(
                                "Add error handling inside the empty catch block.".to_string(),
                            ),
                        });
                    }
                }
            }
            "go" => {
                // Go: `_ = someFunc()` that returns error
                if trimmed.starts_with("_ =") || trimmed.starts_with("_, _ =") {
                    findings.push(ErrFinding {
                        file: path.to_string(),
                        line: lineno,
                        kind: "discard".to_string(),
                        context: truncate(trimmed, 80).to_string(),
                        severity: "medium".to_string(),
                        suggested_fix: Some(
                            "Check and handle the discarded return value.".to_string(),
                        ),
                    });
                }
                // `if err != nil { }` with empty body
                if trimmed == "if err != nil {" || trimmed.starts_with("if err != nil {") {
                    let next = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                    if next == "}" {
                        findings.push(ErrFinding {
                            file: path.to_string(),
                            line: lineno,
                            kind: "empty_error_handler".to_string(),
                            context: truncate(trimmed, 80).to_string(),
                            severity: "high".to_string(),
                            suggested_fix: Some(
                                "Handle the error rather than leaving an empty `if err != nil {}`."
                                    .to_string(),
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    findings
}

fn run(cli: Cli) {
    let extensions = ["rs", "go", "py", "js", "ts", "tsx"];
    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut all_findings: Vec<ErrFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file, cli.include_tests));
    }

    all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let unwrap_c = all_findings.iter().filter(|f| f.kind == "unwrap").count();
    let expect_c = all_findings.iter().filter(|f| f.kind == "expect").count();
    let discard_c = all_findings.iter().filter(|f| f.kind == "discard").count();
    let panic_c = all_findings.iter().filter(|f| f.kind == "panic").count();

    let summary = ErrSummary {
        files_scanned: files.len(),
        total_findings: all_findings.len(),
        unwrap_count: unwrap_c,
        expect_count: expect_c,
        discard_count: discard_c,
        panic_count: panic_c,
        max_unwraps_threshold: cli.max_unwraps,
    };

    match cli.format.as_str() {
        "json" => {
            let report = ErrReport {
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
                println!("No error handling issues detected.");
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
                        width: 8,
                        align_right: false,
                    },
                    Column {
                        header: "Kind",
                        width: 14,
                        align_right: false,
                    },
                    Column {
                        header: "Context",
                        width: 55,
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
                            &truncate(&f.kind, 14),
                            &truncate(&f.context, 55),
                        ],
                    );
                }
            }
            let threshold_status = if summary.total_findings <= cli.max_unwraps {
                "PASS"
            } else {
                "FAIL"
            };
            println!(
                "\nSummary: {} findings ({} unwrap, {} expect, {} discard, {} panic) — {}",
                summary.total_findings, unwrap_c, expect_c, discard_c, panic_c, threshold_status
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
    fn test_detect_unwrap() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn main() {{ let x = some_fn().unwrap(); }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].kind, "unwrap");
    }

    #[test]
    fn test_detect_panic() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn foo() {{ panic!(\"oh no\"); }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(findings.iter().any(|f| f.kind == "panic"));
    }

    #[test]
    fn test_no_findings_clean() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn add(a: i32, b: i32) -> i32 {{ a + b }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_detect_let_discard() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "fn main() {{").unwrap();
        writeln!(f, "    let _ = some_result();").unwrap();
        writeln!(f, "}}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(findings.iter().any(|f| f.kind == "discard"));
    }
}
