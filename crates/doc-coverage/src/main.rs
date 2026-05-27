#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;

use ast_parse_ts::{parse_doc_coverage_items_file, Language};
use cogent_common::{find_source_files, print_table_header, print_table_row, separator, Column};

#[derive(Parser)]
#[command(
    name = "doccov",
    about = "Documentation coverage -- measure public API doc comment percentage"
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

    /// Minimum coverage threshold (exit code 1 if below)
    #[arg(long)]
    min: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct DocItem {
    kind: String, // fn, struct, enum, trait, impl_fn
    name: String,
    public: bool,
    documented: bool,
    file: String,
    line: usize,
    /// Code context (surrounding lines) for the item
    #[serde(skip_serializing_if = "Option::is_none")]
    code_context: Option<String>,
    /// Suggested fix for documentation issues
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Serialize)]
struct DocReport {
    items: Vec<DocItem>,
    summary: DocSummary,
}

#[derive(Serialize)]
struct DocSummary {
    total_public: usize,
    documented: usize,
    undocumented: usize,
    coverage_pct: f64,
    by_kind: KindBreakdown,
}

#[derive(Serialize)]
struct KindBreakdown {
    functions: (usize, usize), // (total_public, documented)
    structs: (usize, usize),
    enums: (usize, usize),
    traits: (usize, usize),
    impl_fns: (usize, usize),
}

const ALL_EXTS: &[&str] = &[
    "rs", "py", "pyi", "js", "mjs", "ts", "tsx", "go", "c", "h", "cpp", "cc", "cxx", "hpp", "cs",
    "java", "php", "rb", "swift",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let all_files = find_source_files(&cli.path, cli.recursive, ALL_EXTS);
    if all_files.is_empty() {
        return Err(format!("No supported source files found at {}", cli.path).into());
    }

    let mut all_items: Vec<DocItem> = Vec::new();

    for file_path in &all_files {
        let lang = Language::from_extension(file_path);
        if lang == Language::Unknown {
            continue;
        }
        let (stats, items) = parse_doc_coverage_items_file(file_path);
        for info in items {
            all_items.push(DocItem {
                kind: info.kind,
                name: info.name,
                public: true,
                documented: info.documented,
                file: file_path.clone(),
                line: info.line,
                code_context: None,
                suggested_fix: None,
                auto_fix_available: None,
            });
        }
        // If parser returned zero items but stats say there are some, fall back to generic items
        if all_items.iter().filter(|i| i.file == *file_path).count() < stats.total_public {
            for _ in 0..stats.total_public {
                all_items.push(DocItem {
                    kind: lang.to_string(),
                    name: String::new(),
                    public: true,
                    documented: false,
                    file: file_path.clone(),
                    line: 0,
                    code_context: None,
                    suggested_fix: None,
                    auto_fix_available: None,
                });
            }
            for item in all_items
                .iter_mut()
                .filter(|i| i.file == *file_path && i.name.is_empty())
                .take(stats.documented)
            {
                item.documented = true;
            }
        }
    }

    match cli.format.as_str() {
        "json" => output_json(&all_items),
        "ndjson" => output_ndjson(&all_items),
        _ => {
            let coverage = output_table(&all_items);
            if let Some(min) = cli.min {
                if coverage < min {
                    eprintln!(
                        "\nFAILED: Coverage {:.0}% is below minimum {:.0}%",
                        coverage, min
                    );
                    return Err(
                        format!("Coverage {:.0}% is below minimum {:.0}%", coverage, min).into(),
                    );
                }
            }
            Ok(())
        }
    }
}

