#![deny(clippy::all)]

use ast_parse_ts::parse_imports_file;
use clap::Parser;
use cogent_common::find_source_files;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "coupling",
    about = "Coupling analysis -- module dependency graphs, fan-in/fan-out"
)]
struct Cli {
    /// Path to scan (directory with src/)
    path: String,

    /// Output format: table (default), json, dot (Graphviz), or ndjson
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Show only tightly coupled modules (fan-in + fan-out > threshold)
    #[arg(long, default_value = "0")]
    min_coupling: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ModuleInfo {
    name: String,
    imports: Vec<String>,       // modules this one depends on
    imported_by: Vec<String>,   // modules that depend on this one
    fan_out: usize,             // how many modules I depend on
    fan_in: usize,              // how many modules depend on me
    instability: f64,           // fan_out / (fan_in + fan_out)
    implicit_deps: Vec<String>, // detected module references without explicit use statements
    /// Suggested fix for high coupling
    #[serde(skip_serializing_if = "Option::is_none")]
    suggested_fix: Option<String>,
    /// Whether an auto-fix is available
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_fix_available: Option<bool>,
}

#[derive(Serialize)]
struct CouplingReport {
    modules: Vec<ModuleInfo>,
    summary: CouplingSummary,
}

#[derive(Serialize)]
struct CouplingSummary {
    total_modules: usize,
    total_dependencies: usize,
    avg_fan_in: f64,
    avg_fan_out: f64,
    most_coupled: Vec<String>,
    modules_with_implicit: Vec<String>,
}

fn resolve_src_path(path: &str) -> PathBuf {
    let src_dir = Path::new(path);
    if src_dir.join("src").is_dir() {
        src_dir.join("src")
    } else if src_dir.is_dir() {
        src_dir.to_path_buf()
    } else {
        eprintln!("No source directory found at {}", path);
        std::process::exit(1);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let src_path = resolve_src_path(&cli.path);

    // Scan ALL source files via tree-sitter (language-agnostic, bounded parallelism)
    let dependencies = scan_all_imports(&src_path);

    // Build module info (no implicit refs in language-agnostic mode)
    let empty_implicit: HashMap<String, HashSet<String>> = HashMap::new();
    let modules = build_module_info(&dependencies, &empty_implicit);

    let filtered: Vec<_> = if cli.min_coupling > 0 {
        modules
            .into_iter()
            .filter(|m| (m.fan_in + m.fan_out) >= cli.min_coupling)
            .collect()
    } else {
        modules
    };

    match cli.format.as_str() {
        "json" => output_json(&filtered),
        "dot" => {
            output_dot(&filtered);
            Ok(())
        }
        "ndjson" => {
            let _ = output_ndjson(&filtered);
            Ok(())
        }
        _ => {
            output_table(&filtered);
            Ok(())
        }
    }
}

/// Scan all source files via tree-sitter and build dependency graph (parallel).
fn scan_all_imports(dir: &Path) -> HashMap<String, HashSet<String>> {
    const EXTS: &[&str] = &[
        "rs", "py", "pyi", "js", "mjs", "cjs", "ts", "tsx", "mts", "go", "c", "h", "cpp", "cc",
        "cxx", "hpp", "cs", "java", "php", "rb", "swift",
    ];
    let files = find_source_files(dir.to_str().unwrap_or(""), true, EXTS);
    // Single-threaded rayon to reduce memory pressure (prevents OOM on 16GB/32GB systems)
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    pool.install(|| {
        files
            .par_iter()
            .filter_map(|file_str| {
                let import_info = parse_imports_file(file_str);
                let is_rust = file_str.ends_with(".rs");
                let targets: HashSet<String> = import_info
                    .into_iter()
                    .map(|i| i.imported_module)
                    .filter(|m| !m.is_empty())
                    .filter(|m| is_workspace_import(m, is_rust))
                    .collect();
                if targets.is_empty() {
                    None
                } else {
                    Some((file_str.clone(), targets))
                }
            })
            .collect()
    })
}

/// Keep only imports that likely reference workspace-local modules.
fn is_workspace_import(module: &str, is_rust: bool) -> bool {
    if module.ends_with("::*") {
        return false; // wildcard imports are not specific module deps
    }
    if !is_rust {
        return true; // other languages have less ambiguity
    }
    // Rust: keep crate-local paths; skip external crates like clap::Parser, serde::Serialize
    module.starts_with("crate::")
        || module.starts_with("self::")
        || module.starts_with("super::")
        || !module.contains("::")
}

fn build_module_info(
    dependencies: &HashMap<String, HashSet<String>>,
    implicit_refs: &HashMap<String, HashSet<String>>,
) -> Vec<ModuleInfo> {
    // Build reverse dependency map
    let mut reverse: HashMap<String, HashSet<String>> = HashMap::new();
    for (module, deps) in dependencies {
        for dep in deps {
            reverse
                .entry(dep.clone())
                .or_default()
                .insert(module.clone());
        }
    }

    let mut all_modules: HashSet<String> = dependencies.keys().cloned().collect();
    all_modules.extend(reverse.keys().cloned());

    let mut modules: Vec<ModuleInfo> = all_modules
        .into_iter()
        .map(|module| {
            let imports: Vec<String> = dependencies
                .get(&module)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let imported_by: Vec<String> = reverse
                .get(&module)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let fan_out = imports.len();
            let fan_in = imported_by.len();
            let total = fan_in + fan_out;
            let instability = if total > 0 {
                fan_out as f64 / total as f64
            } else {
                0.0
            };
            let implicit = implicit_refs
                .get(&module)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();

            ModuleInfo {
                name: module,
                imports,
                imported_by,
                fan_out,
                fan_in,
                instability,
                implicit_deps: implicit,
                suggested_fix: None,
                auto_fix_available: None,
            }
        })
        .collect();

    modules.sort_by_key(|b| std::cmp::Reverse(b.fan_in + b.fan_out));
    modules
}

fn output_table(modules: &[ModuleInfo]) {
    if modules.is_empty() {
        println!("No modules found.");
        return;
    }

    println!("MODULE COUPLING ANALYSIS");
    println!("{}", "─".repeat(80));
    println!(
        "\n{:<40} {:>8} {:>8} {:>12} STATUS",
        "MODULE", "FAN-IN", "FAN-OUT", "INSTABILITY"
    );
    println!("{}", "─".repeat(80));

    let total_fan_in: usize = modules.iter().map(|m| m.fan_in).sum();
    let total_fan_out: usize = modules.iter().map(|m| m.fan_out).sum();

    for m in modules {
        let total = m.fan_in + m.fan_out;
        let status = if total > 10 {
            "⚠ high"
        } else if total > 5 {
            "○ moderate"
        } else {
            "✓ low"
        };

        println!(
            "{:<40} {:>8} {:>8} {:>11.2} {}",
            truncate(&m.name, 38),
            m.fan_in,
            m.fan_out,
            m.instability,
            status,
        );
    }

    println!("{}", "─".repeat(80));
    println!();
    println!("  Total modules:       {}", modules.len());
    println!(
        "  Total dependencies:  {}",
        modules.iter().map(|m| m.fan_out).sum::<usize>()
    );
    println!(
        "  Avg fan-in:          {:.1}",
        total_fan_in as f64 / modules.len() as f64
    );
    println!(
        "  Avg fan-out:         {:.1}",
        total_fan_out as f64 / modules.len() as f64
    );

    // Most coupled
    let coupled: Vec<_> = modules
        .iter()
        .filter(|m| m.fan_in + m.fan_out > 5)
        .collect();

    if !coupled.is_empty() {
        println!();
        println!("  MOST COUPLED:");
        for m in coupled.iter().take(5) {
            println!(
                "    {} (fan-in: {}, fan-out: {})",
                m.name, m.fan_in, m.fan_out
            );
        }
    }

    // Implicit dependencies
    let with_implicit: Vec<_> = modules
        .iter()
        .filter(|m| !m.implicit_deps.is_empty())
        .collect();

    if !with_implicit.is_empty() {
        println!();
        println!("  IMPLICIT DEPENDENCIES (no explicit use statement):");
        for m in with_implicit.iter().take(5) {
            println!(
                "    {} has {} implicit reference(s): {}",
                m.name,
                m.implicit_deps.len(),
                m.implicit_deps.join(", ")
            );
        }
        let total_implicit: usize = with_implicit.iter().map(|m| m.implicit_deps.len()).sum();
        println!();
        println!(
            "    {} modules have {} total implicit dependencies.",
            with_implicit.len(),
            total_implicit
        );
        println!("    Consider adding explicit use statements for clarity.");
    }
}

fn output_json(modules: &[ModuleInfo]) -> Result<(), Box<dyn std::error::Error>> {
    let total_fan_in: usize = modules.iter().map(|m| m.fan_in).sum();
    let total_fan_out: usize = modules.iter().map(|m| m.fan_out).sum();
    let n = modules.len();

    let most_coupled: Vec<String> = modules
        .iter()
        .filter(|m| m.fan_in + m.fan_out > 5)
        .map(|m| m.name.clone())
        .collect();

    let modules_with_implicit: Vec<String> = modules
        .iter()
        .filter(|m| !m.implicit_deps.is_empty())
        .map(|m| m.name.clone())
        .collect();

    let report = CouplingReport {
        modules: modules.to_vec(),
        summary: CouplingSummary {
            total_modules: n,
            total_dependencies: modules.iter().map(|m| m.fan_out).sum(),
            avg_fan_in: if n > 0 {
                total_fan_in as f64 / n as f64
            } else {
                0.0
            },
            avg_fan_out: if n > 0 {
                total_fan_out as f64 / n as f64
            } else {
                0.0
            },
            most_coupled,
            modules_with_implicit,
        },
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn output_ndjson(modules: &[ModuleInfo]) -> Result<(), Box<dyn std::error::Error>> {
    for module in modules {
        println!("{}", serde_json::to_string(module)?);
    }
    Ok(())
}

fn output_dot(modules: &[ModuleInfo]) {
    println!("digraph coupling {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=filled, fillcolor=lightblue];");
    println!();

    for m in modules {
        let short_name = m.name.split("::").last().unwrap_or(&m.name);
        for dep in &m.imports {
            let dep_short = dep.split("::").last().unwrap_or(dep);
            println!("  \"{}\" -> \"{}\";", short_name, dep_short);
        }
    }

    println!("}}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("…{}", &s[s.len() - max + 1..])
    }
}
