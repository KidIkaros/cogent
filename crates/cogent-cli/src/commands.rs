//! CLI commands: init_ci, explain, discover for cogent-cli.

#![deny(clippy::all)]

use colored::Colorize;
use std::time::Instant;
use crate::config::{build_gha_workflow, generate_config, ProjectProfile};
use crate::hooks::install_hooks_impl;
use crate::progress::{format_elapsed, run_with_spinner, visible_len};
use crate::types::ToolInfo;

pub(crate) fn init_ci(config_path: &str, profile: &ProjectProfile) -> i32 {
    let mut ok = true;

    // 1. Write .quality.toml
    {
        let t = Instant::now();
        generate_config(config_path, profile);
        eprintln!(
            "  {} Wrote {}  ({})",
            "✓".green().bold(),
            config_path.cyan(),
            format_elapsed(t.elapsed()).bright_black()
        );
    }

    // 2. Install pre-commit hook
    {
        let t = Instant::now();
        let hook_result = install_hooks_impl(".", false, profile);
        if hook_result == 0 {
            eprintln!(
                "  {} Installed pre-commit hook  ({})",
                "✓".green().bold(),
                format_elapsed(t.elapsed()).bright_black()
            );
        } else {
            eprintln!(
                "  {} Could not install pre-commit hook (not a git repo?)",
                "!".yellow().bold()
            );
        }
    }

    // 3. Write GitHub Actions workflow
    {
        let t = Instant::now();
        let gha_dir = ".github/workflows";
        if let Err(e) = std::fs::create_dir_all(gha_dir) {
            eprintln!(
                "  {} Could not create {}: {}",
                "!".yellow().bold(),
                gha_dir,
                e
            );
            ok = false;
        } else {
            let workflow_path = format!("{}/cogent.yml", gha_dir);
            let workflow = build_gha_workflow(profile);
            match std::fs::write(&workflow_path, workflow) {
                Ok(_) => eprintln!(
                    "  {} Wrote {}  ({})",
                    "✓".green().bold(),
                    workflow_path.cyan(),
                    format_elapsed(t.elapsed()).bright_black()
                ),
                Err(e) => {
                    eprintln!(
                        "  {} Could not write {}: {}",
                        "!".yellow().bold(),
                        workflow_path,
                        e
                    );
                    ok = false;
                }
            }
        }
    }

    // 4. Seed baseline: run `cogent run . --format sarif --no-fail-on-regression`
    let seed = run_with_spinner("seeding quality baseline (this runs all tools)", || {
        std::process::Command::new(std::env::current_exe().unwrap_or("cogent".into()))
            .args(["run", ".", "--format", "sarif", "--no-fail-on-regression"])
            .output()
    });
    match seed {
        Ok(out) => {
            let sarif = String::from_utf8_lossy(&out.stdout);
            match std::fs::write(".cogent-baseline.sarif", sarif.as_bytes()) {
                Ok(_) => eprintln!("  {} Wrote .cogent-baseline.sarif", "✓".green().bold()),
                Err(e) => {
                    eprintln!("  {} Could not write baseline: {}", "!".yellow().bold(), e);
                    ok = false;
                }
            }
        }
        Err(e) => {
            eprintln!(
                "  {} Baseline seeding skipped (cogent not on PATH yet): {}",
                "!".yellow().bold(),
                e
            );
        }
    }

    // 5. Record initial history entry
    let history_dir = ".cogent-history";
    if std::fs::create_dir_all(history_dir).is_ok() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry = serde_json::json!({
            "ts": ts,
            "event": "init",
            "ecosystem": profile.ecosystem.to_string(),
            "passed": 0u64,
            "failed": 0u64,
        });
        let path = format!("{}/{}.jsonl", history_dir, ts);
        let _ = std::fs::write(&path, format!("{}\n", entry));
        println!("  {} Recorded initial history entry", "[✓]".green().bold());
    }

    eprintln!();
    if ok {
        eprintln!("  {} Setup complete — CI is ready.", "✓".green().bold());
    } else {
        eprintln!(
            "  {} Setup completed with warnings. Check messages above.",
            "!".yellow().bold()
        );
    }
    eprintln!();
    eprintln!("  {} Next steps:", "▶".cyan().bold());
    eprintln!(
        "    1. {} cogent check .          {}",
        "$".bright_black(),
        "— verify everything passes locally".bright_black()
    );
    eprintln!(
        "    2. {} cogent report .         {}",
        "$".bright_black(),
        "— open the HTML audit report in your browser".bright_black()
    );
    eprintln!(
        "    3. {} git push                     {}",
        "$".bright_black(),
        "— CI runs automatically on next push".bright_black()
    );
    eprintln!(
        "    4. {} cogent watch . --full   {}",
        "$".bright_black(),
        "— live feedback during development".bright_black()
    );
    eprintln!();
    0
}

