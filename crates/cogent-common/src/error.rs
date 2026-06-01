//! Cogent error types using `thiserror`.

use thiserror::Error;

/// Top-level error type for Cogent operations.
#[derive(Error, Debug)]
pub enum CogentError {
    /// I/O error (file not found, permission denied, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization / deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A required tool binary is missing or unavailable.
    #[error("Tool unavailable: {tool}")]
    ToolUnavailable { tool: String },

    /// A check failed its threshold.
    #[error("Check '{name}' failed: {message}")]
    CheckFailed { name: String, message: String },

    /// Invalid configuration or CLI argument.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Generic error with a message.
    #[error("{0}")]
    Other(String),
}
