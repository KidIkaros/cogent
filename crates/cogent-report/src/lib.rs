//! cogent-report — output formatting and result types for Cogent.

#![deny(clippy::all)]

pub mod formatters;
pub mod html;

use tracing::debug;

#[cfg(test)]
mod tests;

// Re-export common types so formatters.rs can reference them via `crate::`
#[doc(hidden)]
pub use cogent_common::{
    CheckReport, CheckResult, CheckSummary, Finding, Evidence, SuggestedFix, FileSummary,
    SarifArtifactLocation, SarifDriver, SarifInvocation, SarifLocation, SarifLog, SarifMessage,
    SarifPhysicalLocation, SarifRegion, SarifResult, SarifRule, SarifRuleConfig, SarifRun,
    SarifTool,
};

/// Escape HTML special characters.
pub fn html_escape(s: &str) -> String {
    debug!(len = s.len(), "escaping HTML");
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
