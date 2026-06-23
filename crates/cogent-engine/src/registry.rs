//! cogent-engine registry — maps tool names to their crate/binary metadata.
//!
//! Used by `cogent check`, `cogent run`, and standalone tool commands to dispatch
//! to the correct binary without hard-coding every tool in a 46-arm match.

#![deny(clippy::all)]

use crate::checks::*;
use crate::CheckThresholds;
use cogent_common::CheckResult;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Metadata for a single Cogent audit tool.
///
/// Each variant of the `Commands` enum that delegates to an external binary
/// can be represented by an `AuditTool` entry in the registry.
pub struct AuditTool {
    /// The CLI-friendly name (e.g. "secrets", "debt", "vulnscan").
    pub name: &'static str,
    /// The binary name on `$PATH` (e.g. "secrets", "debt-scan").
    pub bin_name: &'static str,
    /// The Cargo crate name used for the `cargo run --bin` fallback.
    pub crate_name: &'static str,
    /// Human-readable description for help text and reports.
    pub description: &'static str,
    /// Whether the tool accepts a `--recursive` flag in batch mode.
    pub supports_recursive: bool,
}

impl AuditTool {
    /// Convenience constructor (defaults to `supports_recursive: true`).
    pub const fn new(
        name: &'static str,
        bin_name: &'static str,
        crate_name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            bin_name,
            crate_name,
            description,
            supports_recursive: true,
        }
    }

    /// Fluent setter for `supports_recursive`.
    pub const fn with_recursive(mut self, val: bool) -> Self {
        self.supports_recursive = val;
        self
    }
}

/// A read-only registry of all available Cogent tools.
///
/// Constructed once at startup via [`ToolRegistry::default`].
pub struct ToolRegistry {
    tools: HashMap<&'static str, AuditTool>,
}

/// Singleton registry, initialized once on first access.
/// Uses `OnceLock` (stable since Rust 1.70) to stay compatible with MSRV 1.75.
static REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

/// Return a reference to the process-wide singleton [`ToolRegistry`].
pub fn registry() -> &'static ToolRegistry {
    REGISTRY.get_or_init(ToolRegistry::new)
}

impl ToolRegistry {
    /// Look up a tool by its CLI name.
    pub fn get(&self, name: &str) -> Option<&AuditTool> {
        self.tools.get(name)
    }

    /// Iterate over every registered tool in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &AuditTool> {
        self.tools.values()
    }

    /// Return the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// True if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Run a single check by tool name using data-driven dispatch.
    ///
    /// Looks up the tool in the registry, then calls the appropriate engine
    /// check function with the given path, recursive flag, and thresholds.
    /// Returns `None` if the tool name is not in the registry or has no
    /// automated check (e.g. "sbom", "mutate").
    pub fn run_check(
        &self,
        name: &str,
        path: &str,
        recursive: bool,
        thresholds: &CheckThresholds,
    ) -> Option<CheckResult> {
        let _tool = self.get(name)?;
        match name {
            "taint" => Some(check_taint(path, recursive, thresholds.max_taint)),
            "secrets" => Some(check_secrets_with_excludes(
                path,
                recursive,
                thresholds.max_secrets,
                &thresholds.secrets_exclude_paths,
                &crate::DefaultToolRunner,
            )),
            "deadcode" => Some(check_deadcode(path, recursive, thresholds.max_deadcode)),
            "coupling" => Some(check_coupling(path, thresholds.max_coupling)),
            "linelen" => Some(check_linelen(path, recursive, thresholds.max_linelen)),
            "dupfind" => Some(check_dupfind(path, recursive, thresholds.max_duplication)),
            "riskmap" => Some(check_riskmap(path, false, thresholds.max_risk)),
            "propcov" => Some(check_propcov(path, recursive, thresholds.min_propcov)),
            "fuzz" => Some(check_fuzz(path, recursive, thresholds.max_fuzz_risk)),
            "halstead" => Some(check_halstead(
                path,
                recursive,
                thresholds.max_halstead_bugs,
            )),
            "cohesion" => Some(check_cohesion(path, recursive, thresholds.max_cohesion)),
            "comments" => Some(check_comments(
                path,
                recursive,
                thresholds.min_comment_ratio,
            )),
            "errhandle" => Some(check_errhandle(path, recursive, thresholds.max_errhandle)),
            "typecov" => Some(check_typecov(path, recursive, thresholds.min_typecov)),
            "sast" => Some(check_sast(path, recursive, thresholds.max_sast)),
            "crypto" => Some(check_crypto(path, recursive, thresholds.max_crypto)),
            "vulnscan" => Some(check_vulnscan(
                path,
                thresholds.max_vuln_critical,
                thresholds.max_vuln_high,
            )),
            "licenses" => Some(check_licenses(path, thresholds.max_license_violations)),
            "access-control" => Some(check_access_control(
                path,
                recursive,
                thresholds.max_access_control,
                &thresholds.access_control_exclude_paths,
                &crate::DefaultToolRunner,
            )),
            "supply-chain" => Some(check_supply_chain(path, thresholds.max_supply_chain)),
            "outdated" => Some(check_outdated(path, thresholds.max_outdated)),
            "debt" => Some(check_debt(path, recursive, thresholds.max_debt)),
            "doccov" => Some(check_doc_coverage(path, recursive, thresholds.min_doc)),
            "complexity" => Some(check_complexity(
                path,
                recursive,
                thresholds.min_complexity,
                thresholds.max_complexity_violations,
            )),
            "crap" => Some(check_crap(
                path,
                recursive,
                &thresholds.coverage_path,
                thresholds.max_crap,
            )),
            "observability" => Some(check_observability(
                path,
                recursive,
                thresholds.max_observability,
            )),
            "test-quality" => Some(check_test_quality(
                path,
                recursive,
                thresholds.max_test_quality,
            )),
            "design-docs" => Some(check_design_docs(path)),
            "debuggability" => Some(check_debuggability(
                path,
                recursive,
                thresholds.max_debuggability,
            )),
            _ => None, // sbom, mutate, and utility commands have no automated check
        }
    }
}

