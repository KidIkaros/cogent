//! Cogent Protocol — JSON-RPC types and method definitions
//!
//! This crate defines the wire protocol for Cogent quality checking.
//! It contains only data types and method definitions — no logic.

#![deny(clippy::all)]
#![warn(missing_docs)]

pub mod error;
pub mod methods;
pub mod types;

pub use error::{ProtocolError, ProtocolResult};
pub use methods::*;
pub use types::*;

/// Current protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// JSON-RPC 2.0 version string
pub const JSONRPC_VERSION: &str = "2.0";