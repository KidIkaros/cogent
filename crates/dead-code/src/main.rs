#![deny(clippy::all)]

use clap::Parser;
use cogent_common::{find_source_files, print_table_header, print_table_row, truncate, Column};
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(
    name = "deadcode",
    about = "Dead code detector — find unused pub symbols, dead_code suppressions, and unreachable patterns"
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

    /// Also flag unreachable patterns (if false / if true constant conditions)
    #[arg(long)]
    check_unreachable: bool,
}

#[derive(Debug, Clone, Serialize)]
enum DeadCodeKind {
    AllowDeadCode,
    UnusedImport,
    UnreachableBranch,
    DeadAssignment,
    EmptyBlock,
}

impl std::fmt::Display for DeadCodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeadCodeKind::AllowDeadCode => write!(f, "allow(dead_code)"),
            DeadCodeKind::UnusedImport => write!(f, "unused_import"),
            DeadCodeKind::UnreachableBranch => write!(f, "unreachable_branch"),
            DeadCodeKind::DeadAssignment => write!(f, "dead_assignment"),
            DeadCodeKind::EmptyBlock => write!(f, "empty_block"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DeadCodeFinding {
    file: String,
    line: usize,
    kind: String,
    context: String,
    severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
}

#[derive(Serialize)]
struct DeadCodeReport {
    findings: Vec<DeadCodeFinding>,
    summary: DeadCodeSummary,
}

#[derive(Serialize)]
struct DeadCodeSummary {
    files_scanned: usize,
    total_findings: usize,
    allow_dead_code_count: usize,
    unused_import_count: usize,
    unreachable_count: usize,
    dead_assignment_count: usize,
    empty_block_count: usize,
}

fn scan_file(path: &str, check_unreachable: bool) -> Vec<DeadCodeFinding> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let mut findings = Vec::new();
    let lang_ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let lines: Vec<&str> = source.lines().collect();

    for (i, &line) in lines.iter().enumerate() {
        let lineno = i + 1;
        let trimmed = line.trim();

        // 1. #[allow(dead_code)] — suppression smell
        if trimmed.contains("#[allow(dead_code)]") || trimmed.contains("# noqa: F401") {
            findings.push(DeadCodeFinding {
                file: path.to_string(),
                line: lineno,
                kind: DeadCodeKind::AllowDeadCode.to_string(),
                context: truncate(trimmed, 80).to_string(),
                severity: "medium".to_string(),
                suggested_fix: Some(
                    "Remove the dead code instead of suppressing the warning.".to_string(),
                ),
            });
        }

        // 2. Unused imports (language-specific)
        match lang_ext {
            "py" if trimmed.starts_with("import ") || trimmed.starts_with("from ") => {
                if let Some(name) = extract_python_import_name(trimmed) {
                    let use_count = source.matches(&name).count();
                    if use_count <= 1 {
                        findings.push(DeadCodeFinding {
                            file: path.to_string(),
                            line: lineno,
                            kind: DeadCodeKind::UnusedImport.to_string(),
                            context: truncate(trimmed, 80).to_string(),
                            severity: "low".to_string(),
                            suggested_fix: Some(format!("Remove unused import `{}`.", name)),
                        });
                    }
                }
            }
            "js" | "ts" | "mjs" | "tsx"
                if trimmed.starts_with("import ") && trimmed.contains("from ") =>
            {
                for name in extract_js_import_names(trimmed) {
                    let use_count = source.matches(&name).count();
                    if use_count <= 1 {
                        findings.push(DeadCodeFinding {
                            file: path.to_string(),
                            line: lineno,
                            kind: DeadCodeKind::UnusedImport.to_string(),
                            context: truncate(trimmed, 80).to_string(),
                            severity: "low".to_string(),
                            suggested_fix: Some(format!("Remove unused import `{}`.", name)),
                        });
                    }
                }
            }
            _ => {}
        }

        // 3. Unreachable branches (if check_unreachable enabled)
        if check_unreachable {
            if trimmed == "if false {" || trimmed == "if (false) {" {
                findings.push(DeadCodeFinding {
                    file: path.to_string(),
                    line: lineno,
                    kind: DeadCodeKind::UnreachableBranch.to_string(),
                    context: truncate(trimmed, 80).to_string(),
                    severity: "high".to_string(),
                    suggested_fix: Some("Remove this unreachable `if false` branch.".to_string()),
                });
            }
            if trimmed == "if true {" || trimmed == "if (true) {" {
                findings.push(DeadCodeFinding {
                    file: path.to_string(),
                    line: lineno,
                    kind: DeadCodeKind::UnreachableBranch.to_string(),
                    context: truncate(trimmed, 80).to_string(),
                    severity: "medium".to_string(),
                    suggested_fix: Some(
                        "Remove the `if true` wrapper — the body always executes.".to_string(),
                    ),
                });
            }
        }

        // 4. Dead assignments: `let _ = expr;` — discarding a value silently
        if lang_ext == "rs" && (trimmed.starts_with("let _ =") || trimmed.starts_with("let _x =")) {
            findings.push(DeadCodeFinding {
                file: path.to_string(),
                line: lineno,
                kind: DeadCodeKind::DeadAssignment.to_string(),
                context: truncate(trimmed, 80).to_string(),
                severity: "low".to_string(),
                suggested_fix: Some(
                    "Consider handling the value instead of discarding it with `let _ = ...`."
                        .to_string(),
                ),
            });
        }

        // 5. Empty blocks: `{}` on its own line or `{ }` — may indicate incomplete implementation
        if (trimmed == "{}" || trimmed == "{ }") && i > 0 {
            // Only flag if the previous non-empty line opens a function/method/if
            let prev = lines[..i]
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim())
                .unwrap_or("");
            let is_fn_or_branch = prev.contains("fn ")
                || prev.contains("def ")
                || prev.contains("function ")
                || prev.ends_with("else")
                || prev.ends_with("else {")
                || prev.ends_with("} else {");
            if is_fn_or_branch {
                findings.push(DeadCodeFinding {
                    file: path.to_string(),
                    line: lineno,
                    kind: DeadCodeKind::EmptyBlock.to_string(),
                    context: format!("{} → {}", truncate(prev, 40), trimmed),
                    severity: "low".to_string(),
                    suggested_fix: Some(
                        "Add implementation or a `todo!()` / `pass` to document intent."
                            .to_string(),
                    ),
                });
            }
        }
    }

    findings
}