impl ToolRegistry {
    /// Construct a new registry — called once by the `LazyLock` singleton.
    fn new() -> Self {
        Self::default()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut tools = HashMap::new();
        let entries = [
            AuditTool::new(
                "crap",
                "crap",
                "crap-metric",
                "CRAP metric — complexity × (1 - coverage)³",
            ),
            AuditTool::new(
                "debt",
                "debt",
                "debt-scan",
                "Technical debt — TODO, FIXME, HACK, XXX markers",
            ),
            AuditTool::new(
                "doccov",
                "doccov",
                "doc-coverage",
                "Documentation coverage for public APIs",
            ),
            AuditTool::new(
                "complexity",
                "complexity",
                "doc-coverage",
                "Cyclomatic complexity per function",
            ),
            AuditTool::new(
                "taint",
                "taint",
                "taint-scan",
                "Taint analysis — untrusted input flows",
            ),
            AuditTool::new(
                "dupfind",
                "dupfind",
                "duplication",
                "Code duplication detection",
            ),
            AuditTool::new(
                "riskmap",
                "riskmap",
                "risk-map",
                "File risk heatmap (churn × complexity)",
            )
            .with_recursive(false),
            AuditTool::new(
                "coupling",
                "coupling",
                "coupling",
                "Architectural coupling / fan-out",
            )
            .with_recursive(false),
            AuditTool::new(
                "propcov",
                "propcov",
                "prop-cov",
                "Property-based test coverage",
            ),
            AuditTool::new(
                "fuzz",
                "fuzz",
                "fuzz-surface",
                "Fuzzable surface area detection",
            ),
            AuditTool::new(
                "linelen",
                "linelen",
                "line-length",
                "Function / file line length limits",
            ),
            AuditTool::new(
                "halstead",
                "halstead",
                "halstead",
                "Halstead complexity metrics",
            ),
            AuditTool::new(
                "secrets",
                "secrets",
                "secrets",
                "Hardcoded secret detection",
            ),
            AuditTool::new(
                "deadcode",
                "deadcode",
                "dead-code",
                "Dead code and unused import detection",
            ),
            AuditTool::new("cohesion", "cohesion", "cohesion", "LCOM4 cohesion metric"),
            AuditTool::new(
                "comments",
                "comments",
                "comment-ratio",
                "Comment ratio analysis",
            ),
            AuditTool::new(
                "errhandle",
                "errhandle",
                "error-handling",
                "Error handling pattern analysis",
            ),
            AuditTool::new(
                "typecov",
                "typecov",
                "type-coverage",
                "Type annotation coverage (Python/JS/TS)",
            ),
            AuditTool::new(
                "vulnscan",
                "vulnscan",
                "vuln-scan",
                "Dependency vulnerability scanning",
            )
            .with_recursive(false),
            AuditTool::new(
                "sast",
                "sast",
                "sast",
                "Static application security testing",
            ),
            AuditTool::new(
                "crypto",
                "cryptocheck",
                "crypto-check",
                "Cryptographic issue detection",
            ),
            AuditTool::new("licenses", "licenses", "licenses", "OSS license compliance")
                .with_recursive(false),
            AuditTool::new(
                "access-control",
                "access-control",
                "access-control",
                "Access control & auth analysis",
            ),
            AuditTool::new(
                "supply-chain",
                "supply-chain",
                "supply-chain",
                "Supply chain risk analysis",
            ),
            AuditTool::new(
                "outdated",
                "outdated",
                "outdated",
                "Dependency freshness — major version drift",
            )
            .with_recursive(false),
            AuditTool::new(
                "observability",
                "observability",
                "cogent-cli",
                "Structured logging observability",
            )
            .with_recursive(true),
            AuditTool::new(
                "test-quality",
                "test-quality",
                "cogent-cli",
                "Test quality & non-determinism detection",
            )
            .with_recursive(true),
            AuditTool::new(
                "design-docs",
                "design-docs",
                "cogent-cli",
                "Design documentation pillar check",
            )
            .with_recursive(false),
            AuditTool::new(
                "debuggability",
                "debuggability",
                "cogent-cli",
                "Contextless unwrap detection",
            )
            .with_recursive(true),
            AuditTool::new(
                "mutate",
                "mutate",
                "mutation-test",
                "Mutation testing kill-rate",
            ),
            AuditTool::new("sbom", "sbom", "sbom", "SBOM generation"),
        ];
        for e in entries {
            tools.insert(e.name, e);
        }
        Self { tools }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_tools() {
        let reg = ToolRegistry::default();
        assert!(!reg.is_empty());
    }

    #[test]
    fn test_registry_get_known_tool() {
        let reg = ToolRegistry::default();
        let tool = reg.get("crap");
        assert!(tool.is_some());
        let t = tool.unwrap();
        assert_eq!(t.name, "crap");
        assert_eq!(t.crate_name, "crap-metric");
    }

    #[test]
    fn test_registry_get_unknown_tool() {
        let reg = ToolRegistry::default();
        assert!(reg.get("unknown-tool").is_none());
    }

    #[test]
    fn test_registry_iter() {
        let reg = ToolRegistry::default();
        let names: Vec<_> = reg.iter().map(|t| t.name).collect();
        assert!(names.contains(&"crap"));
        assert!(names.contains(&"debt"));
    }

    #[test]
    fn test_audit_tool_with_recursive() {
        // Default AuditTool should support recursive scanning
        let recursive_tool = AuditTool::new(
            "sample-recursive",
            "sample-recursive",
            "sample-recursive",
            "sample-recursive",
        );
        assert!(recursive_tool.supports_recursive);

        // Explicitly disabling recursive should stick
        let non_recursive_tool = AuditTool::new(
            "sample-non-recursive",
            "sample-non-recursive",
            "sample-non-recursive",
            "sample-non-recursive",
        )
        .with_recursive(false);
        assert!(!non_recursive_tool.supports_recursive);
    }

    /// Every tool that has a `run_check` match arm must also exist in `ToolRegistry::default()`.
    /// This prevents silent panics when `reg_check!` calls `.unwrap()` on a `None` return.
    #[test]
    fn test_run_check_arms_match_registry_entries() {
        let reg = ToolRegistry::default();
        // Every tool name handled by run_check (excluding `_ => None` fallback)
        let run_check_tools = [
            "taint",
            "secrets",
            "deadcode",
            "coupling",
            "linelen",
            "dupfind",
            "riskmap",
            "propcov",
            "fuzz",
            "halstead",
            "cohesion",
            "comments",
            "errhandle",
            "typecov",
            "sast",
            "crypto",
            "vulnscan",
            "licenses",
            "access-control",
            "supply-chain",
            "outdated",
            "debt",
            "doccov",
            "complexity",
            "crap",
            "observability",
            "test-quality",
            "design-docs",
            "debuggability",
        ];
        for name in run_check_tools {
            assert!(
                reg.get(name).is_some(),
                "run_check has arm for '{}' but it is not in ToolRegistry::default()",
                name
            );
        }
    }
}
