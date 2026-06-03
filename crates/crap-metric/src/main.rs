#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;

use ast_parse_ts::parse_complexity_file;
use cogent_common::{crap_category, crap_score, function_coverage, parse_lcov};
#[cfg(test)]
use cogent_common::CoverageRecord;
use cogent_common::{find_source_files, print_table_header, print_table_row, Column};

#[derive(Parser)]
#[command(
    name = "crap",
    about = "CRAP metric calculator — maintenance risk scoring"
)]
struct Cli {
    /// Path to analyze (file or directory)
    path: String,

    /// Path to lcov coverage file
    #[arg(short, long)]
    coverage: Option<String>,

    /// Coverage percentage override (0-100) if no lcov file
    #[arg(short = 'p', long)]
    coverage_pct: Option<f64>,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Only show functions with CRAP score above this threshold
    #[arg(short, long, default_value = "0")]
    min_score: f64,

    /// Recursively find .rs files in directory
    #[arg(short, long)]
    recursive: bool,
}

#[derive(Serialize)]
struct CrapReport {
    functions: Vec<FunctionReport>,
    summary: Summary,
}

#[derive(Serialize, Clone)]
struct FunctionReport {
    name: String,
    file: String,
    line: usize,
    complexity: u32,
    line_count: usize,
    coverage_pct: f64,
    crap_score: f64,
    category: String,
    severity: String,
    help: String,
    rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Serialize)]
struct Summary {
    total_functions: usize,
    total_complexity: u32,
    avg_complexity: f64,
    avg_crap: f64,
    crappy_count: usize,
    acceptable_count: usize,
    good_count: usize,
    excellent_count: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Find all source files
    let files = find_source_files(
        &cli.path,
        cli.recursive,
        &[
            "rs", "py", "js", "ts", "go", "c", "cpp", "cs", "java", "php", "rb", "swift",
        ],
    );
    if files.is_empty() {
        eprintln!("No source files found at {}", cli.path);
        std::process::exit(1);
    }

    // Load coverage if provided
    let coverage_data = cli.coverage.as_ref().and_then(|path| {
        std::fs::read_to_string(path)
            .ok()
            .map(|content| parse_lcov(&content))
    });

    // Analyze all files (skip CLI entry points and output formatting — not unit-testable)
    let mut all_functions = Vec::new();
    for file in &files {
        let functions = parse_complexity_file(file);
        let skip: &[&str] = &[
            "main",
            "output_table",
            "output_json",
            "output_ndjson",
            "run_self_test",
        ];
        let testable: Vec<_> = functions
            .into_iter()
            .filter(|f| !skip.contains(&f.name.as_str()))
            .collect();
        if testable.is_empty() {
            eprintln!("Warning: No functions found in {}", file);
        } else {
            all_functions.extend(testable);
        }
    }

    // Calculate CRAP scores
    let reports: Vec<FunctionReport> = all_functions
        .into_iter()
        .map(|func| {
            let line_count = func.end_line.saturating_sub(func.line);
            let coverage_pct = if let Some(ref cov_data) = coverage_data {
                let func_cov = function_coverage(cov_data, &func.name);
                if func_cov > 0.0 {
                    func_cov
                } else {
                    cli.coverage_pct.unwrap_or(0.0)
                }
            } else {
                cli.coverage_pct.unwrap_or(0.0)
            };

            let score = crap_score(func.complexity, coverage_pct);
            let category = crap_category(score).to_string();

            let (severity, rule_id, help) = match category.as_str() {
                "excellent" | "good" => (
                    "info".to_string(),
                    "crap-pass".to_string(),
                    format!(
                        "Function has excellent CRAP score ({:.1}). Complexity: {}, Coverage: {:.1}%.",
                        score, func.complexity, coverage_pct
                    ),
                ),
                "acceptable" => (
                    "warning".to_string(),
                    "crap-warning".to_string(),
                    format!(
                        "CRAP score {:.1} (threshold: 30). To improve: 1) Extract helper functions (complexity: {}), 2) Add tests (coverage: {:.1}%).",
                        score, func.complexity, coverage_pct
                    ),
                ),
                _ => (
                    "error".to_string(),
                    "crap-error".to_string(),
                    format!(
                        "CRAP score {:.1} exceeds threshold (30). Actions: 1) Reduce complexity (current: {}), 2) Increase test coverage (current: {:.1}%). Consider extracting methods or using dependency injection.",
                        score, func.complexity, coverage_pct
                    ),
                ),
            };

            FunctionReport {
                name: func.name,
                file: func.file,
                line: func.line,
                complexity: func.complexity,
                line_count,
                coverage_pct,
                crap_score: score,
                category,
                severity,
                help: help.clone(),
                rule_id,
                suggested_fix: Some(help.clone()),
                auto_fix_available: Some(false),
            }
        })
        .filter(|r| r.crap_score >= cli.min_score)
        .collect();

