#![deny(clippy::all)]

use clap::Parser;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use cogent_common::{print_table_header, print_table_row, separator, wrap_tool_response, Column};

mod delta;

#[derive(Parser)]
#[command(
    name = "mutate",
    about = "Mutation testing — evaluate test suite quality by introducing deliberate code changes"
)]
struct Cli {
    /// Path to the crate root (directory with Cargo.toml)
    path: String,

    /// Only test specific files (comma-separated)
    #[arg(long)]
    files: Option<String>,

    /// Package name to test (required for workspace crates; auto-detected for single crates)
    #[arg(short = 'p', long)]
    package: Option<String>,

    /// Maximum mutants to test (default: 5, ceiling: 50)
    #[arg(short = 'n', long, default_value = "5")]
    max_mutants: usize,

    /// Timeout per test run in seconds (enforced via watchdog kill)
    #[arg(short, long, default_value = "30")]
    timeout: u64,

    /// Use cargo-nextest instead of cargo test (3x faster, better memory isolation)
    #[arg(long)]
    nextest: bool,

    /// Output format: table (default) or json
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Pass environment variable to cargo (KEY=VALUE)
    #[arg(long)]
    env: Vec<String>,

    /// Mutation strategies to use: all, standard, bitwise, arithmetic
    #[arg(long, default_value = "all")]
    strategy: String,

    /// Enable delta mutation testing: only mutate functions changed since base ref
    #[arg(long)]
    delta: bool,

    /// Git ref (branch, tag, or commit) to diff against for delta mode (default: HEAD~1)
    #[arg(long, default_value = "HEAD~1")]
    base_ref: String,
}

/// A single mutation applied to source code
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Mutant {
    id: usize,
    file: String,
    line: usize,
    description: String,
    original: String,
    mutated: String,
    category: String, // "standard", "bitwise", "arithmetic", "boundary"
}

/// Result of testing a single mutant
#[derive(Debug, Clone, Serialize)]
struct MutantResult {
    id: usize,
    file: String,
    line: usize,
    description: String,
    status: String, // "killed", "survived", "timeout", "error"
    test_output: String,
}

#[derive(Serialize)]
struct MutationReport {
    results: Vec<MutantResult>,
    summary: MutationSummary,
}

#[derive(Serialize)]
struct MutationSummary {
    total_mutants: usize,
    killed: usize,
    survived: usize,
    timeout: usize,
    error: usize,
    mutation_score: f64,
}

fn analyze_non_rust_file(path: &str, _cli: &Cli) -> Result<(), String> {
    println!("MUTATION ANALYSIS (analysis mode - no test execution)");
    println!("Note: Full mutation testing with test execution is Rust-only.");
    println!("For Ruby/Swift/other languages: Use language-specific mutation frameworks.");
    println!();
    println!("To run full mutation tests:");
    println!("  Ruby: mutant-rs, mutant, rspec-mocks");
    println!("  Swift: SwiftMutator");
    println!("  Python: cosmic-ray, mutmut");
    println!();

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return Err(format!("Failed to read {}: {}", path, e)),
    };

    // Determine language
    let lang = if path.ends_with(".rs") {
        "Rust"
    } else if path.ends_with(".rb") {
        "Ruby"
    } else if path.ends_with(".swift") {
        "Swift"
    } else if path.ends_with(".py") {
        "Python"
    } else if path.ends_with(".js") || path.ends_with(".ts") {
        "JavaScript/TypeScript"
    } else if path.ends_with(".go") {
        "Go"
    } else if path.ends_with(".c") || path.ends_with(".h") {
        "C"
    } else if path.ends_with(".cpp") || path.ends_with(".cc") || path.ends_with(".cxx") {
        "C++"
    } else if path.ends_with(".cs") {
        "C#"
    } else if path.ends_with(".java") {
        "Java"
    } else if path.ends_with(".php") {
        "PHP"
    } else if path.ends_with(".kt") || path.ends_with(".kts") {
        "Kotlin"
    } else {
        "Unknown"
    };

    println!("Language: {}", lang);
    println!("File: {}", path);
    println!();

    // Count potential mutation points (simplified analysis)
    let potential_mutations = count_potential_mutations(&source, lang);

    println!("Analysis complete:");
    println!("  Potential mutation points: {}", potential_mutations);
    println!(
        "  Estimated test coverage needed: {}-{}%",
        potential_mutations * 2,
        potential_mutations * 3
    );
    println!();
    println!("Use language-specific mutation tools for actual mutation testing:");
    if lang == "Ruby" {
        println!("  $ gem install mutant-rs");
        println!("  $ mutant path/to/file.rb");
    } else if lang == "Swift" {
        println!("  # Swift mutation tools require manual setup");
        println!("  # Consider using SwiftMutator or SwiftCheck");
    } else if lang == "Python" {
        println!("  $ pip install cosmic-ray");
        println!("  $ cosmic-ray run --test-runner pytest path/to/file.py");
    } else if lang == "C" || lang == "C++" {
        println!("  # C/C++ mutation testing");
        println!("  $ cargo install mull");
        println!("  $ mull-cpp -mutators=all path/to/file.cpp");
    } else if lang == "C#" {
        println!("  # C# mutation testing");
        println!("  $ dotnet tool install --global dotnet-mutator");
        println!("  $ dotnet-mutator run path/to/File.cs");
    } else if lang == "Java" {
        println!("  # Java mutation testing");
        println!("  $ mvn org.pitest:pitest-maven:calculate-coverage");
        println!("  $ mvn org.pitest:pitest-maven:mutationCoverage path/to/file.java");
    } else if lang == "PHP" {
        println!("  # PHP mutation testing");
        println!("  $ composer require --dev infection/infection");
        println!("  $ vendor/bin/infection path/to/file.php");
    } else if lang == "JavaScript/TypeScript" {
        println!("  $ npm install -g stryker-mutator-core");
        println!("  $ npx stryker run path/to/file.js");
    } else if lang == "Go" {
        println!("  $ go install github.com/zimmsja/go-mutesting@latest");
        println!("  $ go-mutesting ./path/to/file.go");
    }

    Ok(())
}

