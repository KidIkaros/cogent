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

use cogent_common::{separator, wrap_tool_response, Column};

// ═══════════════════════════════════════════
// LANGUAGE MUTATION CONFIG TABLE
// ═══════════════════════════════════════════

/// Configuration for counting potential mutation points in a given language.
struct MutationLangConfig {
    /// Operator patterns to count (e.g. "==", "&&")
    operators: &'static [&'static str],
    /// Control-flow keywords to count (e.g. "if ", "for ")
    keywords: &'static [&'static str],
    /// Human-readable language name
    display_name: &'static str,
    /// Installation/usage instructions for mutation tools
    tool_instructions: &'static [&'static str],
}

const LANGUAGE_CONFIGS: &[(&[&str], MutationLangConfig)] = &[
    (
        &["rb"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while "],
            display_name: "Ruby",
            tool_instructions: &["  $ gem install mutant-rs", "  $ mutant path/to/file.rb"],
        },
    ),
    (
        &["swift"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while ", "switch ", "guard "],
            display_name: "Swift",
            tool_instructions: &[
                "  # Swift mutation tools require manual setup",
                "  # Consider using SwiftMutator or SwiftCheck",
            ],
        },
    ),
    (
        &["py"],
        MutationLangConfig {
            operators: &["==", "!=", "and ", "or "],
            keywords: &["if ", "for ", "while "],
            display_name: "Python",
            tool_instructions: &[
                "  $ pip install cosmic-ray",
                "  $ cosmic-ray run --test-runner pytest path/to/file.py",
            ],
        },
    ),
    (
        &["c", "h"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while ", "switch ", "case "],
            display_name: "C",
            tool_instructions: &[
                "  # C mutation testing",
                "  $ cargo install mull",
                "  $ mull-cpp -mutators=all path/to/file.c",
            ],
        },
    ),
    (
        &["cpp", "cc", "cxx"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while ", "switch ", "case "],
            display_name: "C++",
            tool_instructions: &[
                "  # C/C++ mutation testing",
                "  $ cargo install mull",
                "  $ mull-cpp -mutators=all path/to/file.cpp",
            ],
        },
    ),
    (
        &["cs"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if", "for", "while", "switch", "try"],
            display_name: "C#",
            tool_instructions: &[
                "  # C# mutation testing",
                "  $ dotnet tool install --global dotnet-mutator",
                "  $ dotnet-mutator run path/to/File.cs",
            ],
        },
    ),
    (
        &["java"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if", "for", "while", "switch", "case", "try", "catch"],
            display_name: "Java",
            tool_instructions: &[
                "  # Java mutation testing",
                "  $ mvn org.pitest:pitest-maven:calculate-coverage",
                "  $ mvn org.pitest:pitest-maven:mutationCoverage path/to/file.java",
            ],
        },
    ),
    (
        &["php"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while ", "switch ", "case ", "foreach"],
            display_name: "PHP",
            tool_instructions: &[
                "  # PHP mutation testing",
                "  $ composer require --dev infection/infection",
                "  $ vendor/bin/infection path/to/file.php",
            ],
        },
    ),
    (
        &["go"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "switch ", "case ", "select "],
            display_name: "Go",
            tool_instructions: &[
                "  $ go install github.com/zimmsja/go-mutesting@latest",
                "  $ go-mutesting ./path/to/file.go",
            ],
        },
    ),
    (
        &["js", "ts"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if", "for", "while", "switch", "case", "try", "catch"],
            display_name: "JavaScript/TypeScript",
            tool_instructions: &[
                "  $ npm install -g stryker-mutator-core",
                "  $ npx stryker run path/to/file.js",
            ],
        },
    ),
    (
        &["kt", "kts"],
        MutationLangConfig {
            operators: &["==", "!=", "&&", "||"],
            keywords: &["if ", "for ", "while ", "when "],
            display_name: "Kotlin",
            tool_instructions: &[
                "  # Kotlin mutation testing",
                "  # https://github.com/Fleshgrinder/kotlin-mutation-testing",
            ],
        },
    ),
];

/// Look up config for a given file extension.
fn lang_config_for(ext: &str) -> Option<&'static MutationLangConfig> {
    LANGUAGE_CONFIGS
        .iter()
        .find(|(exts, _)| exts.contains(&ext))
        .map(|(_, config)| config)
}

/// Count potential mutation points in source code for the given language config.
fn count_potential_mutations(source: &str, config: &MutationLangConfig) -> usize {
    let mut count = 0;
    for op in config.operators {
        if source.contains(op) {
            count += source.matches(op).count();
        }
    }
    for kw in config.keywords {
        if source.contains(kw) {
            count += source.matches(kw).count();
        }
    }
    count
}

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
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return Err(format!("Failed to read {}: {}", path, e)),
    };

    let config = lang_config_for(path.rsplit('.').next().unwrap_or(""));
    print!("{}", format_mutation_analysis(path, &source, config));
    Ok(())
}

