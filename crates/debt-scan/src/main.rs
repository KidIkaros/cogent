#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;

use cogent_common::{
    find_source_files, get_git_blame_batch, print_table_header, print_table_row, Column,
};

#[derive(Parser)]
#[command(
    name = "debt",
    about = "Technical debt scanner -- track TODO/FIXME/HACK/XXX markers"
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

    /// Only show markers of this type (comma-separated: todo,fixme,hack,xxx)
    #[arg(long)]
    marker: Option<String>,

    /// Sort by: age (default), file, type, author
    #[arg(short, long, default_value = "age")]
    sort: String,
}

#[derive(Debug, Clone, Serialize)]
struct DebtItem {
    file: String,
    line: usize,
    marker_type: String,
    text: String,
    author: Option<String>,
    date: Option<String>,
    /// Code context (surrounding lines) for the finding
    #[serde(skip_serializing_if = "Option::is_none")]
    code_context: Option<String>,
    /// Suggested fix for the technical debt item
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Serialize)]
struct DebtReport {
    items: Vec<DebtItem>,
    summary: DebtSummary,
}

#[derive(Serialize)]
struct DebtSummary {
    total: usize,
    todo: usize,
    fixme: usize,
    hack: usize,
    xxx: usize,
    by_author: HashMap<String, usize>,
}

const MARKERS: &[(&str, &str)] = &[
    ("TODO", "todo"),
    ("FIXME", "fixme"),
    ("HACK", "hack"),
    ("XXX", "xxx"),
    ("WARN", "warn"),
    ("BUG", "bug"),
    ("OPTIMIZE", "optimize"),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let extensions = [
        "rs", "py", "js", "ts", "go", "c", "cpp", "h", "java", "rb", "php", "swift",
    ];
    let files = find_source_files(&cli.path, cli.recursive, &extensions);
    if files.is_empty() {
        return Err(format!("No source files found at {}", cli.path).into());
    }

    let marker_filter = cli.marker.as_ref().map(|m| {
        m.split(',')
            .map(|s| s.trim().to_lowercase())
            .collect::<Vec<_>>()
    });

    let items = scan_files(&files, &marker_filter);
    let items = sort_items(items, &cli.sort);

    match cli.format.as_str() {
        "json" => output_json(&items),
        "ndjson" => output_ndjson(&items),
        _ => {
            output_table(&items);
            Ok(())
        }
    }
}

fn scan_files(files: &[String], marker_filter: &Option<Vec<String>>) -> Vec<DebtItem> {
    let mut items = Vec::new();
    for file_path in files {
        let Ok(source) = std::fs::read_to_string(file_path) else {
            continue;
        };
        scan_source(file_path, &source, marker_filter, &mut items);
    }
    items
}

fn scan_source(
    file_path: &str,
    source: &str,
    marker_filter: &Option<Vec<String>>,
    items: &mut Vec<DebtItem>,
) {
    // First pass: find all marker lines
    let mut marker_lines: Vec<(usize, &str, &str)> = Vec::new(); // (line_num, marker_name, marker_type)

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        for (marker_name, marker_type) in MARKERS {
            if !has_marker(trimmed, marker_name) {
                continue;
            }
            if is_marker_in_string(trimmed, marker_name) {
                continue;
            }
            if is_filtered_out(marker_filter, marker_type) {
                continue;
            }

            marker_lines.push((line_num + 1, *marker_name, *marker_type));
            break; // Only count first matching marker per line
        }
    }

    // Batch git blame for all marker lines
    let line_numbers: Vec<usize> = marker_lines.iter().map(|(ln, _, _)| *ln).collect();
    let blame_info = get_git_blame_batch(file_path, &line_numbers);

    // Create DebtItems with blame info
    for (line_num, marker_name, marker_type) in marker_lines {
        let line_idx = line_num - 1;
        let line_text = source.lines().nth(line_idx).unwrap_or("");
        let text = extract_comment_text(line_text, marker_name);
        let (author, date) = blame_info.get(&line_num).cloned().unwrap_or((None, None));

        items.push(DebtItem {
            file: file_path.to_string(),
            line: line_num,
            marker_type: marker_type.to_string(),
            text: text.clone(),
            author,
            date,
            code_context: None,
            suggested_fix: get_suggested_fix(marker_type),
            auto_fix_available: Some(false),
        });
    }
}

