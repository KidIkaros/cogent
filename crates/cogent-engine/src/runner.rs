//! Tool runner abstraction for testability.
//!
//! HQSE §6.5: wrap all system calls so fake ones can be substituted for testing.

#![deny(clippy::all)]

use cogent_common::{CogentError, ToolResult};
use std::time::Instant;
use tracing::{error, info, warn};

/// Abstract interface for running Cogent tool binaries.
///
/// Implementations can be real (calls `std::process::Command`) or mock
/// (returns canned JSON) for unit testing.
pub trait ToolRunner {
    /// Run a Cogent tool binary (or `cargo run` fallback) and return a [`ToolResult`].
    fn run(
        &self,
        crate_name: &str,
        bin_name: &str,
        args: &[&str],
        tool_start: Instant,
    ) -> Result<ToolResult, CogentError>;
}

/// The default production runner that spawns real subprocesses.
pub struct DefaultToolRunner;

impl ToolRunner for DefaultToolRunner {
    fn run(
        &self,
        crate_name: &str,
        bin_name: &str,
        args: &[&str],
        tool_start: Instant,
    ) -> Result<ToolResult, CogentError> {
        use std::process::{Command, Stdio};

        info!(tool = bin_name, crate = crate_name, args = ?args, "spawning tool process");
        let output = Command::new(bin_name)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let output = match output {
            Ok(o) if o.status.success() || !o.stdout.is_empty() => {
                info!(tool = bin_name, "tool binary succeeded");
                o
            }
            _ => {
                warn!(tool = bin_name, "binary not found or failed; falling back to cargo run");
                let cargo_output = Command::new("cargo")
                    .args(["run", "--quiet", "-p", crate_name, "--bin", bin_name, "--"])
                    .args(args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output();
                match cargo_output {
                    Ok(o) if o.status.success() || !o.stdout.is_empty() => {
                        info!(tool = bin_name, "cargo fallback succeeded");
                        o
                    }
                    Ok(_) => {
                        error!(tool = bin_name, "cargo fallback produced no output");
                        return Err(CogentError::ToolUnavailable {
                            tool: bin_name.to_string(),
                        });
                    }
                    Err(e) => {
                        let tool = bin_name.to_string();
                        error!(tool = bin_name, error = %e, "cargo fallback failed");
                        return if e.kind() == std::io::ErrorKind::NotFound {
                            Err(CogentError::ToolUnavailable { tool })
                        } else {
                            Err(CogentError::Io(e))
                        };
                    }
                }
            }
        };

        let duration_ms = tool_start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let (data, error) = match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(json) => (json, None),
            Err(_) => {
                let trimmed = stdout.trim();
                if trimmed.is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    (
                        serde_json::Value::Null,
                        Some(format!("No output. stderr: {}", stderr.trim())),
                    )
                } else {
                    (serde_json::json!({ "raw": trimmed }), None)
                }
            }
        };

        Ok(ToolResult {
            tool: bin_name.to_string(),
            success: error.is_none() && output.status.success(),
            duration_ms,
            data,
            error,
            suggested_fix: None,
            auto_fix_available: None,
        })
    }
}

/// A mock runner for unit tests that returns pre-canned output.
pub struct MockToolRunner {
    /// Maps `(bin_name, args_joined)` → canned JSON output.
    pub responses: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for MockToolRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MockToolRunner {
    /// Create a new mock runner with no canned responses.
    pub fn new() -> Self {
        Self {
            responses: std::collections::HashMap::new(),
        }
    }

    /// Add a canned response for a given tool + args key.
    pub fn with_response(mut self, key: &str, data: serde_json::Value) -> Self {
        self.responses.insert(key.to_string(), data);
        self
    }
}

impl ToolRunner for MockToolRunner {
    fn run(
        &self,
        _crate_name: &str,
        bin_name: &str,
        args: &[&str],
        tool_start: Instant,
    ) -> Result<ToolResult, CogentError> {
        info!(tool = bin_name, args = ?args, "MockToolRunner returning canned data");
        let key = format!("{}:{}", bin_name, args.join(":"));
        let data = self
            .responses
            .get(&key)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Ok(ToolResult {
            tool: bin_name.to_string(),
            success: true,
            duration_ms: tool_start.elapsed().as_millis() as u64,
            data,
            error: None,
            suggested_fix: None,
            auto_fix_available: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_runner_returns_tool_unavailable_for_missing_binary() {
        let runner = DefaultToolRunner;
        let result = runner.run(
            "nonexistent-crate",
            "nonexistent-binary-xyz",
            &["--format", "json"],
            Instant::now(),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            CogentError::ToolUnavailable { tool } => {
                assert_eq!(tool, "nonexistent-binary-xyz");
            }
            other => panic!("expected ToolUnavailable, got {:?}", other),
        }
    }

    #[test]
    fn test_mock_runner_returns_canned_data() {
        let runner = MockToolRunner::new().with_response(
            "secrets:--format:json",
            serde_json::json!({
                "summary": {
                    "findings_count": 3,
                    "critical": 1
                }
            }),
        );
        let result = runner
            .run("secrets", "secrets", &["--format", "json"], Instant::now())
            .unwrap();
        assert!(result.success);
        assert_eq!(
            result.data["summary"]["findings_count"],
            3
        );
    }
}