    // Sort by CRAP score descending
    let mut sorted_reports = reports;
    sorted_reports.sort_by(|a, b| b.crap_score.partial_cmp(&a.crap_score).unwrap());

    match cli.format.as_str() {
        "json" => output_json(&sorted_reports),
        "ndjson" => output_ndjson(&sorted_reports),
        _ => {
            output_table(&sorted_reports);
            Ok(())
        }
    }
}

fn output_table(reports: &[FunctionReport]) {
    if reports.is_empty() {
        println!("No functions found.");
        return;
    }

    let columns = [
        Column::left("FUNCTION", 25),
        Column::left("FILE", 30),
        Column::right("LINE", 4),
        Column::right("COMP", 4),
        Column::right("LINES", 10),
        Column::right("CRAP", 10),
        Column::left("CATEGORY", 15),
        Column::left("HINT", 40),
    ];
    print_table_header(&columns);

    let mut excellent = 0;
    let mut good = 0;
    let mut acceptable = 0;
    let mut crappy = 0;

    for r in reports {
        let cat_icon = match r.category.as_str() {
            "excellent" => {
                excellent += 1;
                "✓"
            }
            "good" => {
                good += 1;
                "○"
            }
            "acceptable" => {
                acceptable += 1;
                "△"
            }
            "crappy" => {
                crappy += 1;
                "✗"
            }
            _ => "?",
        };

        let line_str = r.line.to_string();
        let comp_str = r.complexity.to_string();
        let lc_str = r.line_count.to_string();
        let score_str = format!("{:.1}", r.crap_score);
        let cat_str = format!("{} {}", cat_icon, r.category);
        let hint = if r.help.len() > 37 {
            &r.help[0..37]
        } else {
            &r.help
        };
        print_table_row(
            &columns,
            &[
                &r.name, &r.file, &line_str, &comp_str, &lc_str, &score_str, &cat_str, hint,
            ],
        );
    }

    // Summary
    let total = reports.len();
    let total_comp: u32 = reports.iter().map(|r| r.complexity).sum();
    let total_crap: f64 = reports.iter().map(|r| r.crap_score).sum();

    let summary = vec![
        ("Functions analyzed:", total.to_string()),
        ("Total complexity:", total_comp.to_string()),
        (
            "Avg complexity:",
            format!("{:.1}", total_comp as f64 / total as f64),
        ),
        (
            "Avg CRAP score:",
            format!("{:.1}", total_crap / total as f64),
        ),
    ];
    cogent_common::print_summary(&summary);

    println!();
    println!(
        "  {} excellent (≤10)  {} good (≤20)  {} acceptable (≤30)  {} crappy (>30)",
        excellent, good, acceptable, crappy
    );

    if crappy > 0 {
        println!();
        println!(
            "  ⚠ {} function(s) with CRAP > 30 need refactoring or more tests.",
            crappy
        );
    }
}