fn has_marker(trimmed: &str, marker_name: &str) -> bool {
    let patterns = [
        format!("{}:", marker_name),
        format!("{}(", marker_name),
        format!("{} ", marker_name),
    ];
    patterns.iter().any(|p| trimmed.contains(p))
}

fn is_filtered_out(marker_filter: &Option<Vec<String>>, marker_type: &str) -> bool {
    match marker_filter {
        Some(filter) => !filter.contains(&marker_type.to_string()),
        None => false,
    }
}

/// Returns a suggested fix for a given technical debt marker type
fn get_suggested_fix(marker_type: &str) -> Option<String> {
    match marker_type {
        "todo" => Some("Create an issue in your issue tracker (e.g., GitHub Issues) and replace this TODO with a link to the issue. Example: 'TODO: See https://github.com/org/repo/issues/123'".to_string()),
        "fixme" => Some("This indicates a known bug. Either fix it now or create an issue: 'FIXME: Bug with X, see https://github.com/org/repo/issues/456'".to_string()),
        "hack" => Some("HACK indicates a temporary workaround. Plan to refactor: create a follow-up issue and add a deadline. Replace with: 'HACK: Temporary workaround until X is fixed (see issue #789)'".to_string()),
        "xxx" => Some("XXX marks dangerous or questionable code. Review this code carefully and either fix it or document why it's needed. Consider adding a code comment explaining the rationale.".to_string()),
        "warn" => Some("WARNING marker indicates potential issues. Review the code, address the warning if valid, and remove the marker. Document any trade-offs made.".to_string()),
        "bug" => Some("BUG marker indicates a known defect. Prioritize fixing it or create a high-priority issue: 'BUG: Description of bug (fix in PR #123)'".to_string()),
        "optimize" => Some("OPTIMIZE suggests a performance improvement opportunity. Profile first to confirm the bottleneck, then create a tracked issue or implement the optimization with benchmarks.".to_string()),
        _ => Some("Review this marker and consider replacing it with a link to an issue tracker or inline documentation explaining the rationale.".to_string()),
    }
}

fn sort_items(mut items: Vec<DebtItem>, sort: &str) -> Vec<DebtItem> {
    match sort {
        "file" => items.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line))),
        "type" => items.sort_by(|a, b| a.marker_type.cmp(&b.marker_type)),
        "author" => items.sort_by(|a, b| a.author.cmp(&b.author)),
        _ => {}
    }
    items
}

fn is_marker_in_string(line: &str, marker: &str) -> bool {
    // Check if the marker appears between quotes (inside a string literal)
    // Look for patterns like "\"TODO" or "TODO\"" or within quoted strings
    let mut in_string = false;
    let mut prev_char = '\0';
    let chars: Vec<char> = line.chars().collect();
    let marker_chars: Vec<char> = marker.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '"' && prev_char != '\\' {
            in_string = !in_string;
        }

        if in_string {
            // Check if marker starts here
            if i + marker_chars.len() <= chars.len() {
                let slice: String = chars[i..i + marker_chars.len()].iter().collect();
                if slice == marker {
                    return true;
                }
            }
        }

        prev_char = chars[i];
        i += 1;
    }
    false
}

fn extract_comment_text(line: &str, marker: &str) -> String {
    // Find the marker and extract text after it
    if let Some(pos) = line.find(marker) {
        let after = &line[pos + marker.len()..];
        // Remove leading :, (, whitespace
        let after = after.trim_start_matches(':').trim_start_matches('(').trim();
        // Remove trailing */
        let after = after.trim_end_matches("*/").trim_end_matches(')').trim();
        after.to_string()
    } else {
        line.trim().to_string()
    }
}