fn format_mutation_analysis(
    path: &str,
    source: &str,
    config: Option<&MutationLangConfig>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let lang_name = config.map(|c| c.display_name).unwrap_or("Unknown");

    writeln!(out, "MUTATION ANALYSIS (analysis mode - no test execution)").unwrap();
    writeln!(
        out,
        "Note: Full mutation testing with test execution is Rust-only."
    )
    .unwrap();
    writeln!(
        out,
        "For non-Rust languages: Use language-specific mutation frameworks."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "To run full mutation tests:").unwrap();
    for instr in config.map_or(&[] as &[&str], |c| c.tool_instructions) {
        writeln!(out, "{}", instr).unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "Language: {}", lang_name).unwrap();
    writeln!(out, "File: {}", path).unwrap();
    writeln!(out).unwrap();

    let potential_mutations = config
        .map(|c| count_potential_mutations(source, c))
        .unwrap_or(0);

    writeln!(out, "Analysis complete:").unwrap();
    writeln!(out, "  Potential mutation points: {}", potential_mutations).unwrap();
    writeln!(
        out,
        "  Estimated test coverage needed: {}-{}%",
        potential_mutations * 2,
        potential_mutations * 3
    )
    .unwrap();

    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

fn resolve_crate_root(path: &str) -> Result<PathBuf, String> {
    Path::new(path)
        .canonicalize()
        .map_err(|e| format!("Cannot resolve path {}: {}", path, e))
}

fn compute_delta_analysis(
    crate_root: &Path,
    base_ref: &str,
    source_files: &[PathBuf],
) -> delta::DeltaAnalysis {
    let loaded_files: Vec<(String, String)> = source_files
        .iter()
        .filter_map(|f| {
            let s = std::fs::read_to_string(f).ok()?;
            Some((f.to_string_lossy().to_string(), s))
        })
        .collect();

    let analysis =
        delta::run_delta_analysis(crate_root, base_ref, &loaded_files, source_files.len());

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

    analysis
}

fn auto_detect_package_name(
    crate_root: &Path,
    cli_package: &Option<String>,
) -> Result<String, String> {
    if let Some(ref pkg) = cli_package {
        Ok(pkg.clone())
    } else {
        find_package_name(crate_root)
            .or_else(|_| find_first_workspace_member(crate_root))
            .map_err(|e| {
                format!("Could not auto-detect package name. Use -p/--package flag or pass a crate with [package] name. Error: {}", e)
            })
    }
}

/// Results collected from running mutations against all source files.
struct MutationResults {
    results: Vec<MutantResult>,
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
}

#[allow(clippy::too_many_arguments)]
fn run_mutation_loop(
    source_files: &[PathBuf],
    max_mutants: usize,
    strategy: &str,
    delta_analysis: &Option<delta::DeltaAnalysis>,
    crate_root: &Path,
    workspace_root: &Path,
    scratch: &ScratchCrate,
    package_name: &str,
    timeout: u64,
    nextest: bool,
) -> MutationResults {
    let mut all_results: Vec<MutantResult> = Vec::new();
    let mut total_mutants = 0usize;
    let mut killed = 0usize;
    let mut survived = 0usize;
    let mut timeouts = 0usize;
    let mut errors = 0usize;

    for file_path in source_files {
        if total_mutants >= max_mutants {
            break;
        }

        let Ok(source) = std::fs::read_to_string(file_path) else {
            eprintln!("Warning: Could not read {}", file_path.display());
            continue;
        };

        let remaining = max_mutants.saturating_sub(total_mutants);
        let mut file_mutants =
            generate_mutants_for_file(&source, &file_path.to_string_lossy(), strategy, remaining);

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
                workspace_root,
                scratch,
                package_name,
                timeout,
                nextest,
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

    MutationResults {
        results: all_results,
        total: total_mutants,
        killed,
        survived,
        timeouts,
        errors,
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let start = std::time::Instant::now();
    let crate_root_raw = Path::new(&cli.path);
    let crate_root = resolve_crate_root(&cli.path)?;

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

    let package_name = auto_detect_package_name(&crate_root, &cli.package)?;

    // Hard ceiling to prevent runaway test sessions
    let max_mutants = cli.max_mutants.min(50);
    if cli.max_mutants > 50 {
        eprintln!("Warning: --max-mutants capped at 50 to prevent system overload.");
    }

    // Verify tests pass in the ORIGINAL crate first (uses existing build cache)
    verify_tests_pass(&crate_root, &package_name, cli.timeout)?;

    // Build the scratch directory once; all mutations run there
    let scratch = ScratchCrate::new(&crate_root)?;
    eprintln!("Scratch dir: {}", scratch.root.display());

    let source_files = find_source_files(&crate_root, &package_name, &cli.files);
    if source_files.is_empty() {
        return Err("No source files found to mutate.".to_string());
    }

    // Delta mutation testing: compute affected functions from git diff
    let delta_analysis = if cli.delta {
        println!(
            "Computing delta mutation analysis against {}...",
            cli.base_ref
        );
        Some(compute_delta_analysis(
            &crate_root,
            &cli.base_ref,
            &source_files,
        ))
    } else {
        println!("Found {} source files to mutate.\n", source_files.len());
        None
    };

    let workspace_root = find_workspace_root(&crate_root);

    let res = run_mutation_loop(
        &source_files,
        max_mutants,
        &cli.strategy,
        &delta_analysis,
        &crate_root,
        &workspace_root,
        &scratch,
        &package_name,
        cli.timeout,
        cli.nextest,
    );
    let total_mutants = res.total;
    let killed = res.killed;
    let survived = res.survived;
    let timeouts = res.timeouts;
    let errors = res.errors;
    let all_results = res.results;

    // scratch dir cleaned up automatically via Drop
    drop(scratch);

    if total_mutants == 0 {
        println!("No mutants to test (--max-mutants 0 or no matching code).");
        return Ok(());
    }

    output_mutation_results(
        &all_results,
        total_mutants,
        killed,
        survived,
        timeouts,
        errors,
        &cli.format,
        start,
    );

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
    let canonical = crate_root
        .canonicalize()
        .unwrap_or_else(|_| crate_root.to_path_buf());
    let mut dir = canonical.clone();
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
            None => return canonical,
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

// ── Format helpers (string-returning versions of cogent_common print functions) ──

fn format_table_header(columns: &[Column]) -> String {
    let mut line = String::new();
    for col in columns {
        if col.align_right {
            use std::fmt::Write;
            write!(line, "{:>width$} ", col.header, width = col.width).unwrap();
        } else {
            use std::fmt::Write;
            write!(line, "{:<width$} ", col.header, width = col.width).unwrap();
        }
    }
    let header = line.trim_end().to_string();
    let total_width: usize = columns.iter().map(|c| c.width + 1).sum();
    format!("{}\n{}", header, separator(total_width))
}

fn format_table_row(columns: &[Column], values: &[&str]) -> String {
    let mut line = String::new();
    for (col, val) in columns.iter().zip(values.iter()) {
        let truncated = cogent_common::truncate(val, col.width);
        if col.align_right {
            use std::fmt::Write;
            write!(line, "{:>width$} ", truncated, width = col.width).unwrap();
        } else {
            use std::fmt::Write;
            write!(line, "{:<width$} ", truncated, width = col.width).unwrap();
        }
    }
    line.trim_end().to_string()
}

fn format_summary(items: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (key, value) in items {
        use std::fmt::Write;
        writeln!(out, "  {:<25} {}", key, value).unwrap();
    }
    out
}

fn output_table_streaming(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
) {
    println!(
        "{}",
        format_mutation_table(results, total, killed, survived, timeouts, errors)
    );
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
    let value = build_mutation_report_json(
        results,
        total,
        killed,
        survived,
        timeouts,
        errors,
        duration_ms,
    );
    println!("{}", format_mutation_report_json(&value));
    Ok(())
}

fn build_mutation_report_json(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
    duration_ms: u64,
) -> serde_json::Value {
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

    serde_json::to_value(wrap_tool_response(
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
    ))
    .unwrap()
}

fn format_mutation_report_json(report: &serde_json::Value) -> String {
    serde_json::to_string_pretty(report).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn output_mutation_results(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
    format: &str,
    start: std::time::Instant,
) {
    match format {
        "json" => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = output_json_streaming(
                results,
                total,
                killed,
                survived,
                timeouts,
                errors,
                duration_ms,
            );
        }
        _ => output_table_streaming(results, total, killed, survived, timeouts, errors),
    }
}

fn format_mutation_table(
    results: &[MutantResult],
    total: usize,
    killed: usize,
    survived: usize,
    timeouts: usize,
    errors: usize,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out).unwrap();
    writeln!(out, "MUTATION TESTING RESULTS").unwrap();
    writeln!(out, "{}", separator(80)).unwrap();

    if survived > 0 {
        writeln!(out).unwrap();
        writeln!(out, "SURVIVED MUTANTS (tests didn't catch these changes):").unwrap();

        let columns = [
            Column::left("ID", 6),
            Column::left("FILE", 40),
            Column::right("LINE", 5),
            Column::left("DESCRIPTION", 30),
        ];

        writeln!(out, "{}", format_table_header(&columns)).unwrap();

        for r in results.iter().filter(|r| r.status == "survived") {
            let id_str = format!("[{}]", r.id);
            let line_str = r.line.to_string();
            writeln!(
                out,
                "{}",
                format_table_row(&columns, &[&id_str, &r.file, &line_str, &r.description])
            )
            .unwrap();
        }
    }

    writeln!(out).unwrap();
    writeln!(out, "{}", separator(80)).unwrap();

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
    write!(out, "{}", format_summary(&summary)).unwrap();

    if timeouts > 0 {
        writeln!(out, "  Timeout:        {}", timeouts).unwrap();
    }
    if errors > 0 {
        writeln!(out, "  Error:          {}", errors).unwrap();
    }

    if survived > 0 {
        writeln!(out).unwrap();
        writeln!(
            out,
            "  {} mutant(s) survived. Your tests didn't detect these code changes.",
            survived
        )
        .unwrap();
        writeln!(out, "    Consider adding tests for the affected functions.").unwrap();
    }

    out
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

        assert_eq!(mutants.len(), 2, "Should generate 2 arithmetic mutants");
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

    // ── Boundary mutation tests ──────────────

    #[test]
    fn test_generate_boundary_less_than() {
        let source = "if x < 5 {\n    let y = 1;\n}\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0].description, "Replace '<' with '<=' (boundary)");
        assert_eq!(mutants[0].mutated, "if x <= 5 {", "< should become <=");
        assert_eq!(mutants[0].category, "boundary");
        assert_eq!(mutants[0].line, 1);
    }

    #[test]
    fn test_generate_boundary_less_than_or_equal() {
        let source = "if x <= 5 {\n    let y = 1;\n}\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0].description, "Replace '<=' with '<' (boundary)");
        assert_eq!(mutants[0].mutated, "if x < 5 {", "<= should become <");
    }

    #[test]
    fn test_generate_boundary_greater_than() {
        let source = "if x > 5 {\n}\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0].description, "Replace '>' with '>=' (boundary)");
        assert_eq!(mutants[0].mutated, "if x >= 5 {", "> should become >=");
    }

    #[test]
    fn test_generate_boundary_greater_than_or_equal() {
        let source = "if x >= 5 {\n}\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0].description, "Replace '>=' with '>' (boundary)");
        assert_eq!(mutants[0].mutated, "if x > 5 {", ">= should become >");
    }

    #[test]
    fn test_generate_boundary_skips_comment_lines() {
        let source = "// if x < 5 {\nlet y = 1;\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert!(
            mutants.is_empty(),
            "Commented lines should not produce mutants"
        );
    }

    #[test]
    fn test_generate_boundary_no_matches() {
        let source = "let x = 1 + 2;\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        assert!(
            mutants.is_empty(),
            "No boundary ops should yield no mutants"
        );
    }

    #[test]
    fn test_generate_boundary_skips_when_equal_already_present() {
        // When line has both < and <=, only <= should be mutated (not <)
        let source = "if x <= 5 && y < 3 {\n}\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "boundary", 1000);
        // <= becomes < produces 1 mutant; < is skipped because <= is on the same line
        assert_eq!(mutants.len(), 1, "Should only mutate <=, not <");
        assert_eq!(mutants[0].description, "Replace '<=' with '<' (boundary)");
    }

    // ── Extended arithmetic mutation tests ───

    #[test]
    fn test_generate_arithmetic_wrapping_add_exact() {
        let source = "let a = 1u32.wrapping_add(2);\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(
            mutants[0].description,
            "Replace wrapping_add with + (overflow check)"
        );
        // .wrapping_add( is replaced with ., consuming the opening paren
        assert_eq!(
            mutants[0].mutated, "let a = 1u32.2);",
            ".wrapping_add( -> ., consuming the ("
        );
        assert_eq!(mutants[0].category, "arithmetic");
    }

    #[test]
    fn test_generate_arithmetic_saturating_sub_exact() {
        let source = "let b = 3u32.saturating_sub(1);\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(
            mutants[0].description,
            "Replace saturating_sub with - (overflow check)"
        );
        assert_eq!(mutants[0].mutated, "let b = 3u32.1);");
    }

    #[test]
    fn test_generate_arithmetic_checked_add_exact() {
        let source = "let a = 5u32.checked_add(3);\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(
            mutants[0].description,
            "Replace checked_add with + (unwrap result)"
        );
        assert_eq!(mutants[0].mutated, "let a = 5u32.3);");
    }

    #[test]
    fn test_generate_arithmetic_checked_sub_exact() {
        let source = "let b = 8u32.checked_sub(2);\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(
            mutants[0].description,
            "Replace checked_sub with - (unwrap result)"
        );
        assert_eq!(mutants[0].mutated, "let b = 8u32.2);");
    }

    #[test]
    fn test_generate_arithmetic_checked_mul_exact() {
        let source = "let c = 5u32.checked_mul(3);\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 1);
        assert_eq!(
            mutants[0].description,
            "Replace checked_mul with * (unwrap result)"
        );
        assert_eq!(mutants[0].mutated, "let c = 5u32.3);");
    }

    #[test]
    fn test_generate_arithmetic_skips_comment_lines() {
        let source = "// let a = 1u32.wrapping_add(2);\nlet b = 5;\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert!(
            mutants.is_empty(),
            "Commented lines should not produce mutants"
        );
    }

    #[test]
    fn test_generate_arithmetic_no_matches() {
        let source = "let x = 1 + 2;\n";
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert!(
            mutants.is_empty(),
            "No arithmetic ops should yield no mutants"
        );
    }

    #[test]
    fn test_generate_arithmetic_all_variants() {
        let source = r#"
fn test() {
    let a = 1u32.wrapping_add(2);
    let b = 3u32.wrapping_sub(1);
    let c = 5u32.wrapping_mul(2);
    let d = 7u32.saturating_add(2);
    let e = 9u32.saturating_sub(1);
    let f = 11u32.saturating_mul(2);
}
"#;
        let mut id = 0;
        let mutants = generate_mutants(source, "test.rs", &mut id, "arithmetic", 1000);
        assert_eq!(mutants.len(), 6, "Should find all 6 arithmetic variants");
        assert!(mutants.iter().all(|m| m.category == "arithmetic"));
        // Check that all variants are present
        let descs: Vec<&str> = mutants.iter().map(|m| m.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("wrapping_add")));
        assert!(descs.iter().any(|d| d.contains("wrapping_sub")));
        assert!(descs.iter().any(|d| d.contains("wrapping_mul")));
        assert!(descs.iter().any(|d| d.contains("saturating_add")));
        assert!(descs.iter().any(|d| d.contains("saturating_sub")));
        assert!(descs.iter().any(|d| d.contains("saturating_mul")));
    }

    // ── Language config lookup tests ─────────

    #[test]
    fn test_lang_config_for_ruby() {
        let cfg = lang_config_for("rb").unwrap();
        assert_eq!(cfg.display_name, "Ruby");
        assert!(cfg.operators.contains(&"=="));
        assert!(cfg.keywords.contains(&"if "));
    }

    #[test]
    fn test_lang_config_for_python() {
        let cfg = lang_config_for("py").unwrap();
        assert_eq!(cfg.display_name, "Python");
        assert!(cfg.operators.contains(&"and "));
        assert!(cfg.operators.contains(&"or "));
        assert!(!cfg.operators.contains(&"&&"));
    }

    #[test]
    fn test_lang_config_for_cpp() {
        let cfg = lang_config_for("cpp").unwrap();
        assert_eq!(cfg.display_name, "C++");
        assert!(cfg.keywords.contains(&"switch "));
        assert!(cfg.keywords.contains(&"case "));
    }

    #[test]
    fn test_lang_config_for_csharp() {
        let cfg = lang_config_for("cs").unwrap();
        assert_eq!(cfg.display_name, "C#");
        // C# uses "if" without trailing space
        assert!(cfg.keywords.contains(&"if"));
    }

    #[test]
    fn test_lang_config_for_java() {
        let cfg = lang_config_for("java").unwrap();
        assert_eq!(cfg.display_name, "Java");
        assert!(cfg.keywords.contains(&"try"));
        assert!(cfg.keywords.contains(&"catch"));
    }

    #[test]
    fn test_lang_config_for_go() {
        let cfg = lang_config_for("go").unwrap();
        assert_eq!(cfg.display_name, "Go");
        assert!(cfg.keywords.contains(&"select "));
    }

    #[test]
    fn test_lang_config_for_php() {
        let cfg = lang_config_for("php").unwrap();
        assert_eq!(cfg.display_name, "PHP");
        assert!(cfg.keywords.contains(&"foreach"));
    }

    #[test]
    fn test_lang_config_for_swift() {
        let cfg = lang_config_for("swift").unwrap();
        assert_eq!(cfg.display_name, "Swift");
        assert!(cfg.keywords.contains(&"guard "));
    }

    #[test]
    fn test_lang_config_for_kotlin() {
        let cfg = lang_config_for("kt").unwrap();
        assert_eq!(cfg.display_name, "Kotlin");
        assert!(cfg.keywords.contains(&"when "));
    }

    #[test]
    fn test_lang_config_for_js() {
        let cfg = lang_config_for("js").unwrap();
        assert_eq!(cfg.display_name, "JavaScript/TypeScript");
    }

    #[test]
    fn test_lang_config_for_ts() {
        let cfg = lang_config_for("ts").unwrap();
        assert_eq!(cfg.display_name, "JavaScript/TypeScript");
    }

    #[test]
    fn test_lang_config_for_kts() {
        let cfg = lang_config_for("kts").unwrap();
        assert_eq!(cfg.display_name, "Kotlin");
    }

    #[test]
    fn test_lang_config_for_unknown_ext() {
        assert!(lang_config_for("xyz").is_none());
        assert!(lang_config_for("rs").is_none());
    }

    #[test]
    fn test_lang_config_for_c_and_h() {
        let cfg_c = lang_config_for("c").unwrap();
        let cfg_h = lang_config_for("h").unwrap();
        assert_eq!(cfg_c.display_name, "C");
        assert_eq!(cfg_h.display_name, "C");
    }

    // ── count_potential_mutations tests ──────

    #[test]
    fn test_count_potential_mutations_empty() {
        let cfg = lang_config_for("py").unwrap();
        assert_eq!(count_potential_mutations("", cfg), 0);
    }

    #[test]
    fn test_count_potential_mutations_python_and_or() {
        let cfg = lang_config_for("py").unwrap();
        // Python uses "and " / "or " instead of "&&" / "||"
        let src = "if x > 0 and y < 5:\n    pass\nif a or b:\n    pass\n";
        let count = count_potential_mutations(src, cfg);
        // operators: == x0, != x0, and x1, or x1 = 2
        // keywords: if x2, for x0, while x0 = 2
        // total = 4
        assert!(count >= 4, "expected >= 4, got {}", count);
    }

    #[test]
    fn test_count_potential_mutations_python_no_and_or() {
        let cfg = lang_config_for("py").unwrap();
        let src = "x = 1 + 2\n";
        let count = count_potential_mutations(src, cfg);
        // No "==", "!=", "and ", "or ", "if ", "for ", "while "
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_potential_mutations_go() {
        let cfg = lang_config_for("go").unwrap();
        let src = "if x > 0 {\n    switch y {\n    case 1:\n        select {}\n    }\n}\n";
        let count = count_potential_mutations(src, cfg);
        // operators: == x0, != x0, && x0, || x0 = 0
        // keywords: if x1, for x0, switch x1, case x1, select x1 = 4
        // total = 4
        assert!(count >= 4, "expected >= 4, got {}", count);
    }

    #[test]
    fn test_count_potential_mutations_csharp() {
        let cfg = lang_config_for("cs").unwrap();
        let src = "if (x == y) {\n    try { }\n    catch { }\n}\nwhile (true) { }\n";
        let count = count_potential_mutations(src, cfg);
        // operators: == x1, != x0, && x0, || x0 = 1
        // keywords: if x1, for x0, while x1, switch x0, try x1 = 3
        // total = 4
        // Note: C# keywords don't have trailing spaces
        assert!(count >= 4, "expected >= 4, got {}", count);
    }

    // ── replace_line ────────────────────────

    #[test]
    fn test_replace_line_basic() {
        let src = "line1\nline2\nline3\n";
        let result = replace_line(src, 2, "REPLACED");
        // .lines() strips trailing newline, .join("\n") doesn't add it back
        assert_eq!(result, "line1\nREPLACED\nline3");
    }

    #[test]
    fn test_replace_line_first_line() {
        let src = "first\nsecond\n";
        let result = replace_line(src, 1, "newfirst");
        assert_eq!(result, "newfirst\nsecond");
    }

    #[test]
    fn test_replace_line_last_line() {
        let src = "a\nb\nc";
        let result = replace_line(src, 3, "last");
        assert_eq!(result, "a\nb\nlast");
    }

    #[test]
    fn test_replace_line_out_of_bounds() {
        let src = "hello\n";
        let result = replace_line(src, 10, "x");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_replace_line_empty_source() {
        assert_eq!(replace_line("", 1, "x"), "");
    }

    // ── find_package_name ───────────────────

    #[test]
    fn test_find_package_name_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "my-crate"
version = "0.1.0"
"#,
        )
        .unwrap();
        let name = find_package_name(dir.path()).unwrap();
        assert_eq!(name, "my-crate");
    }

    #[test]
    fn test_find_package_name_no_toml() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_package_name(dir.path()).is_err());
    }

    #[test]
    fn test_find_package_name_no_package_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[workspace]