fn count_potential_mutations(source: &str, lang: &str) -> usize {
    let mut count = 0;

    match lang {
        "Ruby" => {
            // Count operators that could be mutated
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("while ") {
                count += source.matches("while ").count();
            }
        }
        "Swift" => {
            // Count Swift operators
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("while ") {
                count += source.matches("while ").count();
            }
            if source.contains("switch ") {
                count += source.matches("switch ").count();
            }
            if source.contains("guard ") {
                count += source.matches("guard ").count();
            }
        }
        "Python" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("and ") {
                count += source.matches("and ").count();
            }
            if source.contains("or ") {
                count += source.matches("or ").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("while ") {
                count += source.matches("while ").count();
            }
        }
        "C" | "C++" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("while ") {
                count += source.matches("while ").count();
            }
            if source.contains("switch ") {
                count += source.matches("switch ").count();
            }
            if source.contains("case ") {
                count += source.matches("case ").count();
            }
        }
        "C#" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if") {
                count += source.matches("if").count();
            }
            if source.contains("for") {
                count += source.matches("for").count();
            }
            if source.contains("while") {
                count += source.matches("while").count();
            }
            if source.contains("switch") {
                count += source.matches("switch").count();
            }
            if source.contains("try") {
                count += source.matches("try").count();
            }
        }
        "Java" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if") {
                count += source.matches("if").count();
            }
            if source.contains("for") {
                count += source.matches("for").count();
            }
            if source.contains("while") {
                count += source.matches("while").count();
            }
            if source.contains("switch") {
                count += source.matches("switch").count();
            }
            if source.contains("case") {
                count += source.matches("case").count();
            }
            if source.contains("try") {
                count += source.matches("try").count();
            }
            if source.contains("catch") {
                count += source.matches("catch").count();
            }
        }
        "PHP" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("while ") {
                count += source.matches("while ").count();
            }
            if source.contains("switch ") {
                count += source.matches("switch ").count();
            }
            if source.contains("case ") {
                count += source.matches("case ").count();
            }
            if source.contains("foreach") {
                count += source.matches("foreach").count();
            }
        }
        "Go" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if ") {
                count += source.matches("if ").count();
            }
            if source.contains("for ") {
                count += source.matches("for ").count();
            }
            if source.contains("switch ") {
                count += source.matches("switch ").count();
            }
            if source.contains("case ") {
                count += source.matches("case ").count();
            }
            if source.contains("select ") {
                count += source.matches("select ").count();
            }
        }
        "JavaScript/TypeScript" => {
            if source.contains("==") {
                count += source.matches("==").count();
            }
            if source.contains("!=") {
                count += source.matches("!=").count();
            }
            if source.contains("&&") {
                count += source.matches("&&").count();
            }
            if source.contains("||") {
                count += source.matches("||").count();
            }
            if source.contains("if") {
                count += source.matches("if").count();
            }
            if source.contains("for") {
                count += source.matches("for").count();
            }
            if source.contains("while") {
                count += source.matches("while").count();
            }
            if source.contains("switch") {
                count += source.matches("switch").count();
            }
            if source.contains("case") {
                count += source.matches("case").count();
            }
            if source.contains("try") {
                count += source.matches("try").count();
            }
            if source.contains("catch") {
                count += source.matches("catch").count();
            }
        }
        _ => {}
    }

    count
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn run(cli: Cli) -> Result<(), String> {
    let start = std::time::Instant::now();
    let crate_root_raw = Path::new(&cli.path);

    let crate_root_buf = crate_root_raw
        .canonicalize()
        .map_err(|e| format!("Cannot resolve path {}: {}", cli.path, e))?;
    let crate_root = crate_root_buf.as_path();

    // Check if this is a Rust crate or other language
    let is_rust_crate = crate_root.join("Cargo.toml").exists();
    if !is_rust_crate {
        // For non-Rust files, provide mutation analysis without test execution
        if crate_root_raw.is_file() {
            return analyze_non_rust_file(&cli.path, &cli);
        } else {
            return Err("Mutation test execution requires Rust crate (Cargo.toml). For other languages, pass individual files for mutation analysis only.".to_string());
        }
    }

    // Determine package name: use CLI flag, or auto-detect from Cargo.toml
    let package_name = if let Some(ref pkg) = cli.package {
        pkg.clone()
    } else {
        // Try to find package from [package] section, or first member from [workspace]
        find_package_name(crate_root)
            .or_else(|_| find_first_workspace_member(crate_root))
            .map_err(|e| {
                format!("Could not auto-detect package name. Use -p/--package flag or pass a crate with [package] name. Error: {}", e)
            })?
    };

    // Hard ceiling to prevent runaway test sessions
    let max_mutants = cli.max_mutants.min(50);
    if cli.max_mutants > 50 {
        eprintln!("Warning: --max-mutants capped at 50 to prevent system overload.");
    }

    // Verify tests pass in the ORIGINAL crate first (uses existing build cache)
    verify_tests_pass(crate_root, &package_name, cli.timeout)?;

    // Build the scratch directory once; all mutations run there
    let scratch = ScratchCrate::new(crate_root)?;
    eprintln!("Scratch dir: {}", scratch.root.display());

    let source_files = find_source_files(crate_root, &package_name, &cli.files);
    if source_files.is_empty() {
        return Err("No source files found to mutate.".to_string());
    }

    // Delta mutation testing: compute affected functions from git diff
    let delta_analysis = if cli.delta {
        println!(
            "Computing delta mutation analysis against {}...",
            cli.base_ref
        );
        let loaded_files: Vec<(String, String)> = source_files
            .iter()
            .filter_map(|f| {
                let s = std::fs::read_to_string(f).ok()?;
                Some((f.to_string_lossy().to_string(), s))
            })
            .collect();

        let analysis =
            delta::run_delta_analysis(crate_root, &cli.base_ref, &loaded_files, source_files.len());

        let affected_count: usize = analysis.affected_functions.values().map(|v| v.len()).sum();
        let changed_fn_count: usize = analysis.changed_functions.values().map(|v| v.len()).sum();

        println!("  Changed files:    {}", analysis.changed_files.len());
        println!("  Changed functions: {}", changed_fn_count);
        println!(
            "  Affected by calls: {}",
            affected_count.saturating_sub(changed_fn_count)
        );
        println!(
            "  Reduction:        {:.1}% fewer mutants\n",
            analysis.reduction_pct
        );

        Some(analysis)
    } else {
        println!("Found {} source files to mutate.\n", source_files.len());
        None
    };

    let workspace_root = find_workspace_root(crate_root);

    let mut all_results: Vec<MutantResult> = Vec::new();
    let mut total_mutants = 0usize;
    let mut killed = 0usize;
    let mut survived = 0usize;
    let mut timeouts = 0usize;
    let mut errors = 0usize;

    for file_path in &source_files {
        if total_mutants >= max_mutants {
            break;
        }

        let Ok(source) = std::fs::read_to_string(file_path) else {
            eprintln!("Warning: Could not read {}", file_path.display());
            continue;
        };

        let remaining = max_mutants.saturating_sub(total_mutants);
        let mut file_mutants = generate_mutants_for_file(
            &source,
            &file_path.to_string_lossy(),
            &cli.strategy,
            remaining,
        );

        // In delta mode, filter mutants to only those in affected functions
        if let Some(ref delta) = delta_analysis {
            let file_str = file_path.to_string_lossy().to_string();
            file_mutants.retain(|m| {
                delta::is_line_in_affected_function(
                    &file_str,
                    m.line,
                    &delta.affected_functions,
                    &[(file_str.clone(), source.clone())],
                )
            });
        }

        if file_mutants.is_empty() {
            continue;
        }

        // Assign global IDs
        for (idx, mutant) in file_mutants.iter_mut().enumerate() {
            mutant.id = total_mutants + idx + 1;
        }

        let file_count = file_mutants.len();
        println!(
            "\nTesting {} mutants from {}...",
            file_count,
            file_path.display()
        );

        for (i, mutant) in file_mutants.iter().enumerate() {
            print!(
                "  [{}/{}] mutant {} (line {})... ",
                i + 1,
                file_count,
                mutant.id,
                mutant.line
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();

            let result = test_mutant_isolated(
                mutant,
                crate_root,
                &workspace_root,
                &scratch,
                &package_name,
                cli.timeout,
                cli.nextest,
            );
            match result.status.as_str() {
                "killed" => println!("✓ KILLED"),
                "survived" => println!("✗ SURVIVED"),
                "timeout" => println!("⏱ TIMEOUT"),
                _ => println!(
                    "? ERROR: {}",
                    &result.test_output[..result.test_output.len().min(80)]
                ),
            }
            match result.status.as_str() {
                "killed" => killed += 1,
                "survived" => survived += 1,
                "timeout" => timeouts += 1,
                _ => errors += 1,
            }
            all_results.push(result);
        }

        total_mutants += file_count;
        drop(source);
        drop(file_mutants);
    }

    // scratch dir cleaned up automatically via Drop
    drop(scratch);

    if total_mutants == 0 {
        println!("No mutants to test (--max-mutants 0 or no matching code).");
        return Ok(());
    }

    println!("\nTested {} mutants total.", total_mutants);

    match cli.format.as_str() {
        "json" => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = output_json_streaming(
                &all_results,
                total_mutants,
                killed,
                survived,
                timeouts,
                errors,
                duration_ms,
            );
        }
        _ => output_table_streaming(
            &all_results,
            total_mutants,
            killed,
            survived,
            timeouts,
            errors,
        ),
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────
// ScratchWorkspace: copies the entire workspace into /tmp so:
//   1. Mutations never touch the real source tree.
//   2. Cargo.lock and inter-crate path deps are resolved correctly.
//   3. The cargo registry cache is reused (CARGO_HOME stays the same).
// ──────────────────────────────────────────────────────────────

struct ScratchCrate {
    root: PathBuf,      // workspace root in /tmp
    crate_rel: PathBuf, // relative path from workspace root to the mutated crate
}

impl ScratchCrate {
    /// `workspace_root` is the top-level dir containing Workspace Cargo.toml.
    /// `crate_root` is the specific crate being mutated (may equal workspace_root).
    fn new(crate_root: &Path) -> Result<Self, String> {
        let workspace_root = find_workspace_root(crate_root);
        let crate_rel = crate_root
            .strip_prefix(&workspace_root)
            .map(|p| p.to_path_buf())
            .unwrap_or_default();

        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let scratch_root = std::env::temp_dir().join(format!("mutate-{}", id));

        eprintln!(
            "Copying workspace to scratch: {} -> {}",
            workspace_root.display(),
            scratch_root.display()
        );

        // Copy entire workspace (excluding target/ and .git/)
        copy_dir_recursive_filtered(&workspace_root, &scratch_root)
            .map_err(|e| format!("Cannot copy workspace to scratch: {}", e))?;

        Ok(Self {
            root: scratch_root,
            crate_rel,
        })
    }

    /// The scratch path of the mutated crate (for running cargo test -p <name>).
    fn scratch_crate_root(&self) -> PathBuf {
        self.root.join(&self.crate_rel)
    }

    /// Return the scratch path for a file given its original workspace path.
    fn scratch_path_for(
        &self,
        original_workspace_root: &Path,
        original_file: &Path,
    ) -> Option<PathBuf> {
        let rel = original_file.strip_prefix(original_workspace_root).ok()?;
        Some(self.root.join(rel))
    }
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Walk up from `crate_root` to find the workspace Cargo.toml (the one with [workspace]).
/// Falls back to crate_root itself if none found.
fn find_workspace_root(crate_root: &Path) -> PathBuf {
    let mut dir = crate_root
        .canonicalize()
        .unwrap_or_else(|_| crate_root.to_path_buf());
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return dir;
                }
            }
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return crate_root.to_path_buf(),
        }
    }
}

