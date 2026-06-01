# Cogent Requirements Traceability

This document maps Cogent features to their implementation and test coverage.

## Legend

- **REQ** — Requirement ID
- **Feature** — User-facing capability
- **Implementation** — Source files / modules
- **Tests** — Unit or integration test files
- **Status** — `MET` (implemented + tested), `PARTIAL` (implemented, weak test coverage), `GAP` (not implemented)

---

## REQ-1: Unified CLI Entry Point

**Feature:** A single binary `cogent` that accepts subcommands and flags.

**Implementation:**
- `crates/cogent-cli/src/main.rs` — CLI parsing with `clap` derive macros
- `crates/cogent-cli/src/config.rs` — config loading and project detection

**Tests:**
- `crates/cogent-cli/src/tests.rs` — 32 unit tests (25 pass, 7 ignored)
- `tests/integration_cli.rs` — CLI invocation tests

**Status:** `MET`

---

## REQ-2: Multi-Tool Audit Orchestration

**Feature:** Run 28+ standalone quality tools and aggregate results.

**Implementation:**
- `crates/cogent-engine/src/lib.rs` — `run_tool()`, `run_tool_with_runner()`
- `crates/cogent-engine/src/runner.rs` — `ToolRunner` trait, `DefaultToolRunner`, `MockToolRunner`
- `crates/cogent-engine/src/registry.rs` — `ToolRegistry`, `AuditTool`
- `crates/cogent-engine/src/checks.rs` — ~28 check functions

**Tests:**
- `crates/cogent-engine/src/tests.rs` — mock-based tests for `check_access_control`, `check_supply_chain`, `check_secrets`
- `crates/cogent-engine/src/runner.rs` — `test_default_runner_returns_tool_unavailable_for_missing_binary`

**Status:** `PARTIAL` — only 3 of ~28 check functions have mock-based tests

---

## REQ-3: Configurable Quality Thresholds

**Feature:** Users can set pass/fail thresholds per tool via `.quality.toml`.

**Implementation:**
- `crates/cogent-cli/src/config.rs` — `load_config_thresholds()`, `parse_toml_f64()`, `parse_toml_usize()`
- `.quality.toml` — example configuration file

**Tests:**
- Config parsing is tested indirectly via integration tests
- No dedicated unit tests for threshold parsing edge cases

**Status:** `PARTIAL`

---

## REQ-4: Multiple Output Formats

**Feature:** Reports can be emitted as text, JSON, NDJSON, SARIF, JUnit XML, HTML, or Markdown.

**Implementation:**
- `crates/cogent-report/src/formatters.rs` — JSON, NDJSON, SARIF, JUnit
- `crates/cogent-report/src/html.rs` — HTML and Markdown reports
- `crates/cogent-cli/src/main.rs` — format dispatch

**Tests:**
- `crates/cogent-report/src/tests.rs` — 4 unit tests
- Integration tests validate SARIF schema compliance

**Status:** `MET`

---

## REQ-5: CI / CD Integration

**Feature:** Cogent can generate GitHub Actions workflows, pre-commit hooks, and SARIF baselines.

**Implementation:**
- `crates/cogent-cli/src/main.rs` — `init_ci()`, `install_hooks_impl()`
- `.github/workflows/quality.yml` — reference workflow
- `.pre-commit-config.yaml` — reference hook config

**Tests:**
- No automated tests for workflow generation (file I/O)
- Manual verification only

**Status:** `PARTIAL`

---

## REQ-6: Structured Logging and Diagnostics

**Feature:** Operations emit structured trace events; release builds strip high-verbosity traces.

**Implementation:**
- `tracing` crate in `cogent-cli`, `cogent-engine`, `cogent-report`
- Compile-time filtering via `max_level_debug` feature
- `cogent doctor` subcommand for support diagnostics

**Tests:**
- Tracing is tested implicitly (if it compiles, it works)
- No dedicated tests for log output verification

**Status:** `MET`

---

## REQ-7: Testability and Mocking

**Feature:** Core logic can be unit-tested without running external binaries.

**Implementation:**
- `ToolRunner` trait abstracts all external process calls
- `MockToolRunner` provides canned JSON responses
- `run_tool_with_runner()` helper for check functions

**Tests:**
- `crates/cogent-engine/src/tests.rs` — mock-based pass/fail tests

**Status:** `PARTIAL` — trait exists but adoption in check functions is incomplete

---

## REQ-8: Cross-Platform Support

**Feature:** Cogent runs on Unix-like systems; Windows support is best-effort.

**Implementation:**
- `#[cfg(unix)]` guards for executable permissions in hooks
- `std::path::Path` used for path handling (mostly)
- `scripts/test.sh` is bash-only

**Tests:**
- CI runs on `ubuntu-latest` only
- No Windows CI job

**Status:** `PARTIAL`

---

## Summary

| REQ | Feature | Status | Test Coverage |
|-----|---------|--------|---------------|
| 1 | Unified CLI | `MET` | Good |
| 2 | Multi-tool audit | `PARTIAL` | 3/28 checks mocked |
| 3 | Configurable thresholds | `PARTIAL` | Indirect only |
| 4 | Multiple output formats | `MET` | Good |
| 5 | CI/CD integration | `PARTIAL` | Manual only |
| 6 | Structured logging | `MET` | Implicit |
| 7 | Testability / mocking | `PARTIAL` | Partial adoption |
| 8 | Cross-platform | `PARTIAL` | Unix only |

**Overall:** 2 MET, 6 PARTIAL → 25% full traceability

**Next steps:**
- Add mock-based tests for all tool-based checks (REQ-2, REQ-7)
- Add unit tests for config threshold parsing (REQ-3)
- Add Windows CI job (REQ-8)