members = ["crates/*"]
"#,
        )
        .unwrap();
        assert!(find_package_name(dir.path()).is_err());
    }

    // ── find_workspace_root ─────────────────

    #[test]
    fn test_find_workspace_root_self() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[workspace]
"#,
        )
        .unwrap();
        let root = find_workspace_root(dir.path());
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_find_workspace_root_no_workspace_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[package]
name = "x"
"#,
        )
        .unwrap();
        let root = find_workspace_root(dir.path());
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_find_workspace_root_no_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = find_workspace_root(dir.path());
        // Falls back to crate_root itself
        #[allow(deprecated)]
        let expected = dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| dir.path().to_path_buf());
        assert_eq!(root, expected);
    }

    // ── find_first_workspace_member ─────────

    #[test]
    fn test_find_first_workspace_member_found() {
        let dir = tempfile::tempdir().unwrap();
        let crates_dir = dir.path().join("crates");
        std::fs::create_dir_all(&crates_dir).unwrap();
        let sub_crate = crates_dir.join("util");
        std::fs::create_dir_all(&sub_crate).unwrap();
        std::fs::write(
            sub_crate.join("Cargo.toml"),
            r#"[package]
name = "util-crate"
"#,
        )
        .unwrap();
        let name = find_first_workspace_member(dir.path()).unwrap();
        assert_eq!(name, "util-crate");
    }

    #[test]
    fn test_find_first_workspace_member_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_first_workspace_member(dir.path()).is_err());
    }

    // ── find_source_files / find_rs_files ───

    #[test]
    fn test_find_rs_files_in_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "fn x() {}").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("README.md"), "docs").unwrap();
        let mut files = Vec::new();
        find_rs_files(dir.path(), &mut files);
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(files.iter().any(|p| p.ends_with("main.rs")));
    }

    #[test]
    fn test_find_rs_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = Vec::new();
        find_rs_files(dir.path(), &mut files);
        assert!(files.is_empty());
    }

    #[test]
    fn test_find_rs_files_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("mod.rs"), "fn f() {}").unwrap();
        let mut files = Vec::new();
        find_rs_files(dir.path(), &mut files);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_find_source_files_with_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        // Only select a.rs
        let filter = Some("a.rs".to_string());
        let files = find_source_files(dir.path(), "x", &filter);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.rs"));
    }

    #[test]
    fn test_find_source_files_std_src_dir() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "").unwrap();
        let files = find_source_files(dir.path(), "x", &None);
        assert_eq!(files.len(), 1);
    }

    // ── home_target_dir ─────────────────────

    #[test]
    fn test_home_target_dir_from_env() {
        std::env::set_var("CARGO_TARGET_DIR", "/tmp/my-target");
        let dir = home_target_dir();
        assert_eq!(dir, std::path::PathBuf::from("/tmp/my-target"));
        std::env::remove_var("CARGO_TARGET_DIR");
    }

    #[test]
    fn test_home_target_dir_fallback() {
        std::env::remove_var("CARGO_TARGET_DIR");
        let dir = home_target_dir();
        // Falls back to a path (might be from current_exe or "target")
        assert!(!dir.as_os_str().is_empty());
    }

    // ── copy_dir_recursive_filtered ─────────

    #[test]
    fn test_copy_dir_recursive_filtered_basic() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap().path().join("copied");
        std::fs::write(src.path().join("file.txt"), "hello").unwrap();
        std::fs::create_dir_all(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("sub").join("nested.txt"), "nested").unwrap();

        copy_dir_recursive_filtered(src.path(), &dst).unwrap();

        assert!(dst.join("file.txt").exists());
        assert!(dst.join("sub").join("nested.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("file.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_copy_dir_recursive_filtered_skips_target_and_git() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap().path().join("copied");
        std::fs::create_dir_all(src.path().join("target")).unwrap();
        std::fs::write(src.path().join("target").join("big.bin"), "data").unwrap();
        std::fs::create_dir_all(src.path().join(".git")).unwrap();
        std::fs::write(src.path().join(".git").join("HEAD"), "ref").unwrap();
        std::fs::write(src.path().join("keep.txt"), "keep").unwrap();

        copy_dir_recursive_filtered(src.path(), &dst).unwrap();

        assert!(dst.join("keep.txt").exists());
        assert!(!dst.join("target").exists());
        assert!(!dst.join(".git").exists());
    }

    // ── format_mutation_analysis ────────────

    #[test]
    fn test_format_mutation_analysis_python() {
        let source = "if x > 0 and y < 0:\n    pass\n";
        let config = lang_config_for("py");
        let out = format_mutation_analysis("/tmp/test.py", source, config);

        assert!(out.contains("MUTATION ANALYSIS"));
        assert!(out.contains("Language: Python"));
        assert!(out.contains("File: /tmp/test.py"));
        assert!(out.contains("Potential mutation points: 2"));
        assert!(out.contains("Estimated test coverage needed: 4-6%"));
        assert!(out.contains("pip install cosmic-ray"));
    }

    #[test]
    fn test_format_mutation_analysis_unknown_language() {
        let source = "fn main() {}\n";
        let out = format_mutation_analysis("/tmp/test.rs", source, None);

        assert!(out.contains("Language: Unknown"));
        assert!(out.contains("Potential mutation points: 0"));
        assert!(out.contains("Estimated test coverage needed: 0-0%"));
    }

    #[test]
    fn test_format_mutation_analysis_empty_source() {
        let source = "";
        let config = lang_config_for("go");
        let out = format_mutation_analysis("main.go", source, config);

        assert!(out.contains("Language: Go"));
        assert!(out.contains("Potential mutation points: 0"));
    }

    #[test]
    fn test_format_mutation_analysis_no_newline_trailing() {
        let source = "if (x == y) { }";
        let config = lang_config_for("cs");
        let out = format_mutation_analysis("file.cs", source, config);

        assert!(out.contains("Language: C#"));
        // C# has == as operator, if/for/while/try as keywords
        assert!(out.contains("Potential mutation points: 2")); // == x1, if x1
    }

    // ── analyze_non_rust_file (thin wrapper smoke) ──

    #[test]
    fn test_analyze_non_rust_file_python() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.py");
        std::fs::write(&path, "if x > 0 and y < 0:\n    pass\n").unwrap();
        let result = analyze_non_rust_file(path.to_str().unwrap(), &Cli::parse_from(["test", "."]));
        assert!(result.is_ok());
    }

    #[test]
    fn test_analyze_non_rust_file_nonexistent() {
        let result = analyze_non_rust_file("/nonexistent/file.py", &Cli::parse_from(["test", "."]));
        assert!(result.is_err());
    }

    // ── build_mutation_report_json / format_mutation_report_json ──

    fn make_test_results() -> Vec<MutantResult> {
        vec![
            MutantResult {
                id: 1,
                file: "src/main.rs".to_string(),
                line: 42,
                description: "Replace '==' with '!='".to_string(),
                status: "killed".to_string(),
                test_output: "tests passed".to_string(),
            },
            MutantResult {
                id: 2,
                file: "src/lib.rs".to_string(),
                line: 10,
                description: "Replace 'true' with 'false'".to_string(),
                status: "survived".to_string(),
                test_output: "all tests passed".to_string(),
            },
        ]
    }

    #[test]
    fn test_build_mutation_report_json_structure() {
        let results = make_test_results();
        let value = build_mutation_report_json(&results, 2, 1, 1, 0, 0, 100);

        assert_eq!(value["tool"], "mutate");
        assert!(value["success"].as_bool().unwrap());
        assert_eq!(value["duration_ms"], 100);

        assert_eq!(value["data"]["results"].as_array().unwrap().len(), 2);
        assert_eq!(value["data"]["results"][0]["id"], 1);
        assert_eq!(value["data"]["results"][0]["status"], "killed");

        assert_eq!(value["data"]["summary"]["total_mutants"], 2);
        assert_eq!(value["data"]["summary"]["killed"], 1);
        assert_eq!(value["data"]["summary"]["survived"], 1);
        assert!((value["data"]["summary"]["mutation_score"].as_f64().unwrap() - 50.0).abs() < 1e-9);

        assert_eq!(value["summary"]["total_mutants"], 2);
        assert_eq!(value["summary"]["passed"], false);
    }

    #[test]
    fn test_build_mutation_report_json_all_passed() {
        let results = vec![MutantResult {
            id: 1,
            file: "a.rs".to_string(),
            line: 1,
            description: "test".to_string(),
            status: "killed".to_string(),
            test_output: "".to_string(),
        }];
        let value = build_mutation_report_json(&results, 1, 1, 0, 0, 0, 50);
        assert_eq!(value["summary"]["passed"], true);
        assert!(
            (value["data"]["summary"]["mutation_score"].as_f64().unwrap() - 100.0).abs() < 1e-9
        );
    }

    #[test]
    fn test_format_mutation_report_json_roundtrip() {
        let results = make_test_results();
        let value = build_mutation_report_json(&results, 2, 1, 1, 0, 0, 100);
        let json_str = format_mutation_report_json(&value);

        // Parses back and has expected content
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["tool"], "mutate");
        assert!(json_str.contains('\n')); // pretty-printed
    }

    // ── format_mutation_table ─────────────────

    #[test]
    fn test_format_mutation_table_with_survived() {
        let results = make_test_results();
        let table = format_mutation_table(&results, 2, 1, 1, 0, 0);

        assert!(table.contains("MUTATION TESTING RESULTS"));
        assert!(table.contains("SURVIVED MUTANTS"));
        assert!(table.contains("[2]")); // survived mutant id
        assert!(table.contains("src/lib.rs")); // survived mutant file
        assert!(table.contains("50%")); // mutation score
        assert!(table.contains("Weak")); // score 50% = "Weak"
        assert!(table.contains("1 mutant(s) survived"));
    }

    #[test]
    fn test_format_mutation_table_all_killed() {
        let results = vec![MutantResult {
            id: 1,
            file: "a.rs".to_string(),
            line: 1,
            description: "test".to_string(),
            status: "killed".to_string(),
            test_output: "".to_string(),
        }];
        let table = format_mutation_table(&results, 1, 1, 0, 0, 0);

        assert!(table.contains("MUTATION TESTING RESULTS"));
        assert!(!table.contains("SURVIVED MUTANTS")); // no survived section
        assert!(table.contains("100%")); // mutation score
        assert!(table.contains("Excellent")); // score 100% = "Excellent"
        assert!(!table.contains("survived. Your tests")); // no survived message
    }

    #[test]
    fn test_format_mutation_table_zero_mutants() {
        let table = format_mutation_table(&[], 0, 0, 0, 0, 0);
        assert!(table.contains("MUTATION TESTING RESULTS"));
        assert!(table.contains("0%")); // mutation score
        assert!(table.contains("Poor")); // score 0% = "Poor"
    }

    // ── resolve_crate_root ─────────────────

    #[test]
    fn test_resolve_crate_root_valid_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_crate_root(dir.path().to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_crate_root_nonexistent() {
        let result = resolve_crate_root("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Cannot resolve path"));
        assert!(err.contains("/nonexistent/path/that/does/not/exist"));
    }

    // ── auto_detect_package_name ────────────

    #[test]
    fn test_auto_detect_package_name_uses_cli_flag() {
        let dir = tempfile::tempdir().unwrap();
        // cli_package overrides everything — even with no Cargo.toml
        let result = auto_detect_package_name(dir.path(), &Some("my-pkg".to_string()));
        assert_eq!(result.unwrap(), "my-pkg");
    }

    #[test]
    fn test_auto_detect_package_name_falls_back_to_find_package_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[package]