/// Extract package name from Cargo.toml [package] section.
/// Returns error if no [package] section found.
fn find_package_name(crate_root: &Path) -> Result<String, String> {
    let cargo_toml = crate_root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)
        .map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;

    // Look for name = "..." in [package] section
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = false;
        }
        if in_package && trimmed.starts_with("name") {
            if let Some(name) = trimmed.split('=').nth(1) {
                let name = name.trim().trim_matches('"').trim_matches('\'');
                return Ok(name.to_string());
            }
        }
    }
    Err("No [package] section with name found in Cargo.toml".to_string())
}

/// Find first member package in a workspace [workspace.members].
/// Useful when running mutate on a workspace root.
fn find_first_workspace_member(workspace_root: &Path) -> Result<String, String> {
    // Simple approach: scan crates/ directories for Cargo.toml with [package]
    let crates_dir = workspace_root.join("crates");
    if crates_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&crates_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let toml = path.join("Cargo.toml");
                    if toml.exists() {
                        // Found a crate! Get its name from [package] section only.
                        if let Ok(crate_content) = std::fs::read_to_string(&toml) {
                            let mut in_package = false;
                            for line in crate_content.lines() {
                                let trimmed = line.trim();
                                if trimmed == "[package]" {
                                    in_package = true;
                                    continue;
                                }
                                if trimmed.starts_with('[') {
                                    in_package = false;
                                }
                                if in_package && trimmed.starts_with("name") {
                                    if let Some(name) = trimmed.split('=').nth(1) {
                                        let name = name.trim().trim_matches('"').trim_matches('\'');
                                        return Ok(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("No workspace members found".to_string())
}

fn copy_dir_recursive_filtered(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip target/ and .git/ to avoid copying gigabytes
        if name_str == "target" || name_str == ".git" {
            continue;
        }
        let ty = entry.file_type()?;
        let dst_path = dst.join(&name);
        if ty.is_dir() {
            copy_dir_recursive_filtered(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────
// Isolated mutant tester: patches scratch copy, runs cargo test
// with a watchdog-enforced timeout, then reverts.
// ──────────────────────────────────────────────────────────────

fn test_mutant_isolated(
    mutant: &Mutant,
    crate_root: &Path,
    workspace_root: &Path,
    scratch: &ScratchCrate,
    package_name: &str,
    timeout_secs: u64,
    use_nextest: bool,
) -> MutantResult {
    // Resolve the file path relative to the original crate root
    let original_file = crate_root.join(&mutant.file);
    let scratch_file = match scratch.scratch_path_for(workspace_root, &original_file) {
        Some(p) => p,
        None => {
            return MutantResult {
                id: mutant.id,
                file: mutant.file.clone(),
                line: mutant.line,
                description: mutant.description.clone(),
                status: "error".to_string(),
                test_output: format!("Cannot resolve scratch path for {}", mutant.file),
            };
        }
    };

    // Read current (clean) state of the scratch file
    let original_source = match std::fs::read_to_string(&scratch_file) {
        Ok(s) => s,
        Err(e) => {
            return MutantResult {
                id: mutant.id,
                file: mutant.file.clone(),
                line: mutant.line,
                description: mutant.description.clone(),
                status: "error".to_string(),
                test_output: format!("Could not read scratch file: {}", e),
            };
        }
    };

    // Apply mutation to the scratch file
    let mutated_source = replace_line(&original_source, mutant.line, &mutant.mutated);
    if std::fs::write(&scratch_file, &mutated_source).is_err() {
        return MutantResult {
            id: mutant.id,
            file: mutant.file.clone(),
            line: mutant.line,
            description: mutant.description.clone(),
            status: "error".to_string(),
            test_output: "Could not write mutated scratch file".to_string(),
        };
    }

    // Run tests with cargo-nextest or cargo test
    let test_result = if use_nextest {
        // nextest doesn't need package flag when running in the package directory
        run_nextest_with_timeout(&scratch.scratch_crate_root(), timeout_secs)
    } else {
        run_cargo_test_with_timeout(&scratch.scratch_crate_root(), package_name, timeout_secs)
    };

    // Always restore the scratch file to clean state
    let _ = std::fs::write(&scratch_file, &original_source);

    match test_result {
        TestOutcome::Killed(output) => MutantResult {
            id: mutant.id,
            file: mutant.file.clone(),
            line: mutant.line,
            description: mutant.description.clone(),
            status: "killed".to_string(),
            test_output: output,
        },
        TestOutcome::Survived(output) => MutantResult {
            id: mutant.id,
            file: mutant.file.clone(),
            line: mutant.line,
            description: mutant.description.clone(),
            status: "survived".to_string(),
            test_output: output,
        },
        TestOutcome::Timeout => MutantResult {
            id: mutant.id,
            file: mutant.file.clone(),
            line: mutant.line,
            description: mutant.description.clone(),
            status: "timeout".to_string(),
            test_output: format!("Timed out after {}s", timeout_secs),
        },
        TestOutcome::Error(msg) => MutantResult {
            id: mutant.id,
            file: mutant.file.clone(),
            line: mutant.line,
            description: mutant.description.clone(),
            status: "error".to_string(),
            test_output: msg,
        },
    }
}

enum TestOutcome {
    Killed(String),
    Survived(String),
    Timeout,
    Error(String),
}

/// Spawn `cargo test --quiet` in `crate_root`, kill it after `timeout_secs` via watchdog thread.
/// Run tests with cargo-nextest (3x faster, better memory isolation)
fn run_nextest_with_timeout(crate_root: &Path, timeout_secs: u64) -> TestOutcome {
    let mut cmd = std::process::Command::new("cargo-nextest");
    cmd.args(["run", "--no-capture"])
        .current_dir(crate_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TestOutcome::Error(format!("Failed to spawn cargo-nextest: {}", e)),
    };

    // Watchdog: kills the child after timeout_secs.
    let child_id = child.id();
    let timed_out = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let timed_out_clone = Arc::clone(&timed_out);
    let done_clone = Arc::clone(&done);

    let watchdog = thread::spawn(move || {
        let deadline = Duration::from_secs(timeout_secs);
        let tick = Duration::from_millis(100);
        let mut elapsed = Duration::ZERO;
        while elapsed < deadline {
            if done_clone.load(Ordering::Relaxed) {
                return; // process finished normally, bail out
            }
            thread::sleep(tick);
            elapsed += tick;
        }
        timed_out_clone.store(true, Ordering::Relaxed);
        // Kill the entire process group so cargo child procs die too
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child_id as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child_id.to_string()])
            .output();
    });

    let output = child.wait_with_output();
    done.store(true, Ordering::Relaxed); // tell watchdog we're done
    let _ = watchdog.join();

    if timed_out.load(Ordering::Relaxed) {
        return TestOutcome::Timeout;
    }

    match output {
        Err(e) => TestOutcome::Error(format!("cargo-nextest failed: {}", e)),
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = format!("{}\n{}", stdout, stderr);
            if out.status.success() {
                TestOutcome::Survived(combined)
            } else {
                TestOutcome::Killed(combined)
            }
        }
    }
}

/// Sets CARGO_TARGET_DIR to the shared host target dir to reuse build artifacts and avoid
/// recompiling everything from scratch for each mutation.
fn run_cargo_test_with_timeout(
    crate_root: &Path,
    package_name: &str,
    timeout_secs: u64,
) -> TestOutcome {
    // Pass --target-dir explicitly so the scratch crate reuses the host build cache.
    // This avoids recompiling everything from scratch for each mutant.
    let target_dir = home_target_dir();

    // Limit parallelism to prevent OOM - critical fix!
    let mut cmd = std::process::Command::new("cargo");
    cmd.env("CARGO_BUILD_JOBS", "1"); // Prevent parallel compilation OOM
    cmd.env("RUST_TEST_THREADS", "1"); // Prevent parallel test OOM
    cmd.args([
        "test",
        "--quiet",
        "-p",
        package_name,
        "--target-dir",
        target_dir.to_str().unwrap_or("target"),
    ]);
    cmd.current_dir(crate_root);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TestOutcome::Error(format!("Failed to spawn cargo: {}", e)),
    };

    // Watchdog: kills the child after timeout_secs.
    // Uses an AtomicBool so we can signal it to stop without blocking.
    let child_id = child.id();
    let timed_out = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let timed_out_clone = Arc::clone(&timed_out);
    let done_clone = Arc::clone(&done);
    let watchdog = thread::spawn(move || {
        let deadline = Duration::from_secs(timeout_secs);
        let tick = Duration::from_millis(100);
        let mut elapsed = Duration::ZERO;
        while elapsed < deadline {
            if done_clone.load(Ordering::Relaxed) {
                return; // process finished normally, bail out
            }
            thread::sleep(tick);
            elapsed += tick;
        }
        timed_out_clone.store(true, Ordering::Relaxed);
        // Kill the entire process group so cargo child procs die too
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child_id as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child_id.to_string()])
            .output();
    });

    let output = child.wait_with_output();
    done.store(true, Ordering::Relaxed); // tell watchdog we're done
    let _ = watchdog.join();

    if timed_out.load(Ordering::Relaxed) {
        return TestOutcome::Timeout;
    }

    match output {
        Err(e) => TestOutcome::Error(format!("cargo test failed: {}", e)),
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = format!("{}\n{}", stdout, stderr);
            if out.status.success() {
                TestOutcome::Survived(combined)
            } else {
                TestOutcome::Killed(combined)
            }
        }
    }
}

/// Returns a shared target directory for reusing build artifacts across mutations.
/// Looks up the real workspace target/ via CARGO_TARGET_DIR env or walks to find it.
fn home_target_dir() -> PathBuf {
    // If already set, honour it
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    // Otherwise use the standard location alongside the binary
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            // binary is at <workspace>/target/debug/mutate
            // walk up to find the target/ dir
            p.parent()?.parent().map(|p| p.to_path_buf())
        })
        .unwrap_or_else(|| PathBuf::from("target"))
}

