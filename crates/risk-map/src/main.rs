#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use ast_parse_ts::{parse_complexity_file, Language};
use cogent_common::{get_git_churn, print_table_header, print_table_row, separator, Column};

#[derive(Parser)]
#[command(
    name = "riskmap",
    about = "Risk map -- cross-reference git churn with code complexity to find danger zones"
)]
struct Cli {
    /// Path to the repository root
    path: String,

    /// Git log time range (e.g., '3 months ago', '2024-01-01')
    #[arg(short, long, default_value = "6 months ago")]
    since: String,

    /// Output format: table (default), json, or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Only show files above this risk score (0-100)
    #[arg(long, default_value = "0")]
    min_risk: u32,
}

#[derive(Debug, Clone, Serialize)]
struct FileRisk {
    file: String,
    /// Number of commits touching this file in the time range
    churn: u32,
    /// Total cyclomatic complexity across all functions
    complexity: u32,
    /// Number of functions
    function_count: u32,
    /// Risk score: churn * complexity (normalized 0-100)
    risk_score: u32,
    /// Risk category
    category: String,
    /// Number of unsafe blocks in the file
    unsafe_count: u32,
    /// Most complex functions
    hot_functions: Vec<String>,
    /// Suggested fix for high-risk files
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
    /// Code context (key problematic sections)
    #[serde(skip_serializing_if = "Option::is_none")]
    code_context: Option<String>,
}

#[derive(Serialize)]
struct RiskReport {
    files: Vec<FileRisk>,
    summary: RiskSummary,
}

#[derive(Serialize)]
struct RiskSummary {
    total_files: usize,
    high_risk: usize,
    medium_risk: usize,
    low_risk: usize,
    total_unsafe: u32,
    danger_zone: Vec<String>, // files with both high churn AND high complexity
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(&cli.path);

    let churn_data = get_git_churn(repo_root, &cli.since);
    if churn_data.is_empty() {
        return Err("No git churn data found. Is this a git repository."
            .to_string()
            .into());
    }

    let mut file_risks = build_file_risks(repo_root, &churn_data);
    file_risks.sort_by_key(|b| std::cmp::Reverse(b.risk_score));

    if cli.min_risk > 0 {
        file_risks.retain(|f| f.risk_score >= cli.min_risk);
    }

    match cli.format.as_str() {
        "json" => output_json(&file_risks),
        "ndjson" => output_ndjson(&file_risks),
        _ => {
            output_table(&file_risks);
            Ok(())
        }
    }
}

const SUPPORTED_EXTS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp", "cc", "cxx",
    "hpp", "cs", "java", "php", "rb", "swift",
];

fn build_file_risks(repo_root: &Path, churn_data: &HashMap<String, u32>) -> Vec<FileRisk> {
    let mut file_risks = Vec::new();
    for (file_path, churn_count) in churn_data {
        let full_path = repo_root.join(file_path);
        if !full_path.exists() {
            continue;
        }
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !SUPPORTED_EXTS.contains(&ext) {
            continue;
        }
        let full_path_str = full_path.to_string_lossy().to_string();
        let lang = Language::from_extension(&full_path_str);

        let ts_funcs = parse_complexity_file(&full_path_str);
        let mut risk = build_risk_from_ts(file_path, *churn_count, &ts_funcs, lang);
        if lang == Language::Rust {
            let unsafe_count = count_unsafe_blocks(&full_path_str);
            let effective_complexity = risk.complexity + (unsafe_count * 2);
            let (risk_score, category) =
                compute_risk_score(*churn_count, effective_complexity, unsafe_count);
            risk.risk_score = risk_score;
            risk.category = category;
            risk.unsafe_count = unsafe_count;
        }
        file_risks.push(risk);
    }
    file_risks
}

fn build_risk_from_ts(
    file_path: &str,
    churn: u32,
    funcs: &[ast_parse_ts::FunctionInfo],
    _lang: Language,
) -> FileRisk {
    let total_complexity: u32 = funcs.iter().map(|f| f.complexity).sum();
    let hot = hot_functions(
        funcs
            .iter()
            .map(|f| (f.name.clone(), f.complexity))
            .collect(),
    );
    let (risk_score, category) = compute_risk_score(churn, total_complexity, 0);
    FileRisk {
        file: file_path.to_string(),
        churn,
        complexity: total_complexity,
        function_count: funcs.len() as u32,
        risk_score,
        category,
        unsafe_count: 0,
        hot_functions: hot,
        suggested_fix: None,
        auto_fix_available: None,
        code_context: None,
    }
}

fn count_unsafe_blocks(path: &str) -> u32 {
    std::fs::read_to_string(path)
        .map(|content| {
            content.matches("unsafe {").count() as u32 + content.matches("unsafe(").count() as u32
        })
        .unwrap_or(0)
}