fn output_table(items: &[DocItem]) -> f64 {
    let public_items: Vec<_> = items.iter().filter(|i| i.public).collect();
    let documented = public_items.iter().filter(|i| i.documented).count();
    let total = public_items.len();
    let coverage = if total > 0 {
        documented as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    // By kind breakdown
    let count_kind = |kind: &str| -> (usize, usize) {
        let items: Vec<_> = public_items.iter().filter(|i| i.kind == kind).collect();
        let doc = items.iter().filter(|i| i.documented).count();
        (items.len(), doc)
    };

    let fns = count_kind("fn");
    let structs = count_kind("struct");
    let enums = count_kind("enum");
    let traits = count_kind("trait");
    let impl_fns = count_kind("impl_fn");

    // Undocumented items
    let undocumented: Vec<_> = public_items.iter().filter(|i| !i.documented).collect();

    println!("DOCUMENTATION COVERAGE");
    println!("{}", separator(70));
    println!();
    println!("  Public items:     {}", total);
    println!("  Documented:       {} ({:.0}%)", documented, coverage);
    println!("  Undocumented:     {}", total - documented);
    println!();
    println!("  By kind:");
    println!("    Functions:      {}/{} ({:.0}%)", fns.1, fns.0, pct(fns));
    println!(
        "    Structs:        {}/{} ({:.0}%)",
        structs.1,
        structs.0,
        pct(structs)
    );
    println!(
        "    Enums:          {}/{} ({:.0}%)",
        enums.1,
        enums.0,
        pct(enums)
    );
    println!(
        "    Traits:         {}/{} ({:.0}%)",
        traits.1,
        traits.0,
        pct(traits)
    );
    println!(
        "    Impl fns:       {}/{} ({:.0}%)",
        impl_fns.1,
        impl_fns.0,
        pct(impl_fns)
    );

    if !undocumented.is_empty() {
        println!();
        println!("  UNDOCUMENTED PUBLIC ITEMS:");

        let columns = [
            Column::left("KIND", 10),
            Column::left("NAME", 30),
            Column::left("FILE", 30),
            Column::right("LINE", 5),
        ];
        print_table_header(&columns);

        for item in undocumented.iter().take(20) {
            let kind_str = format!("{} {}", icon(&item.kind), item.kind);
            let line_str = item.line.to_string();
            print_table_row(&columns, &[&kind_str, &item.name, &item.file, &line_str]);
        }
        if undocumented.len() > 20 {
            println!("    ... and {} more", undocumented.len() - 20);
        }
    }

    println!();
    let verdict = if coverage >= 90.0 {
        "Excellent"
    } else if coverage >= 70.0 {
        "Good"
    } else if coverage >= 50.0 {
        "Needs work"
    } else {
        "Poor"
    };
    println!("  Coverage: {:.0}% -- {}", coverage, verdict);

    coverage
}

fn pct((doc, total): (usize, usize)) -> f64 {
    if total > 0 {
        doc as f64 / total as f64 * 100.0
    } else {
        0.0
    }
}

fn icon(kind: &str) -> &'static str {
    match kind {
        "fn" => "fn",
        "struct" => "▓",
        "enum" => "◇",
        "trait" => "△",
        "impl_fn" => "→",
        _ => "?",
    }
}

fn output_json(items: &[DocItem]) -> Result<(), Box<dyn std::error::Error>> {
    let public_items: Vec<_> = items.iter().filter(|i| i.public).collect();
    let documented = public_items.iter().filter(|i| i.documented).count();
    let total = public_items.len();
    let coverage = if total > 0 {
        documented as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    let count_kind = |kind: &str| -> (usize, usize) {
        let items: Vec<_> = public_items.iter().filter(|i| i.kind == kind).collect();
        let doc = items.iter().filter(|i| i.documented).count();
        (items.len(), doc)
    };

    let report = DocReport {
        items: items.to_vec(),
        summary: DocSummary {
            total_public: total,
            documented,
            undocumented: total - documented,
            coverage_pct: coverage,
            by_kind: KindBreakdown {
                functions: count_kind("fn"),
                structs: count_kind("struct"),
                enums: count_kind("enum"),
                traits: count_kind("trait"),
                impl_fns: count_kind("impl_fn"),
            },
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(items: &[DocItem]) -> Result<(), Box<dyn std::error::Error>> {
    for item in items {
        println!("{}", serde_json::to_string(item)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ast_parse_ts::Language;

    #[test]
    fn test_python_doc_coverage() {
        let source = r#"
def documented_func():
    """This has a docstring."""
    pass

def undocumented_func():
    pass
"#;
        let stats = ast_parse_ts::parse_doc_coverage(source, Language::Python);
        assert_eq!(stats.total_public, 2, "Should find 2 Python functions");
        assert_eq!(stats.documented, 1, "Only 1 should have docstring");
    }

    #[test]
    fn test_js_doc_coverage() {
        let source = r#"
/** Documented function */
function documentedFunc() {
    return 1;
}

function undocumentedFunc() {
    return 2;
}
"#;
        let stats = ast_parse_ts::parse_doc_coverage(source, Language::JavaScript);
        assert_eq!(stats.total_public, 2, "Should find 2 JS functions");
        assert_eq!(stats.documented, 1, "Only 1 should have JSDoc");
    }

    #[test]
    fn test_go_doc_coverage() {
        let source = r#"
// DocumentedFunc does something.
func DocumentedFunc() int {
    return 1
}

func UndocumentedFunc() int {
    return 2
}
"#;
        let stats = ast_parse_ts::parse_doc_coverage(source, Language::Go);
        assert_eq!(stats.total_public, 2, "Should find 2 Go functions");
        assert_eq!(stats.documented, 1, "Only 1 should have Go doc comment");
    }
}