fn verify_tests_pass(crate_root: &Path, package_name: &str, timeout: u64) -> Result<(), String> {
    use std::io::Write;
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let spinner = thread::spawn(move || {
        let tick = Duration::from_secs(1);
        let mut elapsed = 0u64;
        while !done_clone.load(Ordering::Relaxed) {
            eprint!("\r  Verifying baseline tests... {}s", elapsed);
            let _ = std::io::stderr().flush();
            thread::sleep(tick);
            elapsed += 1;
        }
    });
    let outcome = run_cargo_test_with_timeout(crate_root, package_name, timeout);
    done.store(true, Ordering::Relaxed);
    let _ = spinner.join();
    eprint!("\r");
    match outcome {
        TestOutcome::Survived(_) => {
            println!("✓ Original tests pass.");
            println!();
            Ok(())
        }
        TestOutcome::Killed(out) => Err(format!(
            "Tests fail on original code. Fix tests before mutating.\n{}",
            &out[..out.len().min(500)]
        )),
        TestOutcome::Timeout => Err(format!(
            "Baseline test timed out after {}s. Increase --timeout or fix slow tests.",
            timeout
        )),
        TestOutcome::Error(e) => Err(e),
    }
}

/// Generate mutants for a single file with a limit to prevent memory blowup
fn generate_mutants_for_file(
    source: &str,
    file_path: &str,
    strategy: &str,
    limit: usize,
) -> Vec<Mutant> {
    generate_mutants(source, file_path, &mut 0, strategy, limit)
}