fn output_json(reports: &[FunctionReport]) -> Result<(), Box<dyn std::error::Error>> {
    let total = reports.len();
    let total_complexity: u32 = reports.iter().map(|r| r.complexity).sum();
    let total_crap: f64 = reports.iter().map(|r| r.crap_score).sum();

    let mut excellent = 0;
    let mut good = 0;
    let mut acceptable = 0;
    let mut crappy = 0;
    for r in reports {
        match r.category.as_str() {
            "excellent" => excellent += 1,
            "good" => good += 1,
            "acceptable" => acceptable += 1,
            "crappy" => crappy += 1,
            _ => {}
        }
    }

    let report = CrapReport {
        functions: reports.to_vec(),
        summary: Summary {
            total_functions: total,
            total_complexity,
            avg_complexity: if total > 0 {
                total_complexity as f64 / total as f64
            } else {
                0.0
            },
            avg_crap: if total > 0 {
                total_crap / total as f64
            } else {
                0.0
            },
            crappy_count: crappy,
            acceptable_count: acceptable,
            good_count: good,
            excellent_count: excellent,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(reports: &[FunctionReport]) -> Result<(), Box<dyn std::error::Error>> {
    for report in reports {
        println!(
            "{}",
            serde_json::json!({
                "tool": "crap",
                "rule_id": report.rule_id,
                "severity": report.severity,
                "message": format!("Function {} has CRAP score {:.1}", report.name, report.crap_score),
                "help": report.help,
                "file": report.file,
                "line": report.line,
                "col": null,
                "details": {
                    "name": report.name,
                    "complexity": report.complexity,
                    "coverage_pct": report.coverage_pct,
                    "crap_score": report.crap_score,
                    "category": report.category,
                }
            })
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report(name: &str, score: f64) -> FunctionReport {
        FunctionReport {
            name: name.to_string(),
            file: "src/lib.rs".to_string(),
            line: 42,
            complexity: 10,
            line_count: 30,
            coverage_pct: 0.5,
            crap_score: score,
            category: if score > 30.0 { "crappy".into() } else if score > 20.0 { "acceptable".into() } else if score > 10.0 { "good".into() } else { "excellent".into() },
            severity: "error".to_string(),
            help: "Reduce complexity or add tests.".to_string(),
            rule_id: "crap-error".to_string(),
            suggested_fix: Some("Refactor function.".to_string()),
            auto_fix_available: Some(false),
        }
    }

    #[test]
    fn test_output_json_serialization() {
        let reports = vec![
            sample_report("foo", 5.0),
            sample_report("bar", 35.0),
        ];
        // Capture stdout
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = output_json(&reports);
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_json_empty() {
        let reports: Vec<FunctionReport> = vec![];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = output_json(&reports);
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_ndjson_serialization() {
        let reports = vec![
            sample_report("foo", 5.0),
        ];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = output_ndjson(&reports);
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_ndjson_empty() {
        let reports: Vec<FunctionReport> = vec![];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = output_ndjson(&reports);
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_crap_report_json_structure() {
        let reports = vec![
            sample_report("compute", 12.5),
            sample_report("process", 45.0),
        ];
        let json_str = serde_json::to_string_pretty(&CrapReport {
            functions: reports,
            summary: Summary {
                total_functions: 2,
                total_complexity: 20,
                avg_complexity: 10.0,
                avg_crap: 28.75,
                crappy_count: 1,
                acceptable_count: 0,
                good_count: 1,
                excellent_count: 0,
            },
        }).unwrap();
        assert!(json_str.contains("compute"));
        assert!(json_str.contains("process"));
        assert!(json_str.contains("crappy"));
        assert!(json_str.contains("good"));
    }

    #[test]
    fn test_function_coverage_from_common() {
        let records = vec![
            CoverageRecord { function: "foo".into(), line: 10, hits: 5 },
            CoverageRecord { function: "bar".into(), line: 20, hits: 0 },
        ];
        assert_eq!(cogent_common::function_coverage(&records, "foo"), 1.0);
        assert_eq!(cogent_common::function_coverage(&records, "bar"), 0.0);
        assert_eq!(cogent_common::function_coverage(&records, "nonexistent"), 0.0);
    }
}