fn hot_functions(mut funcs: Vec<(String, u32)>) -> Vec<String> {
    funcs.sort_by_key(|b| std::cmp::Reverse(b.1));
    funcs
        .into_iter()
        .take(3)
        .filter(|(_, c)| *c > 3)
        .map(|(n, c)| format!("{} (c:{})", n, c))
        .collect()
}

fn compute_risk_score(churn: u32, effective_complexity: u32, _unsafe_count: u32) -> (u32, String) {
    // Use sqrt(churn) to prevent churn from dominating the score.
    // A file changing 25× isn't 25× riskier than one changing 5× — it's ~2.2× riskier.
    let weighted_churn = (churn as f64).sqrt();
    let raw_risk = (weighted_churn * effective_complexity as f64) / 20.0;
    let risk_score = (raw_risk as u32).min(100);
    (risk_score, risk_category(risk_score))
}

fn risk_category(risk_score: u32) -> String {
    if risk_score >= 70 {
        "DANGER".to_string()
    } else if risk_score >= 40 {
        "HIGH".to_string()
    } else if risk_score >= 20 {
        "MEDIUM".to_string()
    } else {
        "LOW".to_string()
    }
}

fn output_table(file_risks: &[FileRisk]) {
    if file_risks.is_empty() {
        println!("No risk data found.");
        return;
    }

    let high = file_risks
        .iter()
        .filter(|f| f.category == "DANGER" || f.category == "HIGH")
        .count();
    let medium = file_risks.iter().filter(|f| f.category == "MEDIUM").count();
    let low = file_risks.iter().filter(|f| f.category == "LOW").count();

    // Danger zone: high churn AND high complexity
    let danger_zone: Vec<&FileRisk> = file_risks
        .iter()
        .filter(|f| f.churn > 5 && f.complexity > 20)
        .collect();

    println!("RISK MAP: CHURN x COMPLEXITY");
    println!("{}", separator(95));

    let columns = [
        Column::left("FILE", 45),
        Column::right("CHURN", 6),
        Column::right("COMP", 6),
        Column::right("RISK", 5),
        Column::left("STATUS", 8),
        Column::right("UNSAFE", 7),
        Column::left("HOT FUNCTIONS", 20),
    ];
    print_table_header(&columns);

    for f in file_risks.iter().take(30) {
        let icon = match f.category.as_str() {
            "DANGER" => "*",
            "HIGH" => "!",
            "MEDIUM" => "~",
            "LOW" => ".",
            _ => "?",
        };

        let hot = if f.hot_functions.is_empty() {
            String::new()
        } else {
            f.hot_functions[0].clone()
        };

        let churn_str = f.churn.to_string();
        let comp_str = f.complexity.to_string();
        let risk_str = f.risk_score.to_string();
        let unsafe_str = f.unsafe_count.to_string();
        let status_str = format!("{} {}", icon, f.category);
        print_table_row(
            &columns,
            &[
                &f.file,
                &churn_str,
                &comp_str,
                &risk_str,
                &status_str,
                &unsafe_str,
                &hot,
            ],
        );
    }

    println!("{}", separator(95));
    println!();
    println!("RISK SUMMARY");
    println!("  Files analyzed:    {}", file_risks.len());
    println!("  ! DANGER/HIGH:     {} (changing often AND complex)", high);
    println!("  ~ MEDIUM:          {}", medium);
    println!("  . LOW:             {}", low);

    let total_unsafe: u32 = file_risks.iter().map(|f| f.unsafe_count).sum();
    if total_unsafe > 0 {
        let max_unsafe = file_risks.iter().map(|f| f.unsafe_count).max().unwrap_or(0);
        println!();
        println!(
            "  UNSAFE WEIGHTING:  {} total unsafe blocks across {} files",
            total_unsafe,
            file_risks.iter().filter(|f| f.unsafe_count > 0).count()
        );
        println!("    (Each unsafe block adds +2 to effective complexity in risk scoring)");
        if max_unsafe > 0 {
            println!("    Most unsafe:       {} blocks in top file", max_unsafe);
        }
    }

    if !danger_zone.is_empty() {
        println!();
        println!("  DANGER ZONE (high churn + high complexity):");
        for f in danger_zone.iter().take(5) {
            println!(
                "    {} (churn: {}, complexity: {})",
                f.file, f.churn, f.complexity
            );
            for hf in &f.hot_functions {
                println!("      |- {}", hf);
            }
        }
        println!();
        println!("  These files are changing often AND are complex.");
        println!("  They're the most likely source of bugs. Consider refactoring.");
    } else {
        println!();
        println!("  No danger zone detected. Complex files aren't changing much.");
    }
}