/// Generate all possible mutants for a source file
fn generate_mutants(
    source: &str,
    file_path: &str,
    next_id: &mut usize,
    strategy: &str,
    limit: usize,
) -> Vec<Mutant> {
    let mut mutants = Vec::with_capacity(limit.min(1000));
    let include_standard = strategy == "all" || strategy == "standard";
    let include_bitwise = strategy == "all" || strategy == "bitwise";
    let include_arithmetic = strategy == "all" || strategy == "arithmetic";
    let include_boundary = strategy == "all" || strategy == "boundary";

    macro_rules! push_if_limit {
        ($mutant:expr) => {
            if mutants.len() >= limit {
                return mutants;
            }
            mutants.push($mutant);
        };
    }

    // Strategy 1: Binary operator swaps (standard)
    if include_standard {
        let operator_swaps = [
            ("+", "-"),
            ("-", "+"),
            ("*", "/"),
            ("/", "*"),
            ("==", "!="),
            ("!=", "=="),
            (">", "<"),
            ("<", ">"),
            (">=", "<="),
            ("<=", ">="),
            ("&&", "||"),
            ("||", "&&"),
        ];

        for (original_op, mutated_op) in &operator_swaps {
            for (line_num, line) in source.lines().enumerate() {
                if line.contains(original_op) && !line.trim_start().starts_with("//") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: format!("Replace '{}' with '{}'", original_op, mutated_op),
                        original: line.to_string(),
                        mutated: line.replace(original_op, mutated_op),
                        category: "standard".to_string(),
                    });
                }
            }
        }

        // Boolean literal swaps (standard)
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("//") {
                if line.contains("true") && !line.contains("// true") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace 'true' with 'false'".to_string(),
                        original: line.to_string(),
                        mutated: line.replace("true", "false"),
                        category: "standard".to_string(),
                    });
                }
                if line.contains("false") && !line.contains("// false") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace 'false' with 'true'".to_string(),
                        original: line.to_string(),
                        mutated: line.replace("false", "true"),
                        category: "standard".to_string(),
                    });
                }
            }
        }
    }

    // Strategy 2: Boundary value mutations
    if include_boundary {
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("//") {
                if line.contains(" < ") && !line.contains(" <= ") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace '<' with '<=' (boundary)".to_string(),
                        original: line.to_string(),
                        mutated: line.replacen(" < ", " <= ", 1),
                        category: "boundary".to_string(),
                    });
                }
                if line.contains(" <= ") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace '<=' with '<' (boundary)".to_string(),
                        original: line.to_string(),
                        mutated: line.replacen(" <= ", " < ", 1),
                        category: "boundary".to_string(),
                    });
                }
                if line.contains(" >= ") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace '>=' with '>' (boundary)".to_string(),
                        original: line.to_string(),
                        mutated: line.replacen(" >= ", " > ", 1),
                        category: "boundary".to_string(),
                    });
                }
                if line.contains(" > ") && !line.contains(" >= ") {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: "Replace '>' with '>=' (boundary)".to_string(),
                        original: line.to_string(),
                        mutated: line.replacen(" > ", " >= ", 1),
                        category: "boundary".to_string(),
                    });
                }
            }
        }
    }

    // Strategy 3: Bitwise operator mutations
    if include_bitwise {
        let bitwise_swaps = [
            (" ^ ", " | "),
            (" | ", " ^ "),
            (" << ", " >> "),
            (" >> ", " << "),
            (" & ", " | "),
            (" | ", " & "),
        ];

        for (original_op, mutated_op) in &bitwise_swaps {
            for (line_num, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if !trimmed.starts_with("//") && line.contains(original_op) {
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: format!(
                            "Replace '{}' with '{}' (bitwise)",
                            original_op.trim(),
                            mutated_op.trim()
                        ),
                        original: line.to_string(),
                        mutated: line.replace(original_op, mutated_op),
                        category: "bitwise".to_string(),
                    });
                }
            }
        }
    }

    // Strategy 4: Arithmetic overflow mutations
    if include_arithmetic {
        let arithmetic_mutations = [
            (
                "wrapping_add",
                "+",
                "Replace wrapping_add with + (overflow check)",
            ),
            (
                "wrapping_sub",
                "-",
                "Replace wrapping_sub with - (overflow check)",
            ),
            (
                "wrapping_mul",
                "*",
                "Replace wrapping_mul with * (overflow check)",
            ),
            (
                "saturating_add",
                "+",
                "Replace saturating_add with + (overflow check)",
            ),
            (
                "saturating_sub",
                "-",
                "Replace saturating_sub with - (overflow check)",
            ),
            (
                "saturating_mul",
                "*",
                "Replace saturating_mul with * (overflow check)",
            ),
            (
                "checked_add",
                "+",
                "Replace checked_add with + (unwrap result)",
            ),
            (
                "checked_sub",
                "-",
                "Replace checked_sub with - (unwrap result)",
            ),
            (
                "checked_mul",
                "*",
                "Replace checked_mul with * (unwrap result)",
            ),
        ];

        for (func_name, _operator, desc) in &arithmetic_mutations {
            for (line_num, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if !trimmed.starts_with("//") && line.contains(func_name) {
                    let mutated = line.replace(&format!(".{func_name}("), ".");
                    let mutated = mutated.replace(&format!("{func_name}("), "( ");
                    *next_id += 1;
                    push_if_limit!(Mutant {
                        id: *next_id,
                        file: file_path.to_string(),
                        line: line_num + 1,
                        description: desc.to_string(),
                        original: line.to_string(),
                        mutated,
                        category: "arithmetic".to_string(),
                    });
                }
            }
        }
    }

    mutants
}

