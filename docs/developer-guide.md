# Developer Guide — Contributing to Cogent

This guide explains the project architecture, how to add new tools, and testing patterns.

## Project Architecture

```
Cogent/
├── crates/
│   ├── ast-parse-ts/      # Universal AST parsing (tree-sitter)
│   ├── cogent-common/     # Shared utilities (coverage, CRAP, file discovery)
│   ├── cogent-cli/        # Unified CLI (cogent command)
│   ├── cogent-server/     # HTTP API server
│   ├── crap-metric/        # CRAP score calculator
│   ├── debt-scan/          # Technical debt scanner
│   ├── doc-coverage/       # Documentation coverage
│   ├── duplication/        # Code duplication detector
│   ├── coupling/           # Module coupling analysis
│   ├── risk-map/           # Churn × complexity risk map
│   ├── mutation-test/      # Mutation testing (Rust-only)
│   ├── fuzz-surface/       # Fuzzable function identification
│   ├── prop-cov/           # Property test coverage
│   └── taint-scan/        # Taint analysis (data flow)
├── docs/                   # Documentation (user/developer guides)
├── .quality.toml           # Quality gate configuration
└── scripts/test.sh         # Safe test runner (batched)
```

## Key Crates

### `ast-parse-ts`
- Provides multi-language AST parsing via tree-sitter
- Supports 15 languages: Rust, Python, JS, TS, Go, C, C++, C#, Java, PHP, Ruby, Swift, Kotlin, Solidity, Vyper, OCaml
- Exports: `parse_complexity_file()`, `parse_doc_coverage_file()`, `Language` enum

### `cogent-common`
- Shared types: `ToolResult`, `UnifiedReport`, `CoverageRecord`
- Utilities: `find_source_files()`, `crap_score()`, `crap_category()`
- Memory monitoring: `MemoryMonitor` for CI environments

### `cogent-cli`
- Unified binary (`cogent`) that runs all tools in batch
- Subcommands: `check`, `run`, `history`, `init`, `install-hooks`
- Outputs: table, JSON, SARIF, NDJSON

## Adding a New Tool

1. **Create crate**:
   ```bash
   cargo new --bin crates/my-tool
   ```

2. **Use `ast-parse-ts` for multi-language support**:
   ```rust
   use ast_parse_ts::parse_complexity_file;
   ```

3. **Follow CLI pattern** (clap derive API):
   ```rust
   #[derive(Parser)]
   #[command(name = "my-tool", about = "Does something useful")]
   struct Cli {
       path: String,
       #[arg(short, long)]
       recursive: bool,
   }
   ```

4. **Support JSON output** (required):
   ```rust
   #[derive(Serialize)]
   struct Report { /* ... */ }
   let json = serde_json::to_string_pretty(&report)?;
   ```

5. **Add integration tests** in `tests/integration.rs`

6. **Update `Cargo.toml` workspace members** and `README.md`

## Testing Patterns

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_functionality() {
        assert_eq!(my_function(2), 4);
    }
}
```

### Integration Tests
```rust
// tests/integration.rs
use std::process::Command;

#[test]
fn test_cli_help() {
    let output = Command::new("target/debug/my-tool")
        .arg("--help")
        .output()
        .expect("Failed to execute");
    assert!(output.status.success());
}
```

### Property-Based Tests (proptest)
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_test(input in 0..100) {
        assert!(my_function(input) >= 0);
    }
}
```

## CI Workflow

The project uses GitHub Actions (`.github/workflows/quality.yml`):
1. **Build**: Compile all tools in release mode
2. **Test**: Run batched tests (`./scripts/test.sh`)
3. **Audit**: Self-audit with `cogent run .`, upload SARIF

## Quality Standards for Contributors

- All crates must have >90% test coverage
- Zero technical debt markers (TODO/FIXME/HACK)
- All public APIs documented (>95% doc coverage)
- Cyclomatic complexity <5 per function
- No code duplication >3 lines
- Clippy warnings = 0

Run `cogent run .` before submitting PR.
