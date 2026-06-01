//! cogent-engine registry — maps tool names to their crate/binary metadata.
//!
//! Used by `cogent check`, `cogent run`, and standalone tool commands to dispatch
//! to the correct binary without hard-coding every tool in a 46-arm match.

#![deny(clippy::all)]

use std::collections::HashMap;

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
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut tools = HashMap::new();
        let entries = [
            AuditTool::new("crap", "crap", "crap-metric", "CRAP metric — complexity × (1 - coverage)³"),
            AuditTool::new("debt", "debt", "debt-scan", "Technical debt — TODO, FIXME, HACK, XXX markers"),
            AuditTool::new("doccov", "doccov", "doc-coverage", "Documentation coverage for public APIs"),
            AuditTool::new("complexity", "complexity", "doc-coverage", "Cyclomatic complexity per function"),
            AuditTool::new("taint", "taint", "taint-scan", "Taint analysis — untrusted input flows"),
            AuditTool::new("dupfind", "dupfind", "duplication", "Code duplication detection"),
            AuditTool::new("riskmap", "riskmap", "risk-map", "File risk heatmap (churn × complexity)").with_recursive(false),
            AuditTool::new("coupling", "coupling", "coupling", "Architectural coupling / fan-out").with_recursive(false),
            AuditTool::new("propcov", "propcov", "prop-cov", "Property-based test coverage"),
            AuditTool::new("fuzz", "fuzz", "fuzz-surface", "Fuzzable surface area detection"),
            AuditTool::new("linelen", "linelen", "line-length", "Function / file line length limits"),
            AuditTool::new("halstead", "halstead", "halstead", "Halstead complexity metrics"),
            AuditTool::new("secrets", "secrets", "secrets", "Hardcoded secret detection"),
            AuditTool::new("deadcode", "deadcode", "dead-code", "Dead code and unused import detection"),
            AuditTool::new("cohesion", "cohesion", "cohesion", "LCOM4 cohesion metric"),
            AuditTool::new("comments", "comments", "comment-ratio", "Comment ratio analysis"),
            AuditTool::new("errhandle", "errhandle", "error-handling", "Error handling pattern analysis"),
            AuditTool::new("typecov", "typecov", "type-coverage", "Type annotation coverage (Python/JS/TS)"),
            AuditTool::new("vulnscan", "vulnscan", "vuln-scan", "Dependency vulnerability scanning").with_recursive(false),
            AuditTool::new("sast", "sast", "sast", "Static application security testing"),
            AuditTool::new("crypto", "cryptocheck", "crypto-check", "Cryptographic issue detection"),
            AuditTool::new("licenses", "licenses", "licenses", "OSS license compliance").with_recursive(false),
            AuditTool::new("access-control", "access-control", "access-control", "Access control & auth analysis"),
            AuditTool::new("supply-chain", "supply-chain", "supply-chain", "Supply chain risk analysis"),
            AuditTool::new("mutate", "mutate", "mutation-test", "Mutation testing kill-rate"),
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
        assert!(reg.len() > 0);
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
        let t = AuditTool::new("test", "test", "test", "test");
        assert!(t.supports_recursive);
        let t2 = AuditTool::new("test2", "test2", "test2", "test2").with_recursive(false);
        assert!(!t2.supports_recursive);
    }
}