/// Replace a specific line (1-indexed) in source
fn replace_line(source: &str, line_num: usize, new_content: &str) -> String {
    source
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i + 1 == line_num {
                new_content.to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Find source files to mutate
/// If path is a workspace root, looks in crates/<package_name>/src/
fn find_source_files(
    crate_root: &Path,
    package_name: &str,
    filter: &Option<String>,
) -> Vec<PathBuf> {
    if let Some(files) = filter {
        return files
            .split(',')
            .map(|f| crate_root.join(f.trim()))
            .filter(|p| p.exists())
            .collect();
    }

    // Determine the source directory
    // Try src/ first (standard crate layout)
    let src_dir = crate_root.join("src");
    let mut files = Vec::new();

    if src_dir.exists() && src_dir.is_dir() {
        find_rs_files(&src_dir, &mut files);
    } else {
        // Try crates/<package>/src (workspace layout)
        let crate_src_dir = crate_root
            .join("crates")
            .join(package_name.replace('_', "-"))
            .join("src");
        if crate_src_dir.exists() && crate_src_dir.is_dir() {
            find_rs_files(&crate_src_dir, &mut files);
        } else {
            // Try lib/ as fallback
            let lib_dir = crate_root.join("lib");
            if lib_dir.exists() && lib_dir.is_dir() {
                find_rs_files(&lib_dir, &mut files);
            }
        }
    }

    files.sort();
    files
}

fn find_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            } else if path.is_dir() {
                find_rs_files(&path, files);
            }
        }
    }
}