name = "detected-crate"
"#,
        )
        .unwrap();
        let result = auto_detect_package_name(dir.path(), &None);
        assert_eq!(result.unwrap(), "detected-crate");
    }

    #[test]
    fn test_auto_detect_package_name_falls_back_to_workspace_member() {
        let dir = tempfile::tempdir().unwrap();
        // No [package] in root Cargo.toml, but crates/ has one
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[workspace]
members = ["crates/*"]
"#,
        )
        .unwrap();
        let crates_dir = dir.path().join("crates");
        std::fs::create_dir_all(&crates_dir).unwrap();
        let sub_crate = crates_dir.join("util");
        std::fs::create_dir_all(&sub_crate).unwrap();
        std::fs::write(
            sub_crate.join("Cargo.toml"),
            r#"[package]
name = "util-crate"
"#,
        )
        .unwrap();
        let result = auto_detect_package_name(dir.path(), &None);
        assert_eq!(result.unwrap(), "util-crate");
    }

    #[test]
    fn test_auto_detect_package_name_both_fail() {
        let dir = tempfile::tempdir().unwrap();
        // Empty directory — no Cargo.toml at all
        let result = auto_detect_package_name(dir.path(), &None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Could not auto-detect package name"));
        assert!(err.contains("Use -p/--package flag"));
    }
}
