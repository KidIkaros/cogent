---
name: cogent-explore
version: 1.1.0
description: Explore and understand the Cogent repo structure, AST parsing implementation, headless API types, and multi-language toolchain.
category: devops
license: OPL-1.1
metadata:
  hermes:
    tags: [cogent, code-quality, ast-parsing, tree-sitter, multi-language]
    related_skills: [cogent-workspace, cogent-performance-optimization]
---

# Cogent Explore

Guide to the Cogent repository structure, core crates, and tool invocation patterns.

## Overview

The Cogent project is a Cargo workspace (formerly Cogent) containing:

- **Core crates**:
  - `cogent-common` — shared utilities, coverage helpers, CLI output formatting
  - `ast-parse` — Rust-specific cyclomatic complexity and lcov parsing (syn-based)
  - `ast-parse-ts` — tree-sitter powered universal AST parser for 15 languages
- **Tools** (individual binary crates):
  - `crap-metric`, `mutation-test`, `debt-scan`, `doc-coverage`, `duplication`,
  - `coupling`, `risk-map`, `prop-cov`, `fuzz-surface`, `taint-scan`
- **CLI aggregator**: `cogent-cli` (binary name: `cogent`) — unified entry point
- **HTTP server** (optional): `cogent-server` — JSON-RPC/HTTP API with SSE streaming

## Key Components

### 1. AST Parsing (`ast-parse-ts`)

Universal source file parsing via tree-sitter grammars (pure Rust, no external deps).

**Supported languages**: Rust, Python, JavaScript, TypeScript, Go, C, C++, C#, Java, PHP, Ruby, Swift, Kotlin, Solidity, OCaml.

**Entry points**:
- `parse_file(path)` → AST + language detection
- `find_functions(node)` → walks statements, identifies function declarations
- `compute_complexity(node)` → cyclomatic complexity per function

**Shared utilities** (`cogent-common/src/lib.rs`):
- `find_source_files(path, recursive, extensions)` — multi-extension discovery
- `parse_lcov(content)` → `LcovCoverage` with DA records for per-function coverage
- `crap_score(complexity, coverage_pct)` → maintenance risk score
- `truncate`, `separator`, `estimate_line` — formatting helpers

### 2. Headless API Types (`cogent-common/src/lib.rs`)

Structs used to wrap tool execution results:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolRequest {
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub tool: String,
    pub version: String,
    pub success: bool,
    pub duration_ms: u64,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
```

### 3. Tool Execution Pattern

**Modern unified invocation** (recommended):

```bash
# Full audit with SARIF output (GitHub Security tab compatible)
cogent run . --format sarif > results.sarif

# JSON output for programmatic consumption
cogent run . --format json

# Single tool (direct binary)
cargo run -p crap-metric -- ./src --recursive --format json

# Single tool (via cogent subcommand)
cogent crap ./src --recursive
```

**Legacy `quality-cli` style** (pre-rebrand):
```bash
cargo run --bin cogent-cli --tool crap-metric --path ./src/main.rs
```

## Tool Discovery

The `cogent discover --format json` command outputs all available tools, their input schemas, output fields, and rule IDs. Useful for AI agent integration.

## Usage Pattern

1. **Explore repo structure**: Read `crates/cogent-common/src/lib.rs` for shared utilities (file discovery, coverage parsing, CRAP formula).
2. **Understand implementation**: Inspect individual tool crates (`crap-metric/src/lib.rs`, `mutation-test/src/lib.rs`, etc.) for focused logic.
3. **Run tools**:
   - Quick gate: `cogent run . --format table`
   - Deep audit: run individual binaries (`crap`, `mutate`, `debt`, etc.)
4. **Verify results**: Ensure tools compile with `cargo clippy`, test edge cases (empty dirs, missing coverage, symlink paths).

## New Features (since Cogent)

- **Multi-language support** via tree-sitter (15 languages)
- **Unified CLI** `cogent run` with JSON/SARIF/Table output
- **Streaming NDJSON** for incremental AI pipeline processing
- **Tool discovery** via `cogent discover`
- **HTTP server** (`cogent-server`) with batch RPC and SSE streaming
- **Property test coverage** (`prop-cov`) and **fuzz surface analysis** (`fuzz-surface`)
- **Standardized output fields** (`severity`, `help`, `rule_id`) across all tools

## Example Workflow

```bash
# 1. Build entire workspace
cargo build --workspace

# 2. Run full audit
./target/debug/cogent run . --format json

# 3. Run specific tool (CRAP only)
./target/debug/crap ./src --recursive

# 4. Run mutation testing on a specific crate
CARGO_TARGET_DIR=/tmp/build cargo run -p mutation-test -- . -p my-crate --max-mutants 20

# 5. Discover tools (for agent integration)
cogent discover --format json
```