fn output_json(file_risks: &[FileRisk]) -> Result<(), Box<dyn std::error::Error>> {
    let high = file_risks
        .iter()
        .filter(|f| f.category == "DANGER" || f.category == "HIGH")
        .count();
    let medium = file_risks.iter().filter(|f| f.category == "MEDIUM").count();
    let low = file_risks.iter().filter(|f| f.category == "LOW").count();

    let danger_zone: Vec<String> = file_risks
        .iter()
        .filter(|f| f.churn > 5 && f.complexity > 20)
        .map(|f| f.file.clone())
        .collect();
    let total_unsafe: u32 = file_risks.iter().map(|f| f.unsafe_count).sum();

    let report = RiskReport {
        files: file_risks.to_vec(),
        summary: RiskSummary {
            total_files: file_risks.len(),
            high_risk: high,
            medium_risk: medium,
            low_risk: low,
            total_unsafe,
            danger_zone,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(file_risks: &[FileRisk]) -> Result<(), Box<dyn std::error::Error>> {
    for file_risk in file_risks {
        println!("{}", serde_json::to_string(file_risk)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── risk_category ──

    #[test]
    fn test_risk_category_danger() {
        assert_eq!(risk_category(70), "DANGER");
        assert_eq!(risk_category(100), "DANGER");
    }

    #[test]
    fn test_risk_category_high() {
        assert_eq!(risk_category(40), "HIGH");
        assert_eq!(risk_category(69), "HIGH");
    }

    #[test]
    fn test_risk_category_medium() {
        assert_eq!(risk_category(20), "MEDIUM");
        assert_eq!(risk_category(39), "MEDIUM");
    }

    #[test]
    fn test_risk_category_low() {
        assert_eq!(risk_category(0), "LOW");
        assert_eq!(risk_category(19), "LOW");
    }

    // ── compute_risk_score ──

    #[test]
    fn test_compute_risk_score_zero_churn() {
        let (score, cat) = compute_risk_score(0, 0, 0);
        assert_eq!(score, 0);
        assert_eq!(cat, "LOW");
    }

    #[test]
    fn test_compute_risk_score_moderate() {
        let (score, cat) = compute_risk_score(10, 50, 0);
        // sqrt(10) * 50 / 20 ≈ 3.16 * 2.5 = 7.9
        assert!(score < 100);
        assert_eq!(cat, "LOW");
    }

    #[test]
    fn test_compute_risk_score_high_churn_and_complexity() {
        let (score, cat) = compute_risk_score(100, 200, 5);
        // sqrt(100) * 200 / 20 = 10 * 10 = 100, capped at 100
        assert_eq!(score, 100);
        assert_eq!(cat, "DANGER");
    }

    #[test]
    fn test_compute_risk_score_caps_at_100() {
        let (score, _) = compute_risk_score(1000, 1000, 0);
        assert_eq!(score, 100);
    }

    // ── hot_functions ──

    #[test]
    fn test_hot_functions_empty() {
        assert!(hot_functions(vec![]).is_empty());
    }

    #[test]
    fn test_hot_functions_returns_top_3_with_complexity() {
        let funcs = vec![
            ("foo".into(), 10),
            ("bar".into(), 5),
            ("baz".into(), 4),  // > 3, so included
            ("qux".into(), 1),  // <= 3, filtered out
        ];
        let hot = hot_functions(funcs);
        assert_eq!(hot.len(), 3); // foo, bar, baz all > 3
        assert!(hot[0].contains("foo"));
        assert!(hot[1].contains("bar"));
        assert!(hot[2].contains("baz"));
    }

    #[test]
    fn test_hot_functions_filters_complexity_below_3() {
        let funcs = vec![
            ("small".into(), 2),
            ("tiny".into(), 1),
        ];
        assert!(hot_functions(funcs).is_empty());
    }

    #[test]
    fn test_hot_functions_sorted_by_complexity_desc() {
        let funcs = vec![
            ("medium".into(), 15),
            ("high".into(), 42),
            ("low".into(), 5),
        ];
        let hot = hot_functions(funcs);
        assert!(hot[0].contains("high"));
        assert!(hot[1].contains("medium"));
        assert!(hot[2].contains("low"));
    }

    // ── count_unsafe_blocks ──

    #[test]
    fn test_count_unsafe_blocks_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("safe.rs");
        std::fs::write(&path, "fn main() { let x = 1; }").unwrap();
        assert_eq!(count_unsafe_blocks(path.to_str().unwrap()), 0);
    }

    #[test]
    fn test_count_unsafe_blocks_with_unsafe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unsafe.rs");
        std::fs::write(&path, "unsafe { transmute(x) }\nunsafe { ptr.read() }").unwrap();
        assert_eq!(count_unsafe_blocks(path.to_str().unwrap()), 2);
    }

    #[test]
    fn test_count_unsafe_blocks_nonexistent_file() {
        assert_eq!(count_unsafe_blocks("/nonexistent/file.rs"), 0);
    }
}