fn output_table_streaming(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
) {
    println!();
    println!("MUTATION TESTING RESULTS");
    println!("{}", separator(80));

    if survived > 0 {
        println!();
        println!("SURVIVED MUTANTS (tests didn't catch these changes):");

        let columns = [
            Column::left("ID", 6),
            Column::left("FILE", 40),
            Column::right("LINE", 5),
            Column::left("DESCRIPTION", 30),
        ];
        print_table_header(&columns);

        for r in results.iter().filter(|r| r.status == "survived") {
            let id_str = format!("[{}]", r.id);
            let line_str = r.line.to_string();
            print_table_row(&columns, &[&id_str, &r.file, &line_str, &r.description]);
        }
    }

    println!();
    println!("{}", separator(80));

    let score = if total > 0 {
        killed as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let verdict = if score >= 90.0 {
        "Excellent -- strong test suite"
    } else if score >= 70.0 {
        "Good -- most mutations caught"
    } else if score >= 50.0 {
        "Weak -- many mutations survived"
    } else {
        "Poor -- test suite needs significant work"
    };

    let summary = [
        ("Total mutants:", total.to_string()),
        (
            "Killed:",
            format!("{} ({:.0}%)", killed, killed as f64 / total as f64 * 100.0),
        ),
        (
            "Survived:",
            format!(
                "{} ({:.0}%)",
                survived,
                survived as f64 / total as f64 * 100.0
            ),
        ),
        ("Mutation Score:", format!("{:.0}%", score)),
        ("Verdict:", verdict.to_string()),
    ];
    cogent_common::print_summary(&summary);

    if timeouts > 0 {
        println!("  Timeout:        {}", timeouts);
    }
    if errors > 0 {
        println!("  Error:          {}", errors);
    }

    if survived > 0 {
        println!();
        println!(
            "  {} mutant(s) survived. Your tests didn't detect these code changes.",
            survived
        );
        println!("    Consider adding tests for the affected functions.");
    }
}

fn output_json_streaming(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
    duration_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let score = if total > 0 {
        killed as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    let report = MutationReport {
        results: results.to_vec(),
        summary: MutationSummary {
            total_mutants: total,
            killed,
            survived,
            timeout: timeouts,
            error: errors,
            mutation_score: score,
        },
    };

    let response = wrap_tool_response(
        "mutate",
        env!("CARGO_PKG_VERSION"),
        true,
        duration_ms,
        serde_json::to_value(&report).unwrap(),
        Some(serde_json::json!({
            "total_mutants": total,
            "killed": killed,
            "survived": survived,
            "mutation_score": score,
            "passed": survived == 0 && errors == 0,
        })),
        None,
    );

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bitwise_mutants() {
        let source = r#"
fn test() {
    let a = 1 ^ 2;
    let b = 3 << 4;
    let c = 5 & 6;
}
"#;
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "bitwise", 1000);

        // Should find XOR, shift, and AND mutations
        assert!(!mutants.is_empty(), "Should generate bitwise mutants");
        assert!(mutants.iter().any(|m| m.description.contains("bitwise")));
        assert!(mutants.iter().all(|m| m.category == "bitwise"));
    }

    #[test]
    fn test_generate_arithmetic_mutants() {
        let source = r#"
fn test() {
    let a = 1u32.wrapping_add(2);
    let b = 3u32.saturating_sub(1);
}
"#;
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);

        assert!(!mutants.is_empty(), "Should generate arithmetic mutants");
        assert!(mutants
            .iter()
            .any(|m| m.description.contains("wrapping") || m.description.contains("saturating")));
        assert!(mutants.iter().all(|m| m.category == "arithmetic"));
    }

    #[test]
    fn test_strategy_filtering_standard() {
        let source = r#"
fn test() {
    let a = 1 + 2;
    let b = 3 ^ 4;
}
"#;
        let mut id = 0;
        let standard = generate_mutants(source, "test.rs", &mut id, "standard", 1000);
        assert!(standard.iter().all(|m| m.category == "standard"));
        assert!(!standard.iter().any(|m| m.category == "bitwise"));
    }

    #[test]
    fn test_strategy_filtering_bitwise() {
        let source = r#"
fn test() {
    let a = 1 + 2;
    let b = 3 ^ 4;
}
"#;
        let mut id = 0;
        let bitwise = generate_mutants(source, "test.rs", &mut id, "bitwise", 1000);
        assert!(bitwise.iter().all(|m| m.category == "bitwise"));
        assert!(!bitwise.iter().any(|m| m.category == "standard"));
    }

    #[test]
    fn test_mutant_has_category() {
        let mutants = [Mutant {
            id: 1,
            file: "test.rs".to_string(),
            line: 1,
            description: "test".to_string(),
            original: "a + b".to_string(),
            mutated: "a - b".to_string(),
            category: "standard".to_string(),
        }];
        assert_eq!(mutants[0].category, "standard");
    }
}
