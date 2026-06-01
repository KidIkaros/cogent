//! CLI type definitions: `Cli`, `Commands`, and sub-action enums.
//! Kept separate from `main.rs` to give the compiler a clean module
//! boundary and to make the command surface easy to read in isolation.

#![deny(clippy::all)]

use clap::{Parser, Subcommand};

// ═══════════════════════════════════════════
// HQSE PHASE HELPER
// ═══════════════════════════════════════════

pub fn hqse_phase_for(rule_id: &str) -> &'static str {
    let r = rule_id.to_lowercase();
    if r.contains("observability") { return "§7 Support / §4.5 Tracing"; }
    if r.contains("debuggability") || r.contains("contextless-unwrap") { return "§6.6 Debug / §4.5 Tracing"; }
    if r.contains("test-quality") || r.contains("test_quality") || r.contains("nondeterminism") { return "§6 Test"; }
    if r.contains("design-docs") || r.contains("design_docs") { return "§3 Design"; }
    if r.contains("doccov") { return "§3 Design / §4 Code"; }
    if r.contains("errhandle") || r.contains("unwrap") { return "§7 Support / §4 Code"; }
    if r.contains("secrets") || r.contains("sast") || r.contains("crypto") || r.contains("taint") { return "§4 Code"; }
    if r.contains("crap") || r.contains("complexity") || r.contains("debt") || r.contains("cohesion") || r.contains("coupling") { return "§4 Code"; }
    if r.contains("deadcode") || r.contains("linelen") || r.contains("halstead") { return "§4 Code"; }
    if r.contains("vulnscan") || r.contains("license") || r.contains("supply") || r.contains("outdated") { return "§4 Code"; }
    "§4 Code"
}

// ═══════════════════════════════════════════
// SUB-ACTION ENUMS
// ═══════════════════════════════════════════

#[derive(Subcommand)]
pub enum PolicyAction {
    /// Validate all .cogent-policies/*.yaml files
    Validate,
    /// Run checks defined in policies only
    Check {
        #[arg(default_value = ".")]
        path: String,
        #[arg(short, long, default_value = "json")]
        format: String,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ExceptionAction {
    /// Propose a new exception (status: pending)
    Add {
        #[arg(long)]
        finding_id: String,
        #[arg(long)]
        rule_id: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        reviewer: String,
    },
    /// List exceptions
    List {
        #[arg(long)]
        status: Option<String>,
    },
    /// Approve a pending exception
    Approve {
        id: String,
    },
    /// Revoke an approved exception
    Revoke {
        id: String,
    },
}

// ═══════════════════════════════════════════
// CLI DEFINITION
// ═══════════════════════════════════════════

#[derive(Parser)]
#[command(
    name = "cogent",
    about = "Unified code quality tool for Rust. Headless-first, JSON output, CI-ready.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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

    /// SBOM (Software Bill of Materials) generator
    Sbom {
        /// Path to analyze (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Output format: xml, json (default: xml)
        #[arg(short, long, default_value = "xml")]
        format: String,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
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
        after_help = "Example: cogent policy validate → checks all policy files for errors.\ncogent policy check . → runs only the tools defined in policies."
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