fn output_table(items: &[DebtItem]) {
    if items.is_empty() {
        println!("No technical debt markers found. Clean code!");
        return;
    }

    let columns = [
        Column::left("TYPE", 8),
        Column::left("FILE", 40),
        Column::right("LINE", 4),
        Column::left("AUTHOR", 12),
        Column::left("TEXT", 25),
        Column::left("HINT", 40),
    ];
    print_table_header(&columns);

    let mut todo = 0;
    let mut fixme = 0;
    let mut hack = 0;
    let mut xxx = 0;
    let mut by_author: HashMap<String, usize> = HashMap::new();

    for item in items {
        let icon = match item.marker_type.as_str() {
            "todo" => {
                todo += 1;
                "○"
            }
            "fixme" => {
                fixme += 1;
                "⚠"
            }
            "hack" => {
                hack += 1;
                "✗"
            }
            "xxx" => {
                xxx += 1;
                "!"
            }
            _ => "?",
        };

        let author = item.author.as_deref().unwrap_or("unknown");
        *by_author.entry(author.to_string()).or_insert(0) += 1;

        let line_str = item.line.to_string();
        let type_str = format!("{} {}", icon, item.marker_type.to_uppercase());
        let hint = item.suggested_fix.as_deref().unwrap_or("");
        let hint_truncated = if hint.len() > 37 { &hint[0..37] } else { hint };
        print_table_row(
            &columns,
            &[
                &type_str,
                &item.file,
                &line_str,
                author,
                &item.text,
                hint_truncated,
            ],
        );
    }

    // Print summary
    let summary = vec![
        ("Total markers:", items.len().to_string()),
        ("TODO:", format!("{} (can wait)", todo)),
        ("FIXME:", format!("{} (should fix)", fixme)),
        ("HACK:", format!("{} (needs refactor)", hack)),
    ];
    cogent_common::print_summary(&summary);

    if xxx > 0 {
        println!("  XXX:            {} (DANGER)", xxx);
    }

    if !by_author.is_empty() {
        println!();
        println!("  By author:");
        let mut authors: Vec<_> = by_author.iter().collect();
        authors.sort_by(|a, b| b.1.cmp(a.1));
        for (author, count) in authors.iter().take(5) {
            println!("    {}: {}", author, count);
        }
    }

    let debt_ratio = (fixme + hack + xxx) as f64 / items.len() as f64 * 100.0;
    println!();
    if debt_ratio > 50.0 {
        println!(
            "  ⚠ {:.0}% of markers are actionable (FIXME/HACK/XXX). High debt.",
            debt_ratio
        );
    } else if debt_ratio > 20.0 {
        println!(
            "  ○ {:.0}% of markers are actionable. Moderate debt.",
            debt_ratio
        );
    } else {
        println!(
            "  ✓ {:.0}% of markers are actionable. Low debt.",
            debt_ratio
        );
    }
}

fn output_json(items: &[DebtItem]) -> Result<(), Box<dyn std::error::Error>> {
    let mut todo = 0;
    let mut fixme = 0;
    let mut hack = 0;
    let mut xxx = 0;
    let mut by_author: HashMap<String, usize> = HashMap::new();

    for item in items {
        match item.marker_type.as_str() {
            "todo" => todo += 1,
            "fixme" => fixme += 1,
            "hack" => hack += 1,
            "xxx" => xxx += 1,
            _ => {}
        }
        let author = item.author.as_deref().unwrap_or("unknown");
        *by_author.entry(author.to_string()).or_insert(0) += 1;
    }

    let report = DebtReport {
        items: items.to_vec(),
        summary: DebtSummary {
            total: items.len(),
            todo,
            fixme,
            hack,
            xxx,
            by_author,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(items: &[DebtItem]) -> Result<(), Box<dyn std::error::Error>> {
    for item in items {
        println!("{}", serde_json::to_string(item)?);
    }
    Ok(())
}