/// Print per-tool documentation for `cogent explain <tool>`.
pub(crate) fn explain_command(tool: &str) {
    let tool = tool.to_lowercase();
    let (title, description, threshold, how_to_read, fixes, see_also) = match tool.as_str() {
        "crap" => (
            "CRAP (Change Risk Anti-Pattern)",
            "Measures how risky a function is to change. Combines cyclomatic complexity and test coverage. A high-CRAP function is complex AND untested — the most dangerous kind.",
            "max_avg = 15.0  (average CRAP score across all functions)",
            "Score 0-30 is good. Above 30 means many complex functions lack tests. Each function gets a CRAP score; the average is compared to the threshold.",
            vec![
                "Add unit tests for complex functions (coverage drives CRAP down faster than simplification).",
                "Refactor functions with cyclomatic complexity > 10 into smaller helpers.",
                "Focus on the top 5 offenders shown in the output.",
            ],
            "docs/tools/crap.md",
        ),
        "debt" => (
            "Technical Debt Markers",
            "Scans source code for TODO, FIXME, HACK, and XXX markers. Each marker is a known unaddressed issue that accumulates over time.",
            "max_markers = 0  (zero-tolerance by default for Rust)",
            "Output lists each marker with file:line and text. The count is compared to max_markers.",
            vec![
                "Convert TODOs into tracked GitHub issues and remove the marker.",
                "Schedule a 'debt sprint' to resolve FIXMEs before they rot.",
                "Use cogent watch . to catch new markers during development.",
            ],
            "docs/tools/debt.md",
        ),
        "doccov" | "doc" | "doc_coverage" => (
            "Documentation Coverage",
            "Measures the percentage of public functions that have doc comments (/// in Rust, /** in JS, etc.).",
            "min_pct = 95%  (Rust), 80% (Go), 70% (JS)",
            "Output shows the percentage and which public functions are missing docs. Missing docs make APIs harder to consume and maintain.",
            vec![
                "Add /// doc comments to every public function and struct.",
                "Use rustdoc --test to ensure doc examples compile.",
                "Enable #![warn(missing_docs)] in lib.rs to catch omissions at compile time.",
            ],
            "docs/tools/doccov.md",
        ),
        "complexity" | "cyclomatic" => (
            "Cyclomatic Complexity",
            "Counts the number of independent paths through a function (branches, loops, match arms). Higher complexity means harder testing and higher bug risk.",
            "max_violations = 0  (no functions should exceed complexity 10)",
            "Output lists every function with complexity >= 10. The count of such functions is compared to max_violations.",
            vec![
                "Extract nested conditionals into named helper functions.",
                "Replace deep match/if chains with strategy enums or lookup tables.",
                "Add unit tests for each extracted branch to preserve coverage.",
            ],
            "docs/tools/complexity.md",
        ),
        "taint" => (
            "Taint Analysis",
            "Tracks untrusted input flows through your code to find paths that reach security-sensitive operations (SQL, shell, file writes) without sanitization.",
            "max_taint = 0  (zero-tolerance)",
            "Each finding shows the full flow from source (user input) to sink (dangerous call). Red paths need sanitization or validation.",
            vec![
                "Add input validation at the API boundary (length, charset, regex).",
                "Use parameterized queries / prepared statements for all SQL.",
                "Never pass user input directly to std::process::Command or eval().",
            ],
            "",
        ),
        "dup" | "dupfind" | "duplication" => (
            "Code Duplication",
            "Finds blocks of identical or near-identical code. Duplicated code doubles maintenance cost and bug probability.",
            "max_duplicates = 0  (zero-tolerance for blocks >= 3 lines)",
            "Output groups clones by similarity. Each group lists the files and line ranges where the clone appears.",
            vec![
                "Extract the duplicated block into a shared helper function.",
                "If the clones differ slightly, use a function parameter to capture the variation.",
                "For boilerplate, consider macros or code generation.",
            ],
            "",
        ),
        "risk" | "riskmap" => (
            "Risk Map",
            "Identifies files that are both complex and frequently changed (high churn). These are bug hotspots.",
            "max_risk = 50.0  (risk score per file, 0-100)",
            "Score combines git churn (commit count) and cyclomatic complexity. Red files need refactoring or extra tests.",
            vec![
                "Add integration tests for the riskiest files.",
                "Refactor high-churn files to reduce coupling and complexity.",
                "Consider freezing volatile files behind stable interfaces.",
            ],
            "",
        ),
        "coupling" => (
            "Architectural Coupling",
            "Measures how tightly modules depend on each other. High coupling makes refactoring and testing difficult.",
            "max_coupling = 5  (allowed cross-module dependency issues)",
            "Output shows module pairs with excessive imports. Each arrow is a dependency that may violate your architecture.",
            vec![
                "Introduce dependency inversion: modules depend on traits, not concrete types.",
                "Move shared code to a common crate or module.",
                "Use visibility modifiers (pub(crate)) to hide internal details.",
            ],
            "",
        ),
        "secrets" => (
            "Secret Detection",
            "Scans for hardcoded API keys, tokens, passwords, and private keys committed to source control.",
            "max_secrets = 0  (zero-tolerance)",
            "Each finding shows the file, line, and secret type. Even test keys can leak real credentials through copy-paste.",
            vec![
                "Remove the secret from code immediately and rotate it in the service.",
                "Use environment variables or a secret manager (AWS Secrets Manager, 1Password, etc.).",
                "Add the pattern to .gitignore or a pre-commit hook (try cogent install-hooks).",
            ],
            "",
        ),
        "vulnscan" => (
            "Vulnerability Scan",
            "Checks dependencies against known CVE databases using cargo audit (Rust) or equivalent.",
            "max_vuln_critical = 0, max_vuln_high = 0  (zero-tolerance)",
            "Output lists each CVE with severity, affected crate/version, and advisory URL. Critical/High must be fixed immediately.",
            vec![
                "Run cargo update <crate> to pull patched versions.",
                "If no patch exists, consider replacing the dependency or adding a compensating control.",
                "Enable Dependabot or Renovate for automatic update PRs.",
            ],
            "",
        ),
        "sast" => (
            "Static Application Security Testing (SAST)",
            "Finds common security bugs: SQL injection, XSS, path traversal, command injection, insecure deserialization.",
            "max_sast = 0  (zero-tolerance)",
            "Each finding includes the vulnerability type, file:line, and a description of the unsafe pattern.",
            vec![
                "Use safe APIs: parameterized queries, HTML sanitizers, Path::join instead of string concat.",
                "Never eval() or deserialize untrusted data without schema validation.",
                "Run cogent sast --format json and feed findings into your issue tracker.",
            ],
            "",
        ),
        "crypto" => (
            "Cryptography Checker",
            "Detects weak cryptographic practices: MD5/SHA1, ECB mode, insecure random, disabled certificate validation.",
            "max_crypto = 0  (zero-tolerance)",
            "Findings list the insecure algorithm or pattern and where it is used.",
            vec![
                "Replace MD5/SHA1 with SHA-256 or Blake3.",
                "Use AEAD ciphers (ChaCha20-Poly1305, AES-GCM) instead of ECB.",
                "Use OsRng or getrandom for cryptographic randomness, not Math.random.",
            ],
            "",
        ),
        "licenses" => (
            "License Compliance",
            "Checks that all dependencies use permissive or approved open-source licenses.",
            "max_license_violations = 0  (zero-tolerance)",
            "Output lists each dependency with its license. Flagged licenses may conflict with your project's distribution terms.",
            vec![
                "Review flagged dependencies and seek legal approval if needed.",
                "Replace copyleft dependencies with permissive alternatives.",
                "Add an allow-list to .quality.toml under [licenses].",
            ],
            "",
        ),
        "deadcode" => (
            "Dead Code Detection",
            "Finds functions, methods, and modules that are never called. Dead code increases compile time and cognitive load.",
            "max_deadcode = 10  (allowed unused items)",
            "Output lists each unused item with file and name. Some may be false positives (public API, test helpers, feature-gated code).",
            vec![
                "Remove confirmed dead code.",
                "For public API items, add #[allow(dead_code)] with a comment explaining why.",
                "For test-only code, move it to a test module or cfg(test).",
            ],
            "",
        ),
        "mutate" => (
            "Mutation Testing",
            "Introduces small bugs (mutations) into your code and checks if tests catch them. Measures true test effectiveness.",
            "min_kill_rate = 0%  (no minimum by default)",
            "Kill rate = (mutants killed / total mutants) × 100. Higher is better. A low rate means tests are not exercising edge cases.",
            vec![
                "Add boundary-value tests (zero, empty, max, null).",
                "Use property-based testing to catch unexpected edge cases.",
                "Review surviving mutants manually — they often reveal real gaps.",
            ],
            "",
        ),
        "linelen" => (
            "Line Length",
            "Counts lines exceeding a character limit (default 100). Long lines reduce readability and cause horizontal scrolling.",
            "max_linelen = 0  (zero lines should exceed the limit)",
            "Output lists each offending line with file and line number.",
            vec![
                "Break long expressions after operators (Rustfmt does this automatically).",
                "Extract nested calls into intermediate variables.",
                "Configure your editor to show a vertical ruler at 100 characters.",
            ],
            "",
        ),
        "halstead" => (
            "Halstead Metrics",
            "Estimates program difficulty, effort, and predicted bugs based on operator/operand counts.",
            "max_halstead_bugs = 2.0  (estimated bugs per file)",
            "Output shows estimated bugs, volume, and difficulty per file. High values suggest dense, hard-to-review code.",
            vec![
                "Split large files into focused modules.",
                "Reduce the number of distinct operators by using higher-level abstractions.",
                "Add extra review for files with high predicted bug counts.",
            ],
            "",
        ),
        "cohesion" => (
            "LCOM4 Cohesion",
            "Measures how tightly related the methods of a class/struct are. Low cohesion means the type has too many responsibilities.",
            "max_cohesion = 5  (allowed low-cohesion types)",
            "Output lists types with LCOM4 > 1. A score of 1 is perfectly cohesive; higher means unrelated methods share state.",
            vec![
                "Split the type into smaller, single-responsibility types.",
                "Move unrelated methods to dedicated helper structs.",
                "Use composition instead of inheritance to share behavior.",
            ],
            "",
        ),
        "comments" => (
            "Comment Ratio",
            "Measures the ratio of comment lines to total lines. Too few means missing context; too many may indicate over-complex code.",
            "min_comment_ratio = 0.05  (5% of lines should be comments)",
            "Output shows the ratio. This is a soft metric — quality of comments matters more than quantity.",
            vec![
                "Add 'why' comments for non-obvious business logic.",
                "Remove redundant comments that restate the code.",
                "Use doc comments on public APIs instead of inline comments where possible.",
            ],
            "",
        ),
        "errhandle" => (
            "Error Handling",
            "Counts unsafe patterns: unwrap(), expect(), panic(), and discarded Result/Option values.",
            "max_errhandle = 50  (allowed occurrences)",
            "Output lists each occurrence with file:line and the pattern used. Each is a potential crash or silent failure.",
            vec![
                "Replace unwrap() with ? or match for graceful error propagation.",
                "Add context to errors with anyhow::Context or similar.",
                "Use must_use lint to catch discarded Results.",
            ],
            "",
        ),
        "typecov" => (
            "Type Coverage",
            "Measures the percentage of variables/parameters with explicit type annotations (Python/JS/TS).",
            "min_typecov = 0%  (off by default; enable for typed ecosystems)",
            "Output shows the percentage and which functions are missing annotations.",
            vec![
                "Enable strict mode (mypy --strict, tsc --noImplicitAny).",
                "Add return-type annotations to all public functions.",
                "Use a type-checking pre-commit hook to catch regressions.",
            ],
            "",
        ),
        "propcov" => (
            "Property Test Coverage",
            "Measures coverage achieved by property-based tests (e.g., proptest, Hypothesis, fast-check).",
            "min_propcov = 0%  (off by default)",
            "Output shows the percentage. Property tests find edge cases that example-based tests miss.",
            vec![
                "Add proptest or Hypothesis to your test suite.",
                "Write properties that invariants must hold (e.g., roundtrip serialize → deserialize).",
                "Run with --nocapture to see counter-examples on failure.",
            ],
            "",
        ),
        "fuzz" => (
            "Fuzz Surface",
            "Counts functions that accept raw bytes or strings and could benefit from fuzz testing.",
            "max_fuzz_risk = 0  (zero unprotected fuzzable endpoints)",
            "Output lists functions that parse untrusted input. These are ideal targets for cargo-fuzz or libFuzzer.",
            vec![
                "Add fuzz targets for parsers and deserializers.",
                "Use fuzzing to find panic paths and infinite loops.",
                "Sanitize all inputs before parsing (length limits, magic-byte checks).",
            ],
            "",
        ),
        "access-control" => (
            "Access Control",
            "Checks for missing authorization on sensitive endpoints and functions.",
            "max = 0  (zero missing auth checks)",
            "Output lists endpoints without explicit access-control checks.",
            vec![
                "Add middleware or decorators for authentication/authorization.",
                "Enforce least-privilege: default-deny, explicit allow.",
                "Run SAST alongside access-control checks for defense in depth.",
            ],
            "",
        ),
        "supply-chain" => (
            "Supply Chain Security",
            "Analyzes dependencies for supply-chain risks: typosquatting, unmaintained crates, suspicious publishers.",
            "max = 0  (zero supply-chain issues)",
            "Output lists each risky dependency with the reason for concern.",
            vec![
                "Pin dependencies and audit new ones before adding.",
                "Use cargo-vet or npm audit for transitive dependency review.",
                "Mirror critical dependencies internally to reduce external exposure.",
            ],
            "",
        ),
        "outdated" => (
            "Outdated Dependencies",
            "Counts direct dependencies that are a full major version behind the latest release.",
            "max_outdated = 0  (zero outdated major versions)",
            "Output lists each outdated dependency with current and latest versions.",
            vec![
                "Schedule monthly dependency update sprints.",
                "Enable Dependabot or Renovate for automatic PRs.",
                "Test thoroughly after major-version upgrades.",
            ],
            "",
        ),
        "sbom" => (
            "Software Bill of Materials (SBOM)",
            "Generates an SBOM in SPDX or CycloneDX format. Lists all dependencies with versions, licenses, and hashes for audit trails.",
            "max_sbom_violations = 0  (zero unknown/missing packages)",
            "Output shows the full dependency tree in machine-readable format. Use for compliance audits and vulnerability scanning.",
            vec![
                "Run cogent sbom . --format spdx to generate SPDX JSON.",
                "Upload to your compliance tool or vulnerability scanner.",
                "Include SBOM artifacts in your release pipeline.",
            ],
            "",
        ),
        "taint-scan" | "taintscan" => (
            "Taint Analysis",
            "Tracks untrusted input flows through your code to find paths that reach security-sensitive operations (SQL, shell, file writes) without sanitization.",
            "max_taint = 0  (zero-tolerance)",
            "Each finding shows the full flow from source (user input) to sink (dangerous call). Red paths need sanitization or validation.",
            vec![
                "Add input validation at the API boundary (length, charset, regex).",
                "Use parameterized queries / prepared statements for all SQL.",
                "Never pass user input directly to std::process::Command or eval().",
            ],
            "",
        ),
        _ => {
            eprintln!("  {} Unknown tool: '{}'", "✗".red().bold(), tool);
            eprintln!();
            eprintln!("  Available tools:");
            let tools = [
                "crap", "debt", "doccov", "complexity", "taint", "dup", "risk", "coupling",
                "secrets", "vulnscan", "sast", "crypto", "licenses", "deadcode", "mutate",
                "linelen", "halstead", "cohesion", "comments", "errhandle", "typecov",
                "propcov", "fuzz", "access-control", "supply-chain", "outdated",
            ];
            for t in tools.chunks(4) {
                eprintln!("    {}", t.join(", ").bright_black());
            }
            eprintln!();
            eprintln!("  {} Example: cogent explain crap", "▶".cyan());
            return;
        }
    };

    eprintln!("  {}", title.cyan().bold());
    eprintln!("  {}", "─".repeat(visible_len(title)).cyan());
    eprintln!();
    eprintln!("  {}", "What it measures".bold());
    eprintln!("    {}", description);
    eprintln!();
    eprintln!("  {}", "Threshold".bold());
    eprintln!("    {}", threshold);
    eprintln!();
    eprintln!("  {}", "How to read the output".bold());
    eprintln!("    {}", how_to_read);
    eprintln!();
    eprintln!("  {}", "Quick fixes".bold());
    for (i, fix) in fixes.iter().enumerate() {
        eprintln!("    {} {}", format!("{}.", i + 1).cyan(), fix);
    }
    if !see_also.is_empty() {
        eprintln!();
        eprintln!(
            "  {} See {} for deeper explanation and examples.",
            "ℹ".cyan(),
            see_also.cyan()
        );
    }
    eprintln!();
}

