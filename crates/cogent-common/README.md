# cogent-common

Shared types and utilities used across the Cogent workspace.

## What's inside

- `CheckResult`, `CheckReport`, `Finding` — core data structures for audit results
- `ToolResult` — structured output from individual tool binaries
- `CogentError` — unified error type for the engine and CLI
- SARIF output types (`SarifLog`, `SarifRun`, `SarifResult`, etc.)
- Helper functions: file scanning, health scoring, path utilities

## Usage

```rust
use cogent_common::{CheckResult, Finding};
```

This crate is **not** meant to be published standalone; it is an internal workspace dependency.
