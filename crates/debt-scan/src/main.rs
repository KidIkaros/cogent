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

// ═══════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── has_marker ──

    #[test]
    fn test_has_marker_with_colon() {
        assert!(has_marker("// TODO: implement this", "TODO"));
    }

    #[test]
    fn test_has_marker_with_parenthesis() {
        assert!(has_marker("// FIXME(bug): something", "FIXME"));
    }

    #[test]
    fn test_has_marker_with_space() {
        assert!(has_marker("// HACK later", "HACK"));
    }

    #[test]
    fn test_has_marker_no_match() {
        assert!(!has_marker("// normal comment", "TODO"));
    }

    #[test]
    fn test_has_marker_empty_string() {
        assert!(!has_marker("", "TODO"));
    }

    #[test]
    fn test_has_marker_only_word_without_suffix() {
        // "TODO" alone without :, (, or space should NOT match
        assert!(!has_marker("//TODO", "TODO"));
    }

    #[test]
    fn test_has_marker_case_insensitive_does_not_match_lowercase() {
        // Markers are uppercase, but has_marker uses contains() which is case-sensitive
        assert!(!has_marker("// todo: something", "TODO"));
    }

    #[test]
    fn test_has_marker_at_start_of_line() {
        assert!(has_marker("TODO: do it", "TODO"));
    }

    #[test]
    fn test_has_marker_multiple_markers_on_one_line() {
        // Should match the first marker pattern found
        assert!(has_marker("// TODO: and FIXME: both", "TODO"));
        assert!(has_marker("// TODO: and FIXME: both", "FIXME"));
    }

    #[test]
    fn test_has_marker_all_marker_types() {
        for (name, _) in MARKERS {
            assert!(has_marker(&format!("// {}: test", name), name));
        }
    }

    #[test]
    fn test_has_marker_rejects_unsupported_marker() {
        assert!(!has_marker("// NOTE: this is just a note", "TODO"));
    }

    // ── is_marker_in_string ──

    #[test]
    fn test_is_marker_in_string_basic() {
        assert!(is_marker_in_string("let s = \"TODO: fix this\";", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_outside_quotes() {
        assert!(!is_marker_in_string("// TODO: fix this", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_not_present() {
        assert!(!is_marker_in_string("// normal comment", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_escaped_quote_before_marker() {
        // The quote is escaped so it shouldn't toggle string state
        assert!(!is_marker_in_string("\\\" // TODO: not in a string now", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_marker_after_quoted_section() {
        // Marker after closing quote — not in string
        assert!(!is_marker_in_string("let s = \"hello\"; // TODO: fix", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_multiple_strings() {
        // Marker in second string literal
        assert!(is_marker_in_string("let a = \"hello\"; let b = \"TODO: fixme\";", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_empty_line() {
        assert!(!is_marker_in_string("", "TODO"));
    }

    #[test]
    fn test_is_marker_in_string_all_marker_types() {
        for (name, _) in MARKERS {
            assert!(is_marker_in_string(&format!("let s = \"{}: hidden\";", name), name));
        }
    }

    // ── extract_comment_text ──

    #[test]
    fn test_extract_comment_text_with_colon() {
        assert_eq!(extract_comment_text("// TODO: implement this", "TODO"), "implement this");
    }

    #[test]
    fn test_extract_comment_text_with_parenthesis() {
        assert_eq!(extract_comment_text("// FIXME(urgent): fix now", "FIXME"), "urgent): fix now");
    }

    #[test]
    fn test_extract_comment_text_with_space() {
        assert_eq!(extract_comment_text("// HACK make it work", "HACK"), "make it work");
    }

    #[test]
    fn test_extract_comment_text_removes_trailing_star_slash() {
        assert_eq!(extract_comment_text("/* TODO: fix this */", "TODO"), "fix this");
    }

    #[test]
    fn test_extract_comment_text_removes_trailing_paren() {
        // After TODO, ': ' is stripped, then '(' is left, then trailing ')' is stripped
        assert_eq!(extract_comment_text("// TODO: (do something)", "TODO"), "(do something");
    }

    #[test]
    fn test_extract_comment_text_marker_not_found() {
        assert_eq!(extract_comment_text("// no marker here", "TODO"), "// no marker here");
    }

    #[test]
    fn test_extract_comment_text_marker_at_end() {
        assert_eq!(extract_comment_text("// TODO:", "TODO"), "");
    }

    #[test]
    fn test_extract_comment_text_marker_with_leading_text() {
        assert_eq!(extract_comment_text("x = 1; // TODO: clean up", "TODO"), "clean up");
    }

    #[test]
    fn test_extract_comment_text_marker_only() {
        assert_eq!(extract_comment_text("TODO", "TODO"), "");
    }

    #[test]
    fn test_extract_comment_text_marker_with_mixed_case() {
        // extract_comment_text does case-sensitive search
        assert_eq!(extract_comment_text("// todo: lowercase", "TODO"), "// todo: lowercase");
    }

    // ── sort_items ──

    fn make_item(file: &str, line: usize, marker_type: &str, author: Option<&str>) -> DebtItem {
        DebtItem {
            file: file.to_string(),
            line,
            marker_type: marker_type.to_string(),
            text: String::new(),
            author: author.map(|s| s.to_string()),
            date: None,
            code_context: None,
            suggested_fix: None,
            auto_fix_available: None,
        }
    }

    #[test]
    fn test_sort_items_default() {
        let items = vec![
            make_item("b.rs", 5, "todo", None),
            make_item("a.rs", 10, "fixme", None),
        ];
        let sorted = sort_items(items, "age");
        // Default/unknown sort preserves original order
        assert_eq!(sorted[0].file, "b.rs");
        assert_eq!(sorted[1].file, "a.rs");
    }

    #[test]
    fn test_sort_items_by_file() {
        let items = vec![
            make_item("b.rs", 5, "todo", None),
            make_item("a.rs", 10, "fixme", None),
            make_item("a.rs", 1, "hack", None),
        ];
        let sorted = sort_items(items, "file");
        assert_eq!(sorted[0].file, "a.rs");
        assert_eq!(sorted[0].line, 1);
        assert_eq!(sorted[1].file, "a.rs");
        assert_eq!(sorted[1].line, 10);
        assert_eq!(sorted[2].file, "b.rs");
    }

    #[test]
    fn test_sort_items_by_type() {
        let items = vec![
            make_item("a.rs", 1, "todo", None),
            make_item("b.rs", 2, "fixme", None),
            make_item("c.rs", 3, "hack", None),
        ];
        let sorted = sort_items(items, "type");
        assert_eq!(sorted[0].marker_type, "fixme");
        assert_eq!(sorted[1].marker_type, "hack");
        assert_eq!(sorted[2].marker_type, "todo");
    }

    #[test]
    fn test_sort_items_by_author() {
        let items = vec![
            make_item("b.rs", 2, "todo", Some("bob")),
            make_item("a.rs", 1, "fixme", Some("alice")),
            make_item("c.rs", 3, "hack", None),
        ];
        let sorted = sort_items(items, "author");
        // None sorts before Some("alice") which sorts before Some("bob")
        assert!(sorted[0].author.is_none());
        assert_eq!(sorted[1].author.as_deref(), Some("alice"));
        assert_eq!(sorted[2].author.as_deref(), Some("bob"));
    }

    #[test]
    fn test_sort_items_empty() {
        let sorted = sort_items(vec![], "file");
        assert!(sorted.is_empty());
    }

    #[test]
    fn test_sort_items_single() {
        let items = vec![make_item("a.rs", 1, "todo", None)];
        let sorted = sort_items(items, "file");
        assert_eq!(sorted.len(), 1);
    }

    #[test]
    fn test_sort_items_unknown_sort_key() {
        let items = vec![
            make_item("b.rs", 2, "todo", None),
            make_item("a.rs", 1, "fixme", None),
        ];
        // Unknown sort key should preserve original order
        let sorted = sort_items(items, "unknown");
        assert_eq!(sorted[0].file, "b.rs");
        assert_eq!(sorted[1].file, "a.rs");
    }

    // ── get_suggested_fix ──

    #[test]
    fn test_get_suggested_fix_todo() {
        let fix = get_suggested_fix("todo");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("issue"));
    }

    #[test]
    fn test_get_suggested_fix_fixme() {
        let fix = get_suggested_fix("fixme");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("bug"));
    }

    #[test]
    fn test_get_suggested_fix_hack() {
        let fix = get_suggested_fix("hack");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("workaround"));
    }

    #[test]
    fn test_get_suggested_fix_xxx() {
        let fix = get_suggested_fix("xxx");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("dangerous"));
    }

    #[test]
    fn test_get_suggested_fix_warn() {
        let fix = get_suggested_fix("warn");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("WARNING"));
    }

    #[test]
    fn test_get_suggested_fix_bug() {
        let fix = get_suggested_fix("bug");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("B"));
    }

    #[test]
    fn test_get_suggested_fix_optimize() {
        let fix = get_suggested_fix("optimize");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("performance"));
    }

    #[test]
    fn test_get_suggested_fix_unknown() {
        let fix = get_suggested_fix("unknown_marker");
        assert!(fix.is_some());
        assert!(fix.unwrap().contains("Review"));
    }

    #[test]
    fn test_get_suggested_fix_all_marker_types_return_some() {
        for (_, marker_type) in MARKERS {
            assert!(get_suggested_fix(marker_type).is_some(), "marker_type '{}' returned None", marker_type);
        }
    }

    // ── scan_source (integration of has_marker + is_marker_in_string + extract_comment_text + get_suggested_fix) ──

    #[test]
    fn test_scan_source_detects_todo() {
        let source = "// TODO: implement this\nfn main() {}\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].marker_type, "todo");
        assert_eq!(items[0].text, "implement this");
        assert_eq!(items[0].line, 1);
    }

    #[test]
    fn test_scan_source_detects_multiple_marker_types() {
        let source = "// TODO: add tests\n// FIXME: fix bug\n// HACK: temporary\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].marker_type, "todo");
        assert_eq!(items[1].marker_type, "fixme");
        assert_eq!(items[2].marker_type, "hack");
    }

    #[test]
    fn test_scan_source_no_markers() {
        let source = "fn main() {\n    let x = 1;\n    println!(\"hello\");\n}\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn test_scan_source_empty_source() {
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", "", &None, &mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn test_scan_source_marker_in_string_skipped() {
        // Marker inside string literal should be skipped
        let source = "let msg = \"TODO: implement this\";\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert!(items.is_empty(), "markers inside string literals should be skipped");
    }

    #[test]
    fn test_scan_source_comment_and_string_mixed() {
        // Both a comment marker and a string marker — comment should still be found
        let source = "// TODO: real task\nlet msg = \"TODO: not real\";\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 1);
        assert_eq!(items[0].text, "real task");
    }

    #[test]
    fn test_scan_source_with_marker_filter() {
        let source = "// TODO: first\n// FIXME: second\n// HACK: third\n";
        let filter = Some(vec!["fixme".to_string()]);
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &filter, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].marker_type, "fixme");
        assert_eq!(items[0].text, "second");
    }

    #[test]
    fn test_scan_source_with_marker_filter_empty() {
        let source = "// TODO: first\n";
        // Empty filter matches nothing
        let filter = Some(Vec::new());
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &filter, &mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn test_scan_source_multiple_lines_same_marker() {
        let source = "// TODO: first\n// normal line\n// TODO: second\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].line, 1);
        assert_eq!(items[0].text, "first");
        assert_eq!(items[1].line, 3);
        assert_eq!(items[1].text, "second");
    }

    #[test]
    fn test_scan_source_xxx_marker() {
        let source = "// XXX: dangerous code here\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].marker_type, "xxx");
    }

    #[test]
    fn test_scan_source_only_first_marker_per_line_counts() {
        // Only the first matching marker should be counted per line
        let source = "// TODO: first; FIXME: second\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].marker_type, "todo");
    }

    #[test]
    fn test_scan_source_suggested_fix_is_set() {
        let source = "// TODO: do something\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert!(items[0].suggested_fix.is_some());
        assert!(items[0].suggested_fix.as_deref().unwrap().contains("issue"));
    }

    #[test]
    fn test_scan_source_auto_fix_available_is_false() {
        let source = "// TODO: do something\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items[0].auto_fix_available, Some(false));
    }

    #[test]
    fn test_scan_source_marker_with_parenthesis_counts() {
        let source = "// FIXME(security): sanitize input\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].marker_type, "fixme");
    }

    #[test]
    fn test_scan_source_line_numbers_are_one_based() {
        let source = "line1\nline2\n// TODO: on line 3\nline4\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].line, 3);
    }

    #[test]
    fn test_scan_source_handles_all_marker_types() {
        // Build source with all 7 marker types
        let mut source = String::new();
        for (name, marker_type) in MARKERS {
            source.push_str(&format!("// {}: testing {}\n", name, marker_type));
        }
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", &source, &None, &mut items);
        assert_eq!(items.len(), MARKERS.len());
        // Verify each marker type appears exactly once
        let types: Vec<&str> = items.iter().map(|i| i.marker_type.as_str()).collect();
        for (_, marker_type) in MARKERS {
            assert!(types.contains(marker_type), "marker_type '{}' not found in results", marker_type);
        }
    }

    #[test]
    fn test_scan_source_no_false_positives_for_similar_words() {
        let source = "// TODOList: not a real todo\n// TODOLATER: ignore this\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        // These shouldn't match because they don't have the marker suffix patterns.
        assert!(items.is_empty());
    }

    #[test]
    fn test_scan_source_todo_without_suffix_does_not_match() {
        let source = "// TODO\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        // Plain "TODO" without :, (, or space should not match
        assert!(items.is_empty());
    }

    #[test]
    fn test_scan_source_code_context_is_none() {
        let source = "// TODO: add context\n";
        let mut items = Vec::new();
        scan_source("/tmp/test.rs", source, &None, &mut items);
        assert_eq!(items.len(), 1);
        assert!(items[0].code_context.is_none());
    }

    #[test]
    fn test_scan_source_file_path_preserved() {
        let source = "// TODO: test\n";
        let mut items = Vec::new();
        scan_source("/my/custom/path/test.rs", source, &None, &mut items);
        assert_eq!(items[0].file, "/my/custom/path/test.rs");
    }

    // ── MARKERS const correctness ──

    #[test]
    fn test_markers_are_unique() {
        let names: Vec<&str> = MARKERS.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "MARKERS contains duplicate names");

        let types: Vec<&str> = MARKERS.iter().map(|(_, t)| *t).collect();
        let mut sorted_types = types.clone();
        sorted_types.sort();
        sorted_types.dedup();
        assert_eq!(types.len(), sorted_types.len(), "MARKERS contains duplicate types");
    }

    #[test]
    fn test_markers_all_have_non_empty_names_and_types() {
        for (name, marker_type) in MARKERS {
            assert!(!name.is_empty(), "MARKERS contains empty name");
            assert!(!marker_type.is_empty(), "MARKERS contains empty type");
            assert_eq!(name.to_lowercase(), *marker_type, "MARKERS entry '{}' has mismatched type '{}'", name, marker_type);
        }
    }
}
