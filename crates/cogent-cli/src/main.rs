#![deny(clippy::all)]

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::time::Instant;


mod audit;
mod check_runners;
mod checks_cmd;
mod commands;
mod config;
mod diff;
mod history;
mod hooks;
mod progress;
mod report;
mod report_formatters;
mod serve;
mod types;
mod watch;

use check_runners::*;
use commands::{discover_command, explain_command, init_ci};
use diff::diff_command;
use report::{render_html_report, report_command, setup_command};
use report_formatters::*;
use serve::serve_command;
use watch::watch_mode;
use config::{detect_project, generate_config, load_config_thresholds};
use types::{
    aggregate_file_summary, CheckReport, CheckResult, CheckSummary,
    Evidence, Finding, SuggestedFix,
};
use history::history_command;
use hooks::{install_hooks, uninstall_hooks};
use progress::{
    format_elapsed, health_score, print_fix_summary,
    print_offenders, print_severity_grouped, print_summary_box, run_with_spinner,
};

// ═══════════════════════════════════════════
// CLI DEFINITION
// ═══════════════════════════════════════════

#[derive(Parser)]
#[command(
    name = "cogent",
    about = "Unified code quality tool for Rust. Headless-first, JSON output, CI-ready.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum PolicyAction {
    /// Validate all .cogent-policies/*.yaml files
    Validate,
    /// Run checks defined in policies only
    Check {
        /// Path to analyze
        path: String,
        /// Output format: json (default), text
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Force run even without .quality.toml
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ExceptionAction {
    /// Propose a new exception
    Add {
        /// Finding ID to suppress
        #[arg(long)]
        finding_id: String,
        /// Rule ID that triggered the finding
        #[arg(long)]
        rule_id: String,
        /// File path containing the finding
        #[arg(long)]
        file: String,
        /// Reason for the exception
        #[arg(long)]
        reason: String,
        /// Reviewer who must approve
        #[arg(long)]
        reviewer: String,
    },
    /// List exceptions
    List {
        /// Filter by status: pending, approved, revoked
        #[arg(long)]
        status: Option<String>,
    },
    /// Approve a pending exception
    Approve {
        /// Exception ID
        id: String,
    },
    /// Revoke an approved exception
    Revoke {
        /// Exception ID
        id: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Run all Cogent checks, compute a 0-100 health score, and report pass/fail
    #[command(
        after_help = "Example: cogent check . --format text → runs 20+ checks with live progress.\nUse --ci for JSON output suitable for CI pipelines.\nRequires .quality.toml (run cogent init first) or use --force for defaults."
    )]
    Check {
        /// Path to analyze
        path: String,

        /// Recursive scan
        #[arg(short, long)]
        recursive: bool,

        /// Output format: json (default), text, sarif, junit, findings, ndjson, or markdown
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Path to lcov coverage file
        #[arg(long)]
        coverage: Option<String>,

        /// Max average CRAP score (fail if exceeded)
        #[arg(long, default_value = "30")]
        max_crap: f64,

        /// Min doc coverage percentage (fail if below)
        #[arg(long, default_value = "50")]
        min_doc: f64,

        /// Max technical debt markers (fail if exceeded)
        #[arg(long, default_value = "100")]
        max_debt: usize,

        /// Max number of functions with complexity >= 10 allowed before failing (default: 0 = strict)
        #[arg(long, default_value = "0")]
        max_complexity_violations: usize,

        /// Max taint violations (default: 0)
        #[arg(long, default_value = "0")]
        max_taint: usize,

        /// Max code duplication percentage (default: 5.0)
        #[arg(long, default_value = "5.0")]
        max_duplication: f64,

        /// Max allowed file risk score (default: 10.0)
        #[arg(long, default_value = "10.0")]
        max_risk: f64,

        /// Max allowed architectural coupling issues (default: 5)
        #[arg(long, default_value = "5")]
        max_coupling: usize,

        /// Min property test coverage percentage (default: 0.0)
        #[arg(long, default_value = "0.0")]
        min_propcov: f64,

        /// Max unprotected fuzzable endpoints (default: 0)
        #[arg(long, default_value = "0")]
        max_fuzz_risk: usize,

        /// Max functions/files exceeding line length limits (default: 0)
        #[arg(long, default_value = "0")]
        max_linelen: usize,

        /// Max estimated bugs from Halstead metrics per file (default: 2.0)
        #[arg(long, default_value = "2.0")]
        max_halstead_bugs: f64,

        /// Max hardcoded secret findings (default: 0)
        #[arg(long, default_value = "0")]
        max_secrets: usize,

        /// Max dead code findings (default: 10)
        #[arg(long, default_value = "10")]
        max_deadcode: usize,

        /// Max LCOM4 cohesion violations (default: 5)
        #[arg(long, default_value = "5")]
        max_cohesion: usize,

        /// Minimum comment ratio 0.0–1.0 (default: 0.05 = 5%)
        #[arg(long, default_value = "0.05")]
        min_comment_ratio: f64,

        /// Max error handling violations (unwrap/expect/panic/discard, default: 50)
        #[arg(long, default_value = "50")]
        max_errhandle: usize,

        /// Minimum type annotation coverage % for Python/JS/TS (default: 0 = off)
        #[arg(long, default_value = "0.0")]
        min_typecov: f64,

        /// Max critical CVEs from dependency scan (default: 0)
        #[arg(long, default_value = "0")]
        max_vuln_critical: usize,

        /// Max high CVEs from dependency scan (default: 0)
        #[arg(long, default_value = "0")]
        max_vuln_high: usize,

        /// Max SAST findings — SQL injection, XSS, path traversal, cmd injection (default: 0)
        #[arg(long, default_value = "0")]
        max_sast: usize,

        /// Max crypto findings — weak hash, insecure random, ECB, disabled TLS (default: 0)
        #[arg(long, default_value = "0")]
        max_crypto: usize,

        /// Max OSS license violations (default: 0)
        #[arg(long, default_value = "0")]
        max_license_violations: usize,

        /// Max direct dependencies that are a full major version behind latest (default: 0, requires cargo-outdated)
        #[arg(long, default_value = "0")]
        max_outdated: usize,

        /// Skip specific checks (comma-separated: crap,debt,doc,dup,complexity,taint,risk,coupling,propcov,fuzz,linelen,halstead,secrets,deadcode,cohesion,comments,errhandle,typecov,vulnscan,sast,crypto,licenses)
        #[arg(long)]
        skip: Option<String>,

        /// Run only these checks (comma-separated); takes precedence over --skip
        #[arg(long)]
        only: Option<String>,

        /// CI mode: JSON output, no TTY colors or progress (equivalent to --format json + COGENT_NO_PROGRESS=1)
        #[arg(long)]
        ci: bool,

        /// Show top offenders (file:line) for every check, not just failed ones
        #[arg(long)]
        verbose: bool,

        /// Emit a markdown snippet suitable for posting as a GitHub PR comment
        #[arg(long)]
        pr_comment: bool,

        /// Run checks even when .quality.toml is missing (uses hardcoded defaults)
        #[arg(long)]
        force: bool,
    },

    /// Verify environment dependencies (doctor)
    Setup,

    /// CRAP metric — measures change-risk by combining complexity and test coverage
    #[command(
        after_help = "Example: cogent crap ./src --format json → lists functions with high CRAP scores. High CRAP = complex + untested = dangerous to change."
    )]
    Crap {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long)]
        coverage: Option<String>,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Technical debt — finds TODO, FIXME, HACK, and XXX markers in source code
    #[command(
        after_help = "Example: cogent debt ./src --format json → finds all debt markers with file:line. Each marker is a known unaddressed issue."
    )]
    Debt {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long)]
        marker: Option<String>,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Documentation coverage — checks what percentage of public functions have doc comments
    #[command(
        after_help = "Example: cogent doccov ./src --format json → shows % coverage and which public items are missing docs."
    )]
    Doccov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Code duplication — finds identical or near-identical code blocks
    #[command(
        after_help = "Example: cogent dupfind ./src --format json → groups clones by similarity. Duplicated code doubles maintenance cost."
    )]
    Dupfind {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        min_lines: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cyclomatic complexity — counts independent paths through each function
    #[command(
        after_help = "Example: cogent complexity ./src --format json → lists functions with complexity >= 10. Higher complexity = harder to test and more bug-prone."
    )]
    Complexity {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        min_complexity: u32,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Generate default config file
    Init {
        /// Output path (default: .quality.toml)
        #[arg(long, default_value = ".quality.toml")]
        output: String,

        /// Full CI bootstrap: also writes GitHub Actions workflow, installs pre-commit hook, seeds baseline, records history
        #[arg(long)]
        ci: bool,
    },

    /// Explain what a Cogent tool measures, how to read its output, and how to fix common findings
    Explain {
        /// Tool name: crap, debt, doccov, complexity, taint, dup, risk, coupling, propcov, fuzz, linelen, halstead, secrets, deadcode, cohesion, comments, errhandle, typecov, vulnscan, sast, crypto, licenses, mutate, access-control, supply-chain, outdated
        tool: String,
    },

    /// Run all Cogent tools in batch mode using .quality.toml config
    Run {
        /// Path to the crate root (directory with Cargo.toml)
        path: String,

        /// Config file (default: .quality.toml)
        #[arg(long, default_value = ".quality.toml")]
        config: String,

        /// Output format (table, json, or sarif)
        #[arg(short, long, default_value = "table")]
        format: String,

        /// Baseline SARIF/JSON file: only emit new/regressed results
        #[arg(long)]
        baseline: Option<String>,

        /// Do not exit 1 on baseline regression (useful for seeding a new baseline)
        #[arg(long)]
        no_fail_on_regression: bool,
    },

    /// Record or display Cogent history
    History {
        /// Action: record (append current run to history) or show (print trend table)
        #[arg(default_value = "show")]
        action: String,

        /// History directory (default: .quality-history)
        #[arg(long, default_value = ".cogent-history")]
        dir: String,

        /// Number of recent runs to show
        #[arg(long, default_value = "10")]
        last: usize,

        /// Path to a JSON run report to record (default: stdin)
        #[arg(long)]
        report: Option<String>,

        /// Output format: text (default) or html
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Install a Cogent pre-commit git hook
    InstallHooks {
        /// Git repo root (default: current directory)
        #[arg(default_value = ".")]
        repo: String,

        /// Install a lightweight hook that skips test execution (metrics only)
        #[arg(long)]
        fast: bool,
    },

    /// Remove the Cogent pre-commit git hook
    UninstallHooks {
        /// Git repo root (default: current directory)
        #[arg(default_value = ".")]
        repo: String,
    },

    /// Watch for file changes and re-run relevant checks
    Watch {
        /// Path to watch
        #[arg(default_value = ".")]
        path: String,

        /// Which checks to run on change (comma-separated: crap,debt,doc,complexity)
        #[arg(long, default_value = "debt,doc,crap")]
        checks: String,

        /// Debounce delay in milliseconds
        #[arg(long, default_value = "500")]
        debounce_ms: u64,

        /// Skip running tests and coverage collection (metrics-only mode)
        #[arg(long)]
        no_tests: bool,

        /// Run all available checks every cycle (equivalent to cogent check)
        #[arg(long)]
        full: bool,
    },

    /// Discover available Cogent tools and their capabilities
    Discover {
        /// Output format: json (default) or text
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Generate a human-readable audit report (HTML or Markdown) from a check run
    Report {
        /// Path to audit (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Output format: html (default) or markdown
        #[arg(short, long, default_value = "html")]
        format: String,

        /// Output file (default: cogent-report.html or cogent-report.md)
        #[arg(short, long)]
        output: Option<String>,

        /// Project name shown in the report header
        #[arg(long)]
        project: Option<String>,

        /// Optional: path to existing JSON check output (skips re-running checks)
        #[arg(long)]
        from_json: Option<String>,

        /// Skip vulnscan check (faster; use when cargo audit is slow)
        #[arg(long)]
        skip: Option<String>,

        /// Open the report in the default browser after writing
        #[arg(long)]
        open: bool,
    },

    /// Compare two check JSON snapshots and show regressions or improvements
    Diff {
        /// Path to the older check JSON snapshot
        before: String,

        /// Path to the newer check JSON snapshot
        after: String,

        /// Output format: text (default) or html
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Taint analysis only
    Taint {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_taint: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Coupling analysis only
    Coupling {
        path: String,
        #[arg(long, default_value = "5")]
        max_coupling: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Risk map (churn x complexity) only
    Riskmap {
        path: String,
        #[arg(long, default_value = "10.0")]
        max_risk: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Mutation testing only
    Mutate {
        path: String,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Fuzz surface analysis only
    Fuzz {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_fuzz_risk: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Property test coverage only
    Propcov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.0")]
        min_propcov: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Line length check only
    Linelen {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Halstead complexity metrics only
    Halstead {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "2.0")]
        max_bugs: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Secret detection only
    Secrets {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Dead code detection only
    Deadcode {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "10")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cohesion (LCOM4) analysis only
    Cohesion {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "5")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Comment ratio analysis only
    Comments {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.05")]
        min_ratio: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Error handling checker only
    Errhandle {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "50")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Type annotation coverage only
    Typecov {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0.0")]
        min_pct: f64,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Vulnerability scan only
    Vulnscan {
        path: String,
        #[arg(long, default_value = "0")]
        max_critical: usize,
        #[arg(long, default_value = "0")]
        max_high: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// SAST scanner only
    Sast {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Cryptography checker only
    Crypto {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_findings: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// License compliance only
    Licenses {
        path: String,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Outdated dependency checker only
    Outdated {
        path: String,
        #[arg(long, default_value = "0")]
        max_major_behind: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Access control checker only
    AccessControl {
        path: String,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long, default_value = "0")]
        max_violations: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Supply chain checker only
    SupplyChain {
        path: String,
        #[arg(long, default_value = "0")]
        max_risks: usize,
        #[arg(short, long, default_value = "json")]
        format: String,
    },

    /// Start a local HTTP server to browse reports
    Serve {
        /// Port to listen on (default: 8080)
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Directory containing historical reports (default: .cogent-history)
        #[arg(long, default_value = ".cogent-history")]
        history_dir: String,
    },

    /// Generate shell completion scripts for bash, zsh, fish, or powershell
    #[command(after_help = "Example: cogent completions bash > /etc/bash_completion.d/cogent")]
    Completions {
        /// Shell to generate completions for: bash, zsh, fish, or powershell
        shell: String,
    },

    /// Validate or run custom audit policies defined in .cogent-policies/
    #[command(
        after_help = "Example: cogent policy validate → checks all policy files for errors.
cogent policy check . → runs only the tools defined in policies."
    )]
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    /// Manage approved exceptions (false positive overrides)
    #[command(
        after_help = "Example: cogent exception add --finding-id SAST-001 --reason 'sanitized upstream' --reviewer alice"
    )]
    Exception {
        #[command(subcommand)]
        action: ExceptionAction,
    },

    /// Track remediation status of findings
    #[command(
        after_help = "Example: cogent remediate --verify → checks which previous findings are now fixed."
    )]
    Remediate {
        /// Verify closed findings against current scan
        #[arg(long)]
        verify: bool,
        /// Path to analyze
        path: String,
    },

    /// Query and verify the signed audit trail
    #[command(after_help = "Example: cogent audit-trail --verify → checks trail integrity.")]
    AuditTrail {
        /// Verify signatures (detect tampering)
        #[arg(long)]
        verify: bool,
        /// Filter by command
        #[arg(long)]
        command: Option<String>,
        /// Show entries since this ISO date
        #[arg(long)]
        since: Option<String>,
    },

    /// Full audit: runs all checks, enriches findings with fixes + compliance controls, writes audit trail
    #[command(
        after_help = "Example: cogent audit . --format agent | jq\ncogent audit . --format agent --evidence --framework soc2"
    )]
    Audit {
        /// Path to analyze
        path: String,

        /// Output format: agent (NDJSON, one finding per line), json (full report), markdown
        #[arg(short, long, default_value = "agent")]
        format: String,

        /// Only run a category: security, quality, compliance (default: all)
        #[arg(long)]
        only: Option<String>,

        /// Attach code snippets, file hashes, and git blame to each finding
        #[arg(long)]
        evidence: bool,

        /// Map findings to compliance framework controls: soc2, iso27001, both
        #[arg(long)]
        framework: Option<String>,

        /// Run only these checks (comma-separated)
        #[arg(long)]
        checks: Option<String>,

        /// Skip these checks (comma-separated)
        #[arg(long)]
        skip: Option<String>,

        /// CI mode: suppress TTY output, exit 1 on any finding
        #[arg(long)]
        ci: bool,

        /// After scanning, auto-close remediation entries whose findings are no longer present
        #[arg(long)]
        verify: bool,
    },
}


// ═══════════════════════════════════════════

// ═══════════════════════════════════════════
// OUTPUT FORMATTERS
// ═══════════════════════════════════════════

fn output_json(report: &CheckReport) {
    println!("{}", serde_json::to_string_pretty(report).unwrap());
}




// MAIN
// ═══════════════════════════════════════════

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Check {
            path,
            recursive,
            format,
            coverage,
            max_crap,
            min_doc,
            max_debt,
            max_complexity_violations,
            max_taint,
            max_duplication,
            max_risk,
            max_coupling,
            min_propcov,
            max_fuzz_risk,
            max_linelen,
            max_halstead_bugs,
            max_secrets,
            max_deadcode,
            max_cohesion,
            min_comment_ratio,
            max_errhandle,
            min_typecov,
            max_vuln_critical,
            max_vuln_high,
            max_sast,
            max_crypto,
            max_license_violations,
            max_outdated,
            skip,
            only,
            ci,
            verbose,
            pr_comment,
            force,
        } => {
            // --ci: force JSON output and suppress progress (no TTY)
            let format = if ci { "json".to_string() } else { format };
            if ci {
                std::env::set_var("COGENT_NO_PROGRESS", "1");
            }

            // Guard: require .quality.toml unless --force is used
            if !force && !std::path::Path::new(".quality.toml").exists() {
                eprintln!();
                eprintln!(
                    "  {} No {} found.",
                    "!".yellow().bold(),
                    ".quality.toml".cyan()
                );
                eprintln!(
                    "    {} Run {} to auto-detect your project and generate one.",
                    "→".cyan(),
                    "cogent init".cyan().bold()
                );
                eprintln!();
                eprintln!(
                    "    {} Use {} to run with hardcoded defaults anyway.",
                    "→".cyan(),
                    "cogent check . --force".cyan().bold()
                );
                eprintln!();
                std::process::exit(2);
            }

            // Auto-load .quality.toml if present; CLI flags override file values.
            let (
                max_crap,
                min_doc,
                max_debt,
                max_complexity_violations,
                max_duplication,
                max_taint,
                max_risk,
                max_coupling,
                min_propcov,
                max_fuzz_risk,
                max_linelen,
                max_halstead_bugs,
                max_secrets,
                max_deadcode,
                max_cohesion,
                min_comment_ratio,
                max_errhandle,
                min_typecov,
                max_vuln_critical,
                max_vuln_high,
                max_sast,
                max_crypto,
                max_license_violations,
            ) = load_config_thresholds(
                ".quality.toml",
                (
                    max_crap,
                    min_doc,
                    max_debt,
                    max_complexity_violations,
                    max_duplication,
                    max_taint,
                    max_risk,
                    max_coupling,
                    min_propcov,
                    max_fuzz_risk,
                    max_linelen,
                    max_halstead_bugs,
                    max_secrets,
                    max_deadcode,
                    max_cohesion,
                    min_comment_ratio,
                    max_errhandle,
                    min_typecov,
                    max_vuln_critical,
                    max_vuln_high,
                    max_sast,
                    max_crypto,
                    max_license_violations,
                ),
            );

            let skip_list: Vec<String> = skip
                .map(|s| s.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default();

            // --only builds an explicit allowlist; if set, only those names run
            let only_list: Vec<String> = only
                .map(|s| s.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default();

            let should_run = |name: &str| -> bool {
                if !only_list.is_empty() {
                    only_list.contains(&name.to_string())
                } else {
                    !skip_list.contains(&name.to_string())
                }
            };

            let check_start = Instant::now();
            let show_progress = format == "text";

            // Helper: run a check with a live spinner on text format
            macro_rules! run_check {
                ($label:expr, $expr:expr) => {{
                    if show_progress {
                        let label = $label;
                        let t = Instant::now();
                        let result = run_with_spinner(label, || $expr);
                        let elapsed = format_elapsed(t.elapsed());
                        let detail = &result.message;
                        let icon = if result.passed {
                            "✓".green().bold()
                        } else {
                            "✗".red().bold()
                        };
                        let name_col = if result.passed {
                            label.normal()
                        } else {
                            label.red()
                        };
                        let msg_col = if result.passed {
                            detail.bright_black()
                        } else {
                            detail.red()
                        };
                        eprintln!(
                            "  {} {:<18} {}  {}",
                            icon,
                            name_col,
                            elapsed.bright_black(),
                            msg_col
                        );
                        if !result.passed || verbose {
                            print_offenders(&result);
                        }
                        result
                    } else {
                        $expr
                    }
                }};
            }

            let mut checks = Vec::new();

            if should_run("crap") {
                checks.push(run_check!(
                    "crap",
                    check_crap(&path, recursive, &coverage, max_crap)
                ));
            }
            if should_run("debt") {
                checks.push(run_check!("debt", check_debt(&path, recursive, max_debt)));
            }
            if should_run("doc") {
                checks.push(run_check!(
                    "doc_coverage",
                    check_doc_coverage(&path, recursive, min_doc)
                ));
            }
            if should_run("complexity") {
                checks.push(run_check!(
                    "complexity",
                    check_complexity(&path, recursive, 10, max_complexity_violations)
                ));
            }
            if should_run("taint") {
                checks.push(run_check!(
                    "taint",
                    check_taint(&path, recursive, max_taint)
                ));
            }
            if should_run("dup") || should_run("dupfind") || should_run("duplication") {
                checks.push(run_check!(
                    "duplication",
                    check_dupfind(&path, recursive, max_duplication)
                ));
            }
            if should_run("risk") || should_run("riskmap") {
                checks.push(run_check!(
                    "riskmap",
                    check_riskmap(&path, recursive, max_risk)
                ));
            }
            if should_run("coupling") {
                checks.push(run_check!("coupling", check_coupling(&path, max_coupling)));
            }
            if should_run("propcov") {
                checks.push(run_check!(
                    "propcov",
                    check_propcov(&path, recursive, min_propcov)
                ));
            }
            if should_run("fuzz") {
                checks.push(run_check!(
                    "fuzz",
                    check_fuzz(&path, recursive, max_fuzz_risk)
                ));
            }
            if should_run("linelen") {
                checks.push(run_check!(
                    "linelen",
                    check_linelen(&path, recursive, max_linelen)
                ));
            }
            if should_run("halstead") {
                checks.push(run_check!(
                    "halstead",
                    check_halstead(&path, recursive, max_halstead_bugs)
                ));
            }
            if should_run("secrets") {
                checks.push(run_check!(
                    "secrets",
                    check_secrets(&path, recursive, max_secrets)
                ));
            }
            if should_run("deadcode") {
                checks.push(run_check!(
                    "deadcode",
                    check_deadcode(&path, recursive, max_deadcode)
                ));
            }
            if should_run("cohesion") {
                checks.push(run_check!(
                    "cohesion",
                    check_cohesion(&path, recursive, max_cohesion)
                ));
            }
            if should_run("comments") {
                checks.push(run_check!(
                    "comments",
                    check_comments(&path, recursive, min_comment_ratio)
                ));
            }
            if should_run("errhandle") {
                checks.push(run_check!(
                    "errhandle",
                    check_errhandle(&path, recursive, max_errhandle)
                ));
            }
            if should_run("typecov") && min_typecov > 0.0 {
                checks.push(run_check!(
                    "typecov",
                    check_typecov(&path, recursive, min_typecov)
                ));
            }
            if should_run("vulnscan") {
                checks.push(run_check!(
                    "vulnscan",
                    check_vulnscan(&path, max_vuln_critical, max_vuln_high)
                ));
            }
            if should_run("sast") {
                checks.push(run_check!("sast", check_sast(&path, recursive, max_sast)));
            }
            if should_run("crypto") {
                checks.push(run_check!(
                    "crypto",
                    check_crypto(&path, recursive, max_crypto)
                ));
            }
            if should_run("licenses") {
                checks.push(run_check!(
                    "licenses",
                    check_licenses(&path, max_license_violations)
                ));
            }
            if should_run("mutate") {
                checks.push(run_check!("mutate", {
                    let args = vec![&path, "--format", "json"];
                    let res = run_tool("mutation-test", "mutate", &args, Instant::now());
                    let score = res
                        .data
                        .get("summary")
                        .and_then(|s| s.get("kill_rate"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let min_kill_rate = std::fs::read_to_string(".quality.toml")
                        .ok()
                        .and_then(|content| {
                            content.lines().find_map(|line| {
                                let line = line.trim();
                                if line.starts_with("min_kill_rate") {
                                    line.split('=').nth(1)?.trim().parse::<f64>().ok()
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(0.0);
                    let passed = score >= min_kill_rate;
                    let msg = if passed {
                        format!("Mutation testing passed (kill rate {:.1}%)", score)
                    } else {
                        format!(
                            "Mutation testing failed (kill rate {:.1}% < {:.0}%)",
                            score, min_kill_rate
                        )
                    };
                    CheckResult {
                        name: "mutate".into(),
                        passed,
                        score: Some(score),
                        threshold: Some(min_kill_rate),
                        message: msg,
                        details: serde_json::json!({}),
                        severity: None,
                        help: None,
                        rule_id: None,
                        findings: Vec::new(),
                    }
                }));
            }
            if should_run("access-control") {
                checks.push(run_check!("access-control", {
                    check_access_control(&path, true, 0)
                }));
            }

            if should_run("supply-chain") {
                checks.push(run_check!("supply-chain", check_supply_chain(&path, 0)));
            }

            if should_run("outdated") {
                checks.push(run_check!("outdated", check_outdated(&path, max_outdated)));
            }

            let passed = checks.iter().all(|c| c.passed);
            let total_funcs: usize = checks
                .iter()
                .filter_map(|c| c.details.get("total_functions").and_then(|v| v.as_u64()))
                .map(|v| v as usize)
                .sum();

            let passed_count = checks.iter().filter(|c| c.passed).count();
            let failed_count = checks.len() - passed_count;
            let total_checks = checks.len();

            let report = CheckReport {
                passed,
                path: path.clone(),
                checks: checks.clone(),
                summary: CheckSummary {
                    total_checks,
                    passed_checks: passed_count,
                    failed_checks: failed_count,
                    functions_analyzed: total_funcs,
                    avg_complexity: 0.0,
                    avg_crap: 0.0,
                },
                file_summary: aggregate_file_summary(&checks),
            };
            let (health, grade) = health_score(&report.checks);

            if pr_comment {
                let md = pr_comment_md(&report, &path);
                println!("{}", md);
                std::process::exit(if passed { 0 } else { 1 });
            }

            match format.as_str() {
                "text" => {
                    print_summary_box(
                        "COGENT CHECK",
                        passed,
                        &path,
                        passed_count,
                        total_checks,
                        check_start.elapsed(),
                        &report.checks,
                    );
                    if !passed {
                        print_severity_grouped(&report.checks);
                        print_fix_summary(&report.checks);
                    }
                }
                "ndjson" => output_ndjson(&report),
                "sarif" => output_sarif(&report),
                "junit" => output_junit(&report),
                "findings" => output_findings_ndjson(&report),
                "markdown" => output_markdown(&report, &path),
                _ => output_json(&report),
            }

            // CI artifact generation
            if ci {
                let summary = serde_json::json!({
                    "passed": report.passed,
                    "score": health,
                    "grade": grade.to_string(),
                    "failed_checks": report.checks.iter().filter(|c| !c.passed).map(|c| c.name.clone()).collect::<Vec<_>>(),
                    "critical_findings": report.checks.iter().map(|c| c.findings.iter().filter(|f| f.severity == "critical").count()).sum::<usize>(),
                    "report_url": "./cogent-report.html",
                });
                if let Err(e) = std::fs::write(
                    "cogent-summary.json",
                    serde_json::to_string_pretty(&summary).unwrap_or_default(),
                ) {
                    eprintln!("Warning: could not write cogent-summary.json: {}", e);
                }
                // Also write full HTML report to disk
                let html = render_html_report(
                    &report,
                    &path,
                    &chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
                    &[
                        "taint",
                        "secrets",
                        "sast",
                        "crypto",
                        "vulnscan",
                        "access-control",
                    ],
                    &[
                        "crap",
                        "debt",
                        "doc_coverage",
                        "complexity",
                        "duplication",
                        "riskmap",
                        "coupling",
                        "propcov",
                        "fuzz",
                        "linelen",
                        "halstead",
                        "deadcode",
                        "cohesion",
                        "comments",
                        "errhandle",
                        "typecov",
                    ],
                    &["licenses", "outdated", "supply-chain"],
                );
                if let Err(e) = std::fs::write("cogent-report.html", html) {
                    eprintln!("Warning: could not write cogent-report.html: {}", e);
                }
            }

            let total_findings: usize = report.checks.iter().map(|c| c.findings.len()).sum();
            audit::append_audit_trail("check", &path, total_findings, check_start.elapsed());

            if passed {
                0
            } else {
                1
            }
        }

        Commands::Crap {
            path,
            recursive,
            coverage,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("crap", || check_crap(&path, recursive, &coverage, 30.0))
            } else {
                check_crap(&path, recursive, &coverage, 30.0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} crap  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Debt {
            path,
            recursive,
            marker: _,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("debt", || check_debt(&path, recursive, 1000))
            } else {
                check_debt(&path, recursive, 1000)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} debt  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Doccov {
            path,
            recursive,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("doccov", || check_doc_coverage(&path, recursive, 0.0))
            } else {
                check_doc_coverage(&path, recursive, 0.0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} doccov  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Dupfind {
            path,
            recursive,
            min_lines,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("dupfind", || {
                    check_dupfind(&path, recursive, min_lines as f64)
                })
            } else {
                check_dupfind(&path, recursive, min_lines as f64)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Complexity {
            path,
            recursive,
            min_complexity,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("complexity", || {
                    check_complexity(&path, recursive, min_complexity, 0)
                })
            } else {
                check_complexity(&path, recursive, min_complexity, 0)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    let icon = if passed {
                        "✓".green().bold()
                    } else {
                        "✗".red().bold()
                    };
                    eprintln!("  {} complexity  {}", icon, result.message.bright_black());
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Taint {
            path,
            recursive,
            max_taint,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("taint", || check_taint(&path, recursive, max_taint))
            } else {
                check_taint(&path, recursive, max_taint)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Coupling {
            path,
            max_coupling,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("coupling", || check_coupling(&path, max_coupling))
            } else {
                check_coupling(&path, max_coupling)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Riskmap {
            path,
            max_risk,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("riskmap", || check_riskmap(&path, false, max_risk))
            } else {
                check_riskmap(&path, false, max_risk)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Mutate { path, format } => {
            let args = vec![&path, "--format", "json"];
            let res = run_tool("mutation-test", "mutate", &args, Instant::now());
            let passed = res.success;
            let msg = res.error.unwrap_or_else(|| {
                res.data
                    .get("summary")
                    .and_then(|s| s.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });
            match format.as_str() {
                "text" => {
                    println!("{}", msg);
                }
                _ => println!("{}", serde_json::to_string_pretty(&res.data).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Fuzz {
            path,
            recursive,
            max_fuzz_risk,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("fuzz", || check_fuzz(&path, recursive, max_fuzz_risk))
            } else {
                check_fuzz(&path, recursive, max_fuzz_risk)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Propcov {
            path,
            recursive,
            min_propcov,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("propcov", || check_propcov(&path, recursive, min_propcov))
            } else {
                check_propcov(&path, recursive, min_propcov)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Linelen {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("linelen", || {
                    check_linelen(&path, recursive, max_violations)
                })
            } else {
                check_linelen(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Halstead {
            path,
            recursive,
            max_bugs,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("halstead", || check_halstead(&path, recursive, max_bugs))
            } else {
                check_halstead(&path, recursive, max_bugs)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Secrets {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("secrets", || check_secrets(&path, recursive, max_findings))
            } else {
                check_secrets(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Deadcode {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("deadcode", || {
                    check_deadcode(&path, recursive, max_findings)
                })
            } else {
                check_deadcode(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Cohesion {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("cohesion", || {
                    check_cohesion(&path, recursive, max_violations)
                })
            } else {
                check_cohesion(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Comments {
            path,
            recursive,
            min_ratio,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("comments", || check_comments(&path, recursive, min_ratio))
            } else {
                check_comments(&path, recursive, min_ratio)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Errhandle {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("errhandle", || {
                    check_errhandle(&path, recursive, max_violations)
                })
            } else {
                check_errhandle(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Typecov {
            path,
            recursive,
            min_pct,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("typecov", || check_typecov(&path, recursive, min_pct))
            } else {
                check_typecov(&path, recursive, min_pct)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Vulnscan {
            path,
            max_critical,
            max_high,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("vulnscan", || check_vulnscan(&path, max_critical, max_high))
            } else {
                check_vulnscan(&path, max_critical, max_high)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Sast {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("sast", || check_sast(&path, recursive, max_findings))
            } else {
                check_sast(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Crypto {
            path,
            recursive,
            max_findings,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("crypto", || check_crypto(&path, recursive, max_findings))
            } else {
                check_crypto(&path, recursive, max_findings)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Licenses {
            path,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("licenses", || check_licenses(&path, max_violations))
            } else {
                check_licenses(&path, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Outdated {
            path,
            max_major_behind,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("outdated", || check_outdated(&path, max_major_behind))
            } else {
                check_outdated(&path, max_major_behind)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::AccessControl {
            path,
            recursive,
            max_violations,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("access-control", || {
                    check_access_control(&path, recursive, max_violations)
                })
            } else {
                check_access_control(&path, recursive, max_violations)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::SupplyChain {
            path,
            max_risks,
            format,
        } => {
            let result = if format == "text" {
                run_with_spinner("supply-chain", || check_supply_chain(&path, max_risks))
            } else {
                check_supply_chain(&path, max_risks)
            };
            let passed = result.passed;
            match format.as_str() {
                "text" => {
                    println!("{}", result.message);
                }
                _ => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
            }
            if passed {
                0
            } else {
                1
            }
        }

        Commands::Setup => {
            setup_command();
            0
        }

        Commands::Init { output, ci } => {
            let detect_start = Instant::now();
            let profile = run_with_spinner("detecting project ecosystem", || detect_project("."));

            // Determine detection reason
            let reason = if std::path::Path::new("Cargo.toml").exists() {
                "Cargo.toml found"
            } else if std::path::Path::new("go.mod").exists() {
                "go.mod found"
            } else if std::path::Path::new("pyproject.toml").exists()
                || std::path::Path::new("setup.py").exists()
            {
                "pyproject.toml / setup.py found"
            } else if std::path::Path::new("package.json").exists() {
                "package.json found"
            } else {
                "no known manifest found — using generic defaults"
            };

            eprintln!(
                "  {} detected: {}  ({})  {}",
                "✓".green().bold(),
                profile.ecosystem.to_string().cyan().bold(),
                reason.bright_black(),
                if profile.test_cmd.is_empty() {
                    "no test runner".to_string().bright_black().to_string()
                } else {
                    format!("test: {}", profile.test_cmd.join(" "))
                        .bright_black()
                        .to_string()
                }
            );
            let _ = detect_start;
            if ci {
                init_ci(&output, &profile)
            } else {
                let write_start = Instant::now();
                generate_config(&output, &profile);
                eprintln!(
                    "  {} wrote {}  ({})",
                    "✓".green().bold(),
                    output.cyan(),
                    format_elapsed(write_start.elapsed()).bright_black()
                );
                eprintln!();
                eprintln!("  {} Key thresholds chosen:", "▶".cyan().bold());
                eprintln!(
                    "    {} max_crap    = {}",
                    "·".bright_black(),
                    profile.max_crap.to_string().cyan()
                );
                eprintln!(
                    "    {} min_doc     = {}%",
                    "·".bright_black(),
                    profile.min_doc.to_string().cyan()
                );
                eprintln!(
                    "    {} max_debt    = {}",
                    "·".bright_black(),
                    profile.max_debt.to_string().cyan()
                );
                eprintln!(
                    "    {} max_complexity_violations = {}",
                    "·".bright_black(),
                    profile.max_complexity_violations.to_string().cyan()
                );
                eprintln!();
                eprintln!(
                    "  {} {} runs 20+ checks and produces a 0-100 score + letter grade.",
                    "▶".cyan().bold(),
                    "cogent check .".cyan().bold()
                );
                eprintln!();
                eprintln!("  {} Next steps:", "▶".cyan().bold());
                eprintln!(
                    "    1. {} cogent check .          {}",
                    "$".bright_black(),
                    "— run all checks now".bright_black()
                );
                eprintln!(
                    "    2. {} cogent report .         {}",
                    "$".bright_black(),
                    "— generate HTML audit report".bright_black()
                );
                eprintln!(
                    "    3. {} cogent init --ci        {}",
                    "$".bright_black(),
                    "— wire GitHub Actions + pre-commit hook".bright_black()
                );
                eprintln!(
                    "    4. {} cogent watch .          {}",
                    "$".bright_black(),
                    "— live re-check on file save".bright_black()
                );
                eprintln!();
                eprintln!(
                    "  {} Tip: edit {} to tune thresholds for your project.",
                    "ℹ".cyan(),
                    output.cyan()
                );
                0
            }
        }

        Commands::Explain { tool } => {
            explain_command(&tool);
            0
        }

        Commands::Discover { format } => {
            discover_command(&format);
            0
        }

        Commands::Run {
            path,
            config,
            format,
            baseline,
            no_fail_on_regression,
        } => run_batch(
            &path,
            &config,
            &format,
            baseline.as_deref(),
            no_fail_on_regression,
        ),

        Commands::History {
            action,
            dir,
            last,
            report,
            format,
        } => history_command(&action, &dir, last, report.as_deref(), &format),

        Commands::InstallHooks { repo, fast } => install_hooks(&repo, fast),

        Commands::UninstallHooks { repo } => uninstall_hooks(&repo),

        Commands::Watch {
            path,
            checks,
            debounce_ms,
            no_tests,
            full,
        } => watch_mode(&path, &checks, debounce_ms, no_tests, full),

        Commands::Report {
            path,
            format,
            output,
            project,
            from_json,
            skip,
            open,
        } => report_command(
            &path,
            &format,
            output.as_deref(),
            project.as_deref(),
            from_json.as_deref(),
            skip.as_deref(),
            open,
        ),

        Commands::Diff {
            before,
            after,
            format,
        } => diff_command(&before, &after, &format),

        Commands::Serve { port, history_dir } => {
            serve_command(port, &history_dir);
            0
        }

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            use clap_complete::{generate, Shell};
            let shell = match shell.as_str() {
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                "fish" => Shell::Fish,
                "powershell" => Shell::PowerShell,
                "elvish" => Shell::Elvish,
                _ => {
                    eprintln!(
                        "Unknown shell '{}'. Supported: bash, zsh, fish, powershell, elvish",
                        shell
                    );
                    std::process::exit(2);
                }
            };
            let mut cli = Cli::command();
            generate(shell, &mut cli, "cogent", &mut std::io::stdout());
            0
        }

        Commands::Policy { action } => match action {
            PolicyAction::Validate => {
                let known_tools: Vec<String> = [
                    "secrets",
                    "sast",
                    "debt",
                    "dupfind",
                    "deadcode",
                    "linelen",
                    "comments",
                    "coupling",
                    "cohesion",
                    "halstead",
                    "crap",
                    "riskmap",
                    "cryptocheck",
                    "errhandle",
                    "taint",
                    "typecov",
                    "propcov",
                    "fuzz",
                    "licenses",
                    "supply-chain",
                    "access-control",
                    "vulnscan",
                    "mutate",
                    "doccov",
                    "sbom",
                    "complexity",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                let policies = audit::discover_policies(".");
                if policies.is_empty() {
                    eprintln!("No policies found in ./.cogent-policies/");
                    0
                } else {
                    let mut all_valid = true;
                    for p in &policies {
                        let result = audit::validate_policy(p, &known_tools);
                        println!(
                            "{}  {}",
                            if result.valid {
                                "✓".green()
                            } else {
                                "✗".red()
                            },
                            p.display()
                        );
                        for w in &result.warnings {
                            println!("  ⚠  {}", w);
                        }
                        for e in &result.errors {
                            println!("  ✗  {}", e);
                            all_valid = false;
                        }
                    }
                    if all_valid {
                        0
                    } else {
                        1
                    }
                }
            }
            PolicyAction::Check {
                path,
                format,
                force,
            } => {
                // Placeholder: run only tools referenced in policies
                println!(
                    "Policy-based check on {} (format: {}, force: {})",
                    path, format, force
                );
                0
            }
        },

        Commands::Exception { action } => match action {
            ExceptionAction::Add {
                finding_id,
                rule_id,
                file,
                reason,
                reviewer,
            } => match audit::add_exception(&finding_id, &rule_id, &file, &reason, &reviewer) {
                Ok(id) => {
                    println!(
                        "Exception {} proposed (pending approval by {})",
                        id, reviewer
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Failed to add exception: {}", e);
                    2
                }
            },
            ExceptionAction::List { status } => {
                let exceptions = audit::list_exceptions(status.as_deref());
                if exceptions.is_empty() {
                    println!("No exceptions found.");
                } else {
                    println!(
                        "{:<10} {:<16} {:<20} {:<12} Reviewer",
                        "ID", "Finding", "Rule", "Status"
                    );
                    for e in &exceptions {
                        println!(
                            "{:<10} {:<16} {:<20} {:<12} {}",
                            e.id, e.finding_id, e.rule_id, e.status, e.reviewer
                        );
                    }
                }
                0
            }
            ExceptionAction::Approve { id } => match audit::approve_exception(&id) {
                Ok(()) => {
                    println!("Exception {} approved.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to approve exception: {}", e);
                    2
                }
            },
            ExceptionAction::Revoke { id } => match audit::revoke_exception(&id) {
                Ok(()) => {
                    println!("Exception {} revoked.", id);
                    0
                }
                Err(e) => {
                    eprintln!("Failed to revoke exception: {}", e);
                    2
                }
            },
        },

        Commands::Remediate { verify, path } => {
            if verify {
                println!("Verifying remediation on path: {}", path);
                // Placeholder: would run check, compare with remediation log
                audit::print_remediation_summary();
                0
            } else {
                println!("Showing remediation status for: {}", path);
                audit::print_remediation_summary();
                0
            }
        }

        Commands::AuditTrail {
            verify,
            command,
            since,
        } => {
            if verify {
                let (ok, errors) = audit::verify_audit_trail();
                if ok {
                    println!("{} Audit trail integrity verified.", "✓".green());
                } else {
                    println!("{} Audit trail verification failed:", "✗".red());
                    for e in &errors {
                        println!("  {}", e);
                    }
                }
                if ok {
                    0
                } else {
                    2
                }
            } else {
                let entries = audit::query_audit_trail(since.as_deref(), command.as_deref());
                if entries.is_empty() {
                    println!("No audit trail entries found.");
                } else {
                    println!(
                        "{:<26} {:<12} {:<20} {:<20} Findings",
                        "Timestamp", "Actor", "Command", "Scope"
                    );
                    for e in &entries {
                        println!(
                            "{:<26} {:<12} {:<20} {:<20} {}",
                            e.timestamp, e.actor, e.command, e.scope, e.findings_count
                        );
                    }
                }
                0
            }
        }

        Commands::Audit {
            path,
            format,
            only,
            evidence,
            framework,
            checks,
            skip,
            ci,
            verify,
        } => {
            if ci {
                std::env::set_var("COGENT_NO_PROGRESS", "1");
            }

            let audit_start = Instant::now();

            // Determine which check categories to run
            let security_checks = [
                "secrets",
                "sast",
                "crypto",
                "taint",
                "vulnscan",
                "access-control",
            ];
            let quality_checks = [
                "crap",
                "debt",
                "doccov",
                "complexity",
                "dupfind",
                "riskmap",
                "coupling",
                "propcov",
                "fuzz",
                "linelen",
                "halstead",
                "deadcode",
                "cohesion",
                "comments",
                "errhandle",
                "typecov",
            ];
            let compliance_checks = ["licenses", "supply-chain", "outdated", "sbom"];

            let active_categories: Vec<&str> = match only.as_deref() {
                Some("security") => security_checks.to_vec(),
                Some("quality") => quality_checks.to_vec(),
                Some("compliance") => compliance_checks.to_vec(),
                _ => security_checks
                    .iter()
                    .chain(quality_checks.iter())
                    .chain(compliance_checks.iter())
                    .copied()
                    .collect(),
            };

            let skip_set: std::collections::HashSet<String> = skip
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            let only_set: std::collections::HashSet<String> = checks
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            let should_run_audit = |name: &str| -> bool {
                if !only_set.is_empty() {
                    return only_set.contains(name);
                }
                if skip_set.contains(name) {
                    return false;
                }
                active_categories.contains(&name)
            };

            // Run checks using existing check_* functions with default thresholds from .quality.toml
            let mut checks_run: Vec<CheckResult> = Vec::new();

            macro_rules! run_audit_check {
                ($name:expr, $expr:expr) => {
                    if should_run_audit($name) {
                        checks_run.push(run_with_spinner($name, || $expr));
                    }
                };
            }

            run_audit_check!("secrets", check_secrets(&path, true, 0));
            run_audit_check!("sast", check_sast(&path, true, 0));
            run_audit_check!("crypto", check_crypto(&path, true, 0));
            run_audit_check!("taint", check_taint(&path, true, 0));
            run_audit_check!("vulnscan", check_vulnscan(&path, 0, 0));
            run_audit_check!("access-control", check_access_control(&path, true, 0));
            run_audit_check!("licenses", check_licenses(&path, 0));
            run_audit_check!("supply-chain", check_supply_chain(&path, 0));
            run_audit_check!("crap", check_crap(&path, true, &None, 30.0));
            run_audit_check!("debt", check_debt(&path, true, usize::MAX));
            run_audit_check!("doccov", check_doc_coverage(&path, true, 0.0));
            run_audit_check!(
                "complexity",
                check_complexity(&path, true, 10u32, usize::MAX)
            );
            run_audit_check!("dupfind", check_dupfind(&path, true, 100.0));
            run_audit_check!("riskmap", check_riskmap(&path, true, 100.0));
            run_audit_check!("coupling", check_coupling(&path, usize::MAX));
            run_audit_check!("deadcode", check_deadcode(&path, true, usize::MAX));
            run_audit_check!("cohesion", check_cohesion(&path, true, usize::MAX));
            run_audit_check!("comments", check_comments(&path, true, 0.0));
            run_audit_check!("errhandle", check_errhandle(&path, true, usize::MAX));
            run_audit_check!("halstead", check_halstead(&path, true, 100.0));
            run_audit_check!("linelen", check_linelen(&path, true, usize::MAX));

            // Collect all findings
            let mut all_findings: Vec<Finding> =
                checks_run.iter().flat_map(|c| c.findings.clone()).collect();

            // Enrich findings
            let use_framework = framework.as_deref().unwrap_or("");
            for f in &mut all_findings {
                // Always populate suggested_fix and controls
                if let Some((desc, diff, conf)) = audit::suggested_fix_for(&f.rule_id) {
                    f.suggested_fix = Some(SuggestedFix {
                        description: desc,
                        diff,
                        confidence: conf,
                    });
                }
                let ctrl = audit::controls_for(&f.rule_id);
                if !ctrl.is_empty() {
                    f.controls = Some(ctrl);
                }
                // Evidence only when requested
                if evidence {
                    audit::enrich_finding(f);
                }
            }

            let total_findings = all_findings.len();
            let critical = all_findings
                .iter()
                .filter(|f| f.severity == "critical")
                .count();
            let high = all_findings.iter().filter(|f| f.severity == "high").count();
            let medium = all_findings
                .iter()
                .filter(|f| f.severity == "medium")
                .count();
            let low = all_findings.iter().filter(|f| f.severity == "low").count();
            let passed = checks_run.iter().all(|c| c.passed);
            let (health, grade) = health_score(&checks_run);
            let duration = audit_start.elapsed();

            // Persist new findings to remediation log; optionally auto-close resolved ones
            let newly_recorded = audit::record_findings(&all_findings).len();
            let auto_closed = if verify {
                let closed = audit::verify_remediation(&all_findings);
                if !closed.is_empty() && !ci {
                    for id in &closed {
                        eprintln!("  ✓ auto-closed: {}", id);
                    }
                }
                closed.len()
            } else {
                0
            };

            // Write audit trail entry
            audit::append_audit_trail("audit", &path, total_findings, duration);

            match format.as_str() {
                "agent" => {
                    // NDJSON: one finding per line, then a summary line
                    for f in &all_findings {
                        let suppressed = audit::is_suppressed(&f.rule_id, &f.file);
                        let mut obj = serde_json::to_value(f).unwrap_or(serde_json::Value::Null);
                        if let Some(o) = obj.as_object_mut() {
                            o.insert("type".to_string(), serde_json::json!("finding"));
                            o.insert("suppressed".to_string(), serde_json::json!(suppressed));
                            if !use_framework.is_empty() {
                                // controls already populated above
                            }
                        }
                        println!("{}", serde_json::to_string(&obj).unwrap_or_default());
                    }
                    // Summary line
                    let controls_affected: Vec<String> = all_findings
                        .iter()
                        .filter_map(|f| f.controls.as_ref())
                        .flatten()
                        .cloned()
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    let summary = serde_json::json!({
                        "type": "summary",
                        "passed": passed,
                        "score": health,
                        "grade": grade.to_string(),
                        "total_findings": total_findings,
                        "critical": critical,
                        "high": high,
                        "medium": medium,
                        "low": low,
                        "controls_affected": controls_affected,
                        "checks_run": checks_run.len(),
                        "duration_ms": duration.as_millis(),
                        "path": path,
                        "framework": use_framework,
                        "newly_recorded": newly_recorded,
                        "auto_closed": auto_closed,
                    });
                    println!("{}", serde_json::to_string(&summary).unwrap_or_default());
                }
                "json" => {
                    let report = CheckReport {
                        passed,
                        path: path.clone(),
                        checks: checks_run.clone(),
                        summary: CheckSummary {
                            total_checks: checks_run.len(),
                            passed_checks: checks_run.iter().filter(|c| c.passed).count(),
                            failed_checks: checks_run.iter().filter(|c| !c.passed).count(),
                            functions_analyzed: 0,
                            avg_complexity: 0.0,
                            avg_crap: 0.0,
                        },
                        file_summary: aggregate_file_summary(&checks_run),
                    };
                    output_json(&report);
                }
                "markdown" => {
                    let report = CheckReport {
                        passed,
                        path: path.clone(),
                        checks: checks_run.clone(),
                        summary: CheckSummary {
                            total_checks: checks_run.len(),
                            passed_checks: checks_run.iter().filter(|c| c.passed).count(),
                            failed_checks: checks_run.iter().filter(|c| !c.passed).count(),
                            functions_analyzed: 0,
                            avg_complexity: 0.0,
                            avg_crap: 0.0,
                        },
                        file_summary: vec![],
                    };
                    output_markdown(&report, &path);
                }
                _ => {
                    eprintln!("Unknown format '{}'. Use: agent, json, markdown", format);
                    std::process::exit(2);
                }
            }

            if ci && total_findings > 0 {
                1
            } else if passed {
                0
            } else {
                1
            }
        }
    };

    std::process::exit(exit_code);
}