fn extract_python_import_name(line: &str) -> Option<String> {
    // `import foo` → "foo", `from foo import bar` → "bar"
    if let Some(rest) = line.strip_prefix("from ") {
        if let Some(pos) = rest.find(" import ") {
            let names = &rest[pos + 8..];
            let name = names
                .split(',')
                .next()?
                .split_whitespace()
                .next()?
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    } else if let Some(rest) = line.strip_prefix("import ") {
        let name = rest
            .split(',')
            .next()?
            .split_whitespace()
            .next()?
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn extract_js_import_names(line: &str) -> Vec<String> {
    // `import { foo, bar } from '...'` → ["foo", "bar"]
    // `import DefaultExport from '...'` → ["DefaultExport"]
    let mut names = Vec::new();
    if let Some(start) = line.find('{') {
        if let Some(end) = line.find('}') {
            let inner = &line[start + 1..end];
            for part in inner.split(',') {
                let name = part.trim().split(" as ").next().unwrap_or("").trim();
                if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_alphabetic()) {
                    names.push(name.to_string());
                }
            }
        }
    } else {
        // Default import: `import Foo from '...'`
        let after_import = line.trim_start_matches("import").trim();
        let name = after_import
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('*');
        if !name.is_empty() && name != "type" {
            names.push(name.to_string());
        }
    }
    names
}

fn run(cli: Cli) {
    let extensions = [
        "rs", "py", "js", "mjs", "ts", "tsx", "go", "c", "cpp", "java", "rb", "swift",
    ];

    let files = if Path::new(&cli.path).is_file() {
        vec![cli.path.clone()]
    } else {
        find_source_files(&cli.path, cli.recursive, &extensions)
    };

    let mut all_findings: Vec<DeadCodeFinding> = Vec::new();
    for file in &files {
        all_findings.extend(scan_file(file, cli.check_unreachable));
    }

    all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let allow_dc = all_findings
        .iter()
        .filter(|f| f.kind == "allow(dead_code)")
        .count();
    let unused_imp = all_findings
        .iter()
        .filter(|f| f.kind == "unused_import")
        .count();
    let unreachable = all_findings
        .iter()
        .filter(|f| f.kind == "unreachable_branch")
        .count();
    let dead_assign = all_findings
        .iter()
        .filter(|f| f.kind == "dead_assignment")
        .count();
    let empty_blk = all_findings
        .iter()
        .filter(|f| f.kind == "empty_block")
        .count();

    let summary = DeadCodeSummary {
        files_scanned: files.len(),
        total_findings: all_findings.len(),
        allow_dead_code_count: allow_dc,
        unused_import_count: unused_imp,
        unreachable_count: unreachable,
        dead_assignment_count: dead_assign,
        empty_block_count: empty_blk,
    };

    match cli.format.as_str() {
        "json" => {
            let report = DeadCodeReport {
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
                println!("No dead code patterns detected.");
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
                        width: 20,
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
                            &truncate(&f.kind, 20),
                            &truncate(&f.context, 50),
                        ],
                    );
                }
            }
            println!(
                "\nSummary: {} findings in {} files  [{} suppressed, {} unused imports, {} unreachable, {} dead assigns, {} empty blocks]",
                summary.total_findings, summary.files_scanned,
                allow_dc, unused_imp, unreachable, dead_assign, empty_blk
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
    fn test_extract_python_import() {
        assert_eq!(extract_python_import_name("import os"), Some("os".into()));
        assert_eq!(
            extract_python_import_name("from os import path"),
            Some("path".into())
        );
        assert_eq!(
            extract_python_import_name("from os.path import join"),
            Some("join".into())
        );
    }

    #[test]
    fn test_extract_js_imports() {
        let names = extract_js_import_names("import { foo, bar } from './mod';");
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"bar".to_string()));
    }

    #[test]
    fn test_allow_dead_code_detected() {
        // Write a temp file and scan it
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "#[allow(dead_code)]\npub fn unused() {{}}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].kind, "allow(dead_code)");
    }

    #[test]
    fn test_no_findings_on_clean_file() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
        writeln!(f, "pub fn add(a: i32, b: i32) -> i32 {{ a + b }}").unwrap();
        let findings = scan_file(f.path().to_str().unwrap(), false);
        assert!(findings.is_empty());
    }
}
