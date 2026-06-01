# cogent-engine

Audit orchestration engine for Cogent.

## What's inside

- `ToolRunner` trait — abstraction for running external tool binaries, enabling mock-based testing
- `DefaultToolRunner` — runs tools via `std::process::Command` with `cargo run` fallback
- `MockToolRunner` — canned responses for unit tests
- `run_tool()` / `run_tool_with_runner()` — execute a tool and parse JSON output
- `check_*` functions — ~28 quality checks (complexity, debt, secrets, coverage, etc.)
- `ToolRegistry` — discover and dispatch available tools

## Testability

All `check_*` functions have `_with_runner` variants that accept any `ToolRunner`:

```rust
use cogent_engine::{MockToolRunner, checks};

let runner = MockToolRunner::new().with_response("secrets:.:--format:json", json!({"summary": {"findings_count": 0}}));
let result = checks::check_secrets_with_runner(".", false, 5, &runner);
assert!(result.passed);
```

## Dependencies

- `cogent-common` — shared types
- `ast-parse-ts` — tree-sitter based AST parsing
- `syn` — Rust AST parsing for doc-coverage and complexity metrics
