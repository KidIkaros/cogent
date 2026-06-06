//! Engine-specific types

use cogent_protocol::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Internal check request (from protocol to engine)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRequest {
    pub workspace: String,
    pub targets: Vec<String>,
    pub rules: Vec<String>,
    pub rule_packs: Vec<String>,
    pub rule_configs: HashMap<String, RuleConfig>,
    pub baseline_id: Option<String>,
    pub incremental: bool,
    pub changed_files: Vec<String>,
    pub output_format: OutputFormat,
}

/// Internal check response (from engine to protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub check_id: String,
    pub passed: bool,
    pub summary: CheckSummary,
    pub baseline_id: Option<String>,
}

/// Streaming check event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckEvent {
    Finding(Finding),
    Progress(ProgressParams),
    RuleComplete(RuleCompleteParams),
    End(FindingsEndParams),
}

use crate::config::EngineConfig;

/// Tool runner trait (async version)
#[async_trait::async_trait]
pub trait AsyncToolRunner: Send + Sync {
    async fn run(
        &self,
        crate_name: &str,
        bin_name: &str,
        args: &[&str],
    ) -> Result<cogent_common::ToolResult, EngineError>;

    fn name(&self) -> &'static str;
}

/// Engine errors
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Config error: {0}")]
    ConfigError(#[from] crate::config::ConfigError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Protocol error: {0}")]
    ProtocolError(#[from] cogent_protocol::error::ProtocolError),

    #[error("Join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Other: {0}")]
    Other(String),
}

pub type EngineResult<T> = Result<T, EngineError>;

/// Default async tool runner implementation
pub struct DefaultAsyncToolRunner;

impl DefaultAsyncToolRunner {
    pub fn new() -> Self {
        Self
    }

    fn workspace_root(&self) -> String {
        std::env::var("COGENT_WORKSPACE_ROOT").unwrap_or_else(|_| {
            std::env::var("CARGO_MANIFEST_DIR")
                .ok()
                .and_then(|d| {
                    std::path::Path::new(&d)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| ".".to_string())
        })
    }

    fn resolve_binary(&self, bin_name: &str) -> String {
        let workspace_root = self.workspace_root();
        let release_binary = std::path::Path::new(&workspace_root)
            .join("target")
            .join("release")
            .join(bin_name);

        if release_binary.exists() {
            release_binary
                .canonicalize()
                .unwrap_or(release_binary)
                .to_string_lossy()
                .to_string()
        } else {
            bin_name.to_string()
        }
    }
}

#[async_trait::async_trait]
impl AsyncToolRunner for DefaultAsyncToolRunner {
    async fn run(
        &self,
        crate_name: &str,
        bin_name: &str,
        args: &[&str],
    ) -> Result<cogent_common::ToolResult, EngineError> {
        use std::process::Stdio;
        use tokio::process::Command;

        let binary_path = self.resolve_binary(bin_name);
        let start = std::time::Instant::now();

        tracing::info!(tool = bin_name, crate = crate_name, args = ?args, "spawning tool process async");

        let output = if binary_path != bin_name {
            Command::new(&binary_path)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        } else {
            // Try cargo fallback
            tracing::warn!(tool = bin_name, "no pre-built binary found, using cargo run");
            Command::new("cargo")
                .args(["run", "--quiet", "-p", crate_name, "--bin", bin_name, "--"])
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        };

        let output = match output {
            Ok(o) if o.status.success() || !o.stdout.is_empty() => o,
            _ => {
                tracing::error!(tool = bin_name, "tool execution failed");
                return Err(EngineError::ToolExecutionFailed(format!(
                    "Tool {} failed or produced no output",
                    bin_name
                )));
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let (data, error) = match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(json) => (json, None),
            Err(_) => {
                let trimmed = stdout.trim();
                if trimmed.is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    (serde_json::Value::Null, Some(format!("No output. stderr: {}", stderr.trim())))
                } else {
                    (serde_json::json!({ "raw": trimmed }), None)
                }
            }
        };

        Ok(cogent_common::ToolResult {
            tool: bin_name.to_string(),
            success: error.is_none() && output.status.success(),
            duration_ms,
            data,
            error,
            suggested_fix: None,
            auto_fix_available: None,
        })
    }

    fn name(&self) -> &'static str {
        "DefaultAsyncToolRunner"
    }
}

impl Default for DefaultAsyncToolRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_runner_missing_binary() {
        let runner = DefaultAsyncToolRunner::new();
        let result = runner.run("nonexistent", "nonexistent-binary-xyz", &["--format", "json"]).await;
        assert!(result.is_err());
    }
}