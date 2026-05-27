#![deny(clippy::all)]

use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;

use ast_parse_ts::{fingerprint_similarity, parse_fingerprints_file};
use cogent_common::{find_source_files, truncate};

#[derive(Parser)]
#[command(
    name = "dupfind",
    about = "Code duplication detection -- find copy-pasted blocks via structural similarity"
)]
struct Cli {
    /// Path to scan (file or directory)
    path: String,

    /// Recursive scan
    #[arg(short, long)]
    recursive: bool,

    /// Minimum block size (lines) to consider
    #[arg(short, long, default_value = "5")]
    min_lines: usize,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateGroup {
    fingerprint: String,
    instances: Vec<DuplicateInstance>,
    similarity: f64,
    /// Suggested fix for the duplication
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct DuplicateInstance {
    file: String,
    function: String,
    line: usize,
    /// Code context (surrounding lines) for the instance
    #[serde(skip_serializing_if = "Option::is_none")]
    code_context: Option<String>,
}

#[derive(Serialize)]
struct DupReport {
    groups: Vec<DuplicateGroup>,
    summary: DupSummary,
}

#[derive(Serialize)]
struct DupSummary {
    total_groups: usize,
    total_instances: usize,
    files_affected: usize,
}

/// A normalized function skeleton for comparison
#[derive(Debug, Clone)]
struct FunctionSkeleton {
    name: String,
    file: String,
    line: usize,
    /// Normalized statement pattern (structure without identifiers)
    pattern: String,
    /// Statement count
    stmt_count: usize,
}

const SUPPORTED_EXTS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "cxx", "hpp", "cs",
    "java", "php", "rb", "swift",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let all_files = find_source_files(&cli.path, cli.recursive, SUPPORTED_EXTS);
    if all_files.is_empty() {
        eprintln!("No supported source files found at {}", cli.path);
        std::process::exit(1);
    }

    // Single-threaded rayon to reduce memory pressure (prevents OOM on 16GB/32GB systems)
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let skeletons: Vec<FunctionSkeleton> = pool.install(|| {
        all_files
            .par_iter()
            .flat_map(|file_path| {
                parse_fingerprints_file(file_path)
                    .into_iter()
                    .map(|fp| {
                        let stmt_count = fp.fingerprint.split(';').count();
                        FunctionSkeleton {
                            name: format!("{}:{}", fp.file, fp.line),
                            file: fp.file,
                            line: fp.line,
                            pattern: fp.fingerprint,
                            stmt_count,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    });

    // Group by pattern similarity
    let groups = find_duplicates(&skeletons, cli.min_lines);

    match cli.format.as_str() {
        "json" => output_json(&groups),
        "ndjson" => output_ndjson(&groups),
        _ => {
            output_table(&groups);
            Ok(())
        }
    }
}

/// Find duplicate groups by comparing skeletons
fn find_duplicates(skeletons: &[FunctionSkeleton], min_lines: usize) -> Vec<DuplicateGroup> {
    let mut groups = Vec::new();
    let mut used = vec![false; skeletons.len()];

    for i in 0..skeletons.len() {
        if let Some(group) = try_build_group(skeletons, &mut used, i, min_lines) {
            groups.push(group);
        }
    }

    groups
}

/// Try to build a duplicate group anchored at index `i`.
fn try_build_group(
    skeletons: &[FunctionSkeleton],
    used: &mut [bool],
    i: usize,
    min_lines: usize,
) -> Option<DuplicateGroup> {
    if used[i] || skeletons[i].stmt_count < min_lines {
        return None;
    }

    let mut group_instances = vec![DuplicateInstance {
        file: skeletons[i].file.clone(),
        function: skeletons[i].name.clone(),
        line: skeletons[i].line,
        code_context: None,
    }];

    for j in (i + 1)..skeletons.len() {
        if used[j] || skeletons[j].stmt_count < min_lines {
            continue;
        }

        let similarity = pattern_similarity(&skeletons[i].pattern, &skeletons[j].pattern);
        if similarity >= 0.7 {
            group_instances.push(DuplicateInstance {
                file: skeletons[j].file.clone(),
                function: skeletons[j].name.clone(),
                line: skeletons[j].line,
                code_context: None,
            });
            used[j] = true;
        }
    }

    if group_instances.len() > 1 {
        used[i] = true;
        Some(DuplicateGroup {
            fingerprint: truncate(&skeletons[i].pattern, 60),
            instances: group_instances,
            similarity: 1.0, // All in group are similar
            suggested_fix: Some(
                "Refactor: Extract common logic into a shared function/module. Consider using composition or inheritance to eliminate duplication.".to_string()
            ),
            auto_fix_available: Some(false),
        })
    } else {
        None
    }
}

/// Calculate similarity between two patterns (0.0 to 1.0).
/// Delegates to `ast_parse_ts::fingerprint_similarity` (Jaccard on token sets).
fn pattern_similarity(a: &str, b: &str) -> f64 {
    fingerprint_similarity(a, b)
}

fn output_table(groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("No code duplication found. Clean code!");
        return;
    }

    let total_instances: usize = groups.iter().map(|g| g.instances.len()).sum();
    let files: std::collections::HashSet<&str> = groups
        .iter()
        .flat_map(|g| g.instances.iter().map(|i| i.file.as_str()))
        .collect();

    println!("CODE DUPLICATION");
    println!("{}", "─".repeat(70));
    println!();

    for (i, group) in groups.iter().enumerate() {
        println!("  Group {} ({} instances):", i + 1, group.instances.len());
        println!("    Pattern: {}", group.fingerprint);
        if let Some(hint) = &group.suggested_fix {
            println!("    Hint: {}", hint);
        }
        for inst in &group.instances {
            println!("      - {} ({}:{})", inst.function, inst.file, inst.line);
        }
        println!();
    }

    println!("{}", "─".repeat(70));
    println!("  Duplicate groups:    {}", groups.len());
    println!("  Total instances:     {}", total_instances);
    println!("  Files affected:      {}", files.len());

    let dup_ratio = total_instances as f64 / (total_instances + files.len()) as f64 * 100.0;
    if dup_ratio > 20.0 {
        println!();
        println!("  ⚠ Significant duplication detected. Consider refactoring.");
    }
}

fn output_json(groups: &[DuplicateGroup]) -> Result<(), Box<dyn std::error::Error>> {
    let total_instances: usize = groups.iter().map(|g| g.instances.len()).sum();
    let files: std::collections::HashSet<&str> = groups
        .iter()
        .flat_map(|g| g.instances.iter().map(|i| i.file.as_str()))
        .collect();

    let report = DupReport {
        groups: groups.to_vec(),
        summary: DupSummary {
            total_groups: groups.len(),
            total_instances,
            files_affected: files.len(),
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(groups: &[DuplicateGroup]) -> Result<(), Box<dyn std::error::Error>> {
    for group in groups {
        println!("{}", serde_json::to_string(group)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_similarity_identical() {
        assert!((pattern_similarity("A;B;C", "A;B;C") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_pattern_similarity_disjoint() {
        assert!((pattern_similarity("A;B;C", "D;E;F") - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_pattern_similarity_partial() {
        let sim = pattern_similarity("A;B;C", "A;B;D");
        assert!((0.5..1.0).contains(&sim), "Expected >= 0.5, got {}", sim);
    }

    #[test]
    fn test_pattern_similarity_empty() {
        assert_eq!(pattern_similarity("", "A"), 0.0);
        assert_eq!(pattern_similarity("A", ""), 0.0);
    }

    #[test]
    fn test_python_fingerprint() {
        let source = r#"
def foo(x, y):
    if x > 0:
        if y > 0:
            return x + y
        return x - y
    if y == 0:
        return 0
    return x * y

# Exact duplicate
def bar(x, y):
    if x > 0:
        if y > 0:
            return x + y
        return x - y
    if y == 0:
        return 0
    return x * y
"#;
        let fps =
            ast_parse_ts::parse_fingerprints(source, "test.py", ast_parse_ts::Language::Python);
        assert_eq!(fps.len(), 2, "Should find 2 Python functions");
        assert_eq!(
            fps[0].fingerprint, fps[1].fingerprint,
            "Identical functions should match"
        );
    }

    #[test]
    fn test_js_fingerprint() {
        let source = r#"
function foo(x, y) {
    if (x > 0) {
        if (y > 0) {
            return x + y;
        }
        return x - y;
    }
    return x * y;
}

function bar(x, y) {
    if (x > 0) {
        if (y > 0) {
            return x + y;
        }
        return x - y;
    }
    return x * y;
}
"#;
        let fps =
            ast_parse_ts::parse_fingerprints(source, "test.js", ast_parse_ts::Language::JavaScript);
        assert!(
            fps.len() >= 2,
            "Should find at least 2 JS functions, got {}",
            fps.len()
        );
        // Find two identical fingerprints among all detected functions
        let first_fp = &fps[0].fingerprint;
        let matches = fps.iter().filter(|f| f.fingerprint == *first_fp).count();
        assert!(matches >= 2, "Expected at least 2 identical fingerprints");
    }

    #[test]
    fn test_go_fingerprint() {
        let source = r#"
package test

func Foo(x int, y int) int {
    if x > 0 {
        if y > 0 {
            return x + y
        }
        return x - y
    }
    if y == 0 {
        return 0
    }
    return x * y
}

func Bar(x int, y int) int {
    if x > 0 {
        if y > 0 {
            return x + y
        }
        return x - y
    }
    if y == 0 {
        return 0
    }
    return x * y
}
"#;
        let fps = ast_parse_ts::parse_fingerprints(source, "test.go", ast_parse_ts::Language::Go);
        assert_eq!(fps.len(), 2, "Should find 2 Go functions");
        assert_eq!(
            fps[0].fingerprint, fps[1].fingerprint,
            "Identical functions should match"
        );
    }
}