pub(crate) fn discover_command(format: &str) {
    // Output tool discovery info (existing functionality)
    // This outputs internal ToolInfo format
    let tools = vec![
        ToolInfo {
            name: "crap".to_string(),
            binary: "crap".to_string(),
            description: "CRAP score calculator (maintenance risk)".to_string(),
            supported_formats: vec![
                "json".to_string(),
                "text".to_string(),
                "sarif".to_string(),
                "ndjson".to_string(),
            ],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["crap-error".to_string(), "crap-warning".to_string()],
        },
        ToolInfo {
            name: "debt".to_string(),
            binary: "debt".to_string(),
            description: "Technical debt scanner (TODO/FIXME/HACK)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "type".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec![
                "debt-todo".to_string(),
                "debt-fixme".to_string(),
                "debt-hack".to_string(),
                "debt-xxx".to_string(),
                "debt-bug".to_string(),
            ],
        },
        ToolInfo {
            name: "doccov".to_string(),
            binary: "doccov".to_string(),
            description: "Documentation coverage for public APIs".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["doccov-missing-doc".to_string()],
        },
        ToolInfo {
            name: "dupfind".to_string(),
            binary: "dupfind".to_string(),
            description: "Code duplication detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["dupfind-duplicate".to_string()],
        },
        ToolInfo {
            name: "coupling".to_string(),
            binary: "coupling".to_string(),
            description: "Module dependency analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["coupling-high".to_string()],
        },
        ToolInfo {
            name: "riskmap".to_string(),
            binary: "riskmap".to_string(),
            description: "Risk map (churn × complexity)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["riskmap-high-risk".to_string()],
        },
        ToolInfo {
            name: "mutate".to_string(),
            binary: "mutate".to_string(),
            description: "Mutation testing (Rust-only)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["mutate-unmutated".to_string()],
        },
        ToolInfo {
            name: "fuzz".to_string(),
            binary: "fuzz".to_string(),
            description: "Fuzz surface analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["fuzz-unsafe-surface".to_string()],
        },
        ToolInfo {
            name: "propcov".to_string(),
            binary: "propcov".to_string(),
            description: "Property test coverage".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["propcov-low-coverage".to_string()],
        },
        ToolInfo {
            name: "taint".to_string(),
            binary: "taint".to_string(),
            description: "Taint analysis (data flow)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec![
                "rule_id".to_string(),
                "severity".to_string(),
                "message".to_string(),
                "file".to_string(),
                "line".to_string(),
                "help".to_string(),
            ],
            rule_ids: vec!["taint-unsafe-flow".to_string()],
        },
        ToolInfo {
            name: "init".to_string(),
            binary: "cogent".to_string(),
            description: "Auto-detect project ecosystem and write .quality.toml. Use --ci for full GitHub Actions + pre-commit hook + baseline bootstrap.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["ecosystem".to_string(), "config_path".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "check".to_string(),
            binary: "cogent".to_string(),
            description: "Run all quality checks in one call. Auto-loads .quality.toml thresholds. Exit 0=pass, 1=fail, 2=error.".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "sarif".to_string(), "junit".to_string(), "findings".to_string(), "ndjson".to_string(), "markdown".to_string()],
            output_fields: vec![
                "passed".to_string(),
                "checks".to_string(),
                "score".to_string(),
                "threshold".to_string(),
                "message".to_string(),
                "findings".to_string(),
                "file_summary".to_string(),
            ],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "watch".to_string(),
            binary: "cogent".to_string(),
            description: "Watch for file changes and re-run checks. Auto-detects test runner and coverage. Use --no-tests for metrics-only mode.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "install-hooks".to_string(),
            binary: "cogent".to_string(),
            description: "Install a pre-commit git hook. Default: full hook (tests + coverage + check). Use --fast for lightweight metrics-only hook.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "complexity".to_string(),
            binary: "complexity".to_string(),
            description: "Cyclomatic complexity violations".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["complexity-high".to_string()],
        },
        ToolInfo {
            name: "linelen".to_string(),
            binary: "linelen".to_string(),
            description: "Line length violations".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["linelen-exceeded".to_string()],
        },
        ToolInfo {
            name: "halstead".to_string(),
            binary: "halstead".to_string(),
            description: "Halstead complexity metrics".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["halstead-high".to_string()],
        },
        ToolInfo {
            name: "secrets".to_string(),
            binary: "secrets".to_string(),
            description: "Hardcoded secret and credential detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["secrets-exposed".to_string()],
        },
        ToolInfo {
            name: "deadcode".to_string(),
            binary: "deadcode".to_string(),
            description: "Dead code and unreachable branch detection".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["deadcode-unused".to_string()],
        },
        ToolInfo {
            name: "cohesion".to_string(),
            binary: "cohesion".to_string(),
            description: "LCOM4 cohesion analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["cohesion-low".to_string()],
        },
        ToolInfo {
            name: "comments".to_string(),
            binary: "comments".to_string(),
            description: "Comment ratio analysis".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["comments-low".to_string()],
        },
        ToolInfo {
            name: "errhandle".to_string(),
            binary: "errhandle".to_string(),
            description: "Error handling pattern checker (unwrap/expect/panic)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["errhandle-unsafe".to_string()],
        },
        ToolInfo {
            name: "typecov".to_string(),
            binary: "typecov".to_string(),
            description: "Type annotation coverage for Python/JS/TS".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["typecov-low".to_string()],
        },
        ToolInfo {
            name: "vulnscan".to_string(),
            binary: "vulnscan".to_string(),
            description: "CVE vulnerability scanner".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["vulnscan-cve".to_string()],
        },
        ToolInfo {
            name: "sast".to_string(),
            binary: "sast".to_string(),
            description: "Static application security testing (SQLi, XSS, path traversal, smart contracts)".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["sast-sqli".to_string(), "sast-xss".to_string(), "sast-pathtraversal".to_string(), "sast-cmdi".to_string(), "sast-reentrancy".to_string(), "sast-access-control".to_string()],
        },
        ToolInfo {
            name: "crypto".to_string(),
            binary: "crypto".to_string(),
            description: "Weak cryptography checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["crypto-weak".to_string(), "crypto-insecure-random".to_string(), "crypto-ecb".to_string()],
        },
        ToolInfo {
            name: "licenses".to_string(),
            binary: "licenses".to_string(),
            description: "OSS license compliance checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["license-violation".to_string()],
        },
        ToolInfo {
            name: "outdated".to_string(),
            binary: "outdated".to_string(),
            description: "Outdated dependency checker".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["outdated-dependency".to_string()],
        },
        ToolInfo {
            name: "report".to_string(),
            binary: "cogent".to_string(),
            description: "Generate a human-readable audit report (HTML or Markdown)".to_string(),
            supported_formats: vec!["html".to_string(), "markdown".to_string()],
            output_fields: vec!["passed".to_string(), "checks".to_string(), "score".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "diff".to_string(),
            binary: "cogent".to_string(),
            description: "Compare two check JSON snapshots".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec![],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "access-control".to_string(),
            binary: "access-control".to_string(),
            description: "Access control checker — missing auth guards, hardcoded credentials, IAM policies, CORS".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "file".to_string(), "line".to_string(), "help".to_string()],
            rule_ids: vec!["acl-auth".to_string(), "acl-cred".to_string(), "acl-iam".to_string(), "acl-cors".to_string()],
        },
        ToolInfo {
            name: "supply-chain".to_string(),
            binary: "supply-chain".to_string(),
            description: "Supply chain checker — dependency integrity, typosquatting, abandoned packages, unpinned deps".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string(), "ndjson".to_string()],
            output_fields: vec!["rule_id".to_string(), "severity".to_string(), "message".to_string(), "package".to_string(), "version".to_string(), "help".to_string()],
            rule_ids: vec!["supply-lock".to_string(), "supply-typo".to_string(), "supply-pin".to_string()],
        },
        ToolInfo {
            name: "sbom".to_string(),
            binary: "sbom".to_string(),
            description: "Software bill of materials — generate CycloneDX/SPDX SBOM from Cargo/npm/go.mod".to_string(),
            supported_formats: vec!["json".to_string(), "text".to_string()],
            output_fields: vec!["components".to_string(), "format".to_string(), "version".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "policy".to_string(),
            binary: "cogent".to_string(),
            description: "Validate or enforce audit policies defined in .cogent-policies/. Subcommands: validate, check.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["valid".to_string(), "warnings".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "exception".to_string(),
            binary: "cogent".to_string(),
            description: "Manage approved exceptions (false positive overrides). Subcommands: add, list, approve, revoke.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["id".to_string(), "rule_id".to_string(), "file".to_string(), "status".to_string(), "reviewer".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "remediate".to_string(),
            binary: "cogent".to_string(),
            description: "Track remediation status of findings. Use --verify to confirm fixes applied.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["open".to_string(), "resolved".to_string(), "overdue".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "audit-trail".to_string(),
            binary: "cogent".to_string(),
            description: "Query and verify the signed JSONL audit trail. Use --verify to detect tampering, --since for time-bounded queries.".to_string(),
            supported_formats: vec!["text".to_string()],
            output_fields: vec!["timestamp".to_string(), "actor".to_string(), "command".to_string(), "scope".to_string(), "findings_count".to_string()],
            rule_ids: vec![],
        },
        ToolInfo {
            name: "audit".to_string(),
            binary: "cogent".to_string(),
            description: "Full DIY auditor: runs all checks, enriches every finding with suggested_fix + compliance controls, writes audit trail. --format agent emits NDJSON (one finding per line) for agent consumption.".to_string(),
            supported_formats: vec!["agent".to_string(), "json".to_string(), "markdown".to_string()],
            output_fields: vec![
                "type".to_string(),
                "tool".to_string(),
                "rule_id".to_string(),
                "severity".to_string(),
                "file".to_string(),
                "line".to_string(),
                "message".to_string(),
                "suggested_fix".to_string(),
                "controls".to_string(),
                "suppressed".to_string(),
                "evidence".to_string(),
            ],
            rule_ids: vec![],
        },
    ];

    match format {
        "text" => {
            for tool in &tools {
                println!("{} ({})", tool.name, tool.binary);
                println!("  Description: {}", tool.description);
                println!("  Supported Formats: {}", tool.supported_formats.join(", "));
                println!("  Output Fields: {}", tool.output_fields.join(", "));
                println!("  Rule IDs: {}", tool.rule_ids.join(", "));
                println!();
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&tools).unwrap());
        }
    }
}

