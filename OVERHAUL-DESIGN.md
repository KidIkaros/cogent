# Cogent Overhaul Design — Six-Nines Quality Analysis

> Applied framework: "High-Quality Software Engineering" (Drysdale 2005-2007)
> Date: 2026-05-30
> Scope: Full architectural overhaul of cogent audit toolchain

---

## 1. Diagnosis — Current State Assessment

### 1.1 Structural Metrics

| Metric | Value | Assessment |
|--------|-------|------------|
| Total lines | 56,739 | Large — manageable with decomposition |
| Crates | 30 (28 tool crates + cli + common) | Good separation by concern |
| main.rs | 11,463 lines | **CRITICAL** — god module |
| audit.rs | 2,045 lines | Acceptable (replacer module) |
| lib.rs (common) | 1,102 lines | Healthy |
| Subcommand dispatch arms | 46 | Monolithic match in single function |
| Functions in main.rs | ~65 | Too many for one file |
| `match`/`if let`/`if` in main.rs | 793 | Cyclomatic complexity overflow |
| `.clone()` calls | 85 | Borrow checker fighting, not designing |
| `.unwrap()`/`.expect()` in main.rs | 46 | Error handling debt |
| Debt markers (TODO/FIXME/HACK) | 24 across 3 core files | Moderate |
| Integration tests | 37 | Good count, but all in 3 files |
| Unit tests in main.rs | 0 | **CRITICAL** — untestable by design |

### 1.2 Six-Nines Principle Violations

#### Maintainability (Chapter 1)
**FAIL.** main.rs at 11,463 lines is unmaintainable. No single developer can hold this in working memory. The 65+ functions range from CLI parsing to business logic to report formatting to CI generation — they have no cohesive responsibility.

#### Black Box Principle (Chapter 3.1)
**FAIL.** `CheckResult` is a 300-line struct used as a god-bag. Every function in main.rs takes it or produces it. There is no interface boundary between "run a check" and "format its output" and "apply a fix." All three concerns reach into the same struct fields directly.

#### Component Responsibility (Chapter 3.2.1)
**PARTIAL.** The 28 tool crates are well-separated (each ~300-500 lines, single responsibility). But `cogent-cli` violates this by being 4 crates worth of logic crammed into one:

1. **CLI parsing + dispatch** (argparse, subcommand routing)
2. **Check orchestration** (running tools, aggregating results)
3. **Reporting + formatting** (dashboard, JSON, summary boxes)
4. **Fix engine** (replacer, patch application, validation)
5. **Config management** (TOML parsing, thresholds, CI generation)

These are 5 distinct responsibilities living in one `main()`.

#### Minimizing Special Cases (Chapter 3.2.2)
**FAIL.** The 46-arm `match` on `Commands` has repeated patterns — every `Commands::X` handler does the same dance: parse args, call `run_tool()`, build `CheckResult`, format output. This is a special-case explosion where the pattern should be data-driven.

#### Design for Testability (Chapter 6.5)
**FAIL.** Zero unit tests in main.rs. The 37 integration tests call the compiled binary as a black box. You cannot unit-test any of the 65 functions because they're all in `fn main()` scope with no `pub` visibility. The fix engine we just built has zero test coverage — we can only test it by running the binary.

#### Diagnostics (Chapter 3.2.4)
**MISSING.** No structured logging, no tracing, no health checks. The `run_with_spinner` function is the only observability.

---

## 2. Root Cause Analysis

The core disease is **cogent-cli is a monolith masquerading as a crate**.

The 28 tool crates are architecturally sound — each is a focused ~400-line binary that does one check well. The problem is everything *around* them: the orchestration, formatting, configuration, and fix engine are all piled into a single 11K-line main.rs with no module boundaries.

This creates a cascade:
- Untestable (no public API to test)
- Unmaintainable (no one can reason about 11K lines)
- Special-case heavy (46 match arms instead of a registry)
- Poor error handling (46 `.unwrap()` calls in application logic)

---

## 3. Overhaul Design

### 3.1 Decompose cogent-cli into Library Crates

Split `cogent-cli` into 4 focused crates + the existing thin binary:

```
cogent-cli/              (thin binary — just calls dispatch)
cogent-engine/           (orchestration, tool registry, result aggregation)
cogent-report/           (formatting: dashboard, JSON, summary boxes, diffs)
cogent-config/           (TOML parsing, thresholds, CI generation, project detection)
cogent-fix/              (replacer engine, FixPatch, all fixers, validation)
```

The existing `cogent-common` stays as-is — it's healthy.

#### 3.1.1 cogent-engine (Orchestration)

Extract from main.rs lines 1881-2086 + 4230-6500:
- `CheckResult`, `CheckReport`, `CheckSummary` structs
- `run_tool()`, `run_batch()` functions
- Tool registry (data-driven, not match-arm-driven)
- `Finding`, `Evidence`, `SuggestedFix` types

```rust
// cogent-engine/src/lib.rs

/// A tool that can be run as part of an audit check.
pub trait AuditTool: Send + Sync {
    /// The crate binary name (e.g., "secrets", "debt-scan")
    fn bin_name(&self) -> &str;
    /// Human-readable name for display
    fn display_name(&self) -> &str;
    /// Default threshold key
    fn threshold_key(&self) -> &str;
    /// Parse the tool's JSON output into a CheckResult
    fn parse_output(&self, raw: &str) -> CheckResult;
    /// Which ecosystem this tool supports
    fn ecosystem(&self) -> ProjectEcosystem;
}

/// Registry of all available audit tools
pub struct ToolRegistry { /* ... */ }

impl ToolRegistry {
    pub fn for_ecosystem(&self, eco: ProjectEcosystem) -> Vec<&dyn AuditTool> { /* ... */ }
    pub fn get(&self, name: &str) -> Option<&dyn AuditTool> { /* ... */ }
}
```

This replaces the 46-arm match with a data-driven dispatch:
```rust
// Instead of:
match command {
    Commands::Secrets { .. } => run_tool("secrets", "secrets", args, now),
    Commands::Debt { .. } => run_tool("debt-scan", "debt-scan", args, now),
    // ... 44 more arms
}

// You get:
let tool = registry.get(check_name)?;
let result = engine::run_tool(tool, args, config)?;
```

#### 3.1.2 cogent-report (Formatting)

Extract from main.rs lines 26-680 + 2027-2086 + 2649:
- `print_dashboard()`, `print_summary_box()`, `print_severity_grouped()`
- `print_fix_summary()`, `print_offenders()`
- `format_elapsed()`, `format_ms()`, `format_duration()`
- `output_json()`, `box_row()`, `ProgressBar`
- `FileSummary`, aggregation functions

```rust
// cogent-report/src/lib.rs

pub trait ReportFormatter {
    fn format_report(&self, report: &CheckReport) -> String;
    fn format_fix_patches(&self, patches: &[FixPatch]) -> String;
}

pub struct DashboardFormatter { /* terminal UI */ }
pub struct JsonFormatter { /* machine-readable */ }
pub struct DiffFormatter { /* unified diff */ }
pub struct QuietFormatter { /* summary counts only */ }
```

#### 3.1.3 cogent-config (Configuration)

Extract from main.rs lines 896-1034 + 2657-3085 + 3711-3798:
- `ProjectProfile`, `ProjectEcosystem`, `detect_project()`
- `ConfigSection`, `Thresholds`, TOML parsing
- `generate_config()`, `init_ci()`, `build_gha_workflow()`
- `load_config_with_overrides()`, `load_config_thresholds()`

#### 3.1.4 cogent-fix (Fix Engine)

Extract from audit.rs lines 1096-2045:
- `FixPatch`, `ApplyResult`, `Replacer`
- All 6 fixers (errhandle, deadcode, debt, secrets, crypto, doccov)
- `apply_patches()`, `format_diff()`, syn validation
- The `DocStubVisitor` and helper functions

This is already well-isolated from the audit check logic — it just needs its own crate.

### 3.2 Replace Monolithic Dispatch with Registry

The 46-arm `match Commands::` becomes:

```rust
// cogent-cli/src/main.rs (target: ~200 lines)
fn main() {
    let cli = Cli::parse();
    let config = cogent_config::load(&cli)?;
    let registry = cogent_engine::ToolRegistry::new();
    let formatter = cogent_report::formatter_for(cli.format)?;

    match cli.command {
        Commands::Check { .. } => {
            let report = cogent_engine::run_audit(&registry, &config)?;
            formatter.format_report(&report);
        }
        Commands::Fix { .. } => {
            let patches = cogent_fix::scan(&config)?;
            if cli.apply { cogent_fix::apply(&patches, &config)?; }
            else { formatter.format_fix_patches(&patches); }
        }
        Commands::Init { .. } => cogent_config::init(&config)?,
        // ... thin wrappers only
    }
}
```

Target: main.rs shrinks from 11,463 → ~300 lines.

### 3.3 Error Handling Overhaul

**Current state:** 46 `.unwrap()` / `.expect()` in application logic.

**Target:** Zero unwraps in application code. All errors flow through `anyhow::Result` or custom error types.

```rust
// cogent-engine/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Tool '{name}' failed with exit code {code}")]
    ToolFailed { name: String, code: i32 },
    #[error("Tool '{name}' not found in PATH")]
    ToolNotFound { name: String },
    #[error("Invalid tool output: {detail}")]
    ParseError { detail: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

Each crate gets its own error type via `thiserror`. The CLI binary uses `anyhow` for top-level error reporting.

### 3.4 Test Strategy (Chapter 6)

**Current state:** 37 integration tests (black-box binary testing), 0 unit tests.

**Target architecture:**

| Layer | Test Type | Count Target |
|-------|-----------|-------------|
| cogent-fix | Unit tests per fixer | 30+ (5 per fixer) |
| cogent-engine | Unit tests for registry, parse | 15+ |
| cogent-report | Snapshot tests for formatters | 10+ |
| cogent-config | Unit tests for TOML parsing | 10+ |
| Integration | Existing + fix engine E2E | 40+ |
| Property-based | proptest for fixers | 5+ |

**Testability design for fixers:**

```rust
// cogent-fix/src/errhandle.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwrap_to_question_mark() {
        let src = "let x = foo().unwrap();";
        let patches = fixer_errhandle(&[("test.rs".into(), src.to_string())]);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].new_text, "let x = foo()?;");
    }

    #[test]
    fn expect_with_quotes_to_map_err() {
        let src = r#"let x = foo().expect("failed");"#;
        let patches = fixer_errhandle(&[("test.rs".into(), src.to_string())]);
        assert_eq!(patches[0].new_text, r#"let x = foo().map_err(|e| format!("failed: {e}"))?;"#);
    }

    #[test]
    fn unwrap_in_test_fn_is_skipped() {
        let src = "#[test]\nfn test_foo() {\n    let x = foo().unwrap();\n}";
        let patches = fixer_errhandle(&[("test.rs".into(), src.to_string())]);
        assert_eq!(patches.len(), 0); // don't fix unwraps in test functions
    }
}
```

### 3.5 Fixer Precision Improvements

**Current debt fixer false positives:** Matches `todo:` inside struct field names (`DebtCount { todo: usize }`). Fix with context awareness:

```rust
// Before: naive string match
if line.contains(marker) { ... }

// After: context-aware — skip struct/enum definitions
if trimmed.contains(marker)
    && !trimmed.starts_with("pub ")  // skip declarations
    && !line_before.contains('{')     // skip struct fields
    && !is_inside_string_literal(line, marker_pos) { ... }
```

### 3.6 Observability (Chapter 3.2.4)

Add `tracing` spans to all crate operations:

```rust
use tracing::{info_span, instrument};

#[instrument(skip(tool, config))]
pub fn run_tool(tool: &dyn AuditTool, config: &Config) -> Result<CheckResult> {
    let span = info_span!("tool", name = tool.display_name());
    let _enter = span.enter();
    // ...
}
```

---

## 4. Migration Plan

### Phase 1: Extract Libraries (estimated: 3-4 days)

| Task | From | To | Lines Moved |
|------|------|----|-------------|
| 1. Create `cogent-fix` crate | audit.rs:1096-2045 | cogent-fix/src/ | ~950 |
| 2. Create `cogent-config` crate | main.rs:896-1034, 2657-3085, 3711-3798 | cogent-config/src/ | ~1,600 |
| 3. Create `cogent-report` crate | main.rs:26-680, 2027-2086, 2649 | cogent-report/src/ | ~1,400 |
| 4. Create `cogent-engine` crate | main.rs:1881-2086, 4230-6500 | cogent-engine/src/ | ~2,800 |
| 5. Thin `cogent-cli` main.rs | remaining glue | ~300 lines | ~11,200 removed |

Each extraction follows the same pattern:
1. Create new crate with `cargo init --lib`
2. Move types/functions
3. Add `pub` visibility
4. Wire dependency in workspace Cargo.toml
5. Build + test
6. Commit

### Phase 2: Tool Registry (estimated: 1-2 days)

1. Define `AuditTool` trait
2. Implement for all 28 tools
3. Replace 46-arm match with registry lookup
4. Add tool discovery (scan PATH, fallback to workspace crates)

### Phase 3: Error Handling (estimated: 1 day)

1. Add `thiserror` to each crate
2. Define error types per crate
3. Replace all `.unwrap()` / `.expect()` with `?`
4. CLI top-level uses `anyhow`

### Phase 4: Tests (estimated: 2-3 days)

1. Unit tests per fixer (30+)
2. Unit tests for engine registry (15+)
3. Snapshot tests for formatters (10+)
4. Property-based tests for fixers (5+)
5. Fix false positives in debt fixer

### Phase 5: Polish (estimated: 1-2 days)

1. Add `tracing` spans
2. Add `--format json` to all subcommands
3. Clean all Clippy warnings
4. Document public API with rustdoc
5. Fix `test_sbom_runs` bug

---

## 5. Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| main.rs lines | 11,463 | < 300 |
| Crates | 30 | 34 (+4 library crates) |
| Unit tests | 0 | 70+ |
| Integration tests | 37 | 40+ |
| `.unwrap()` in app logic | 46 | 0 |
| Match arms in dispatch | 46 | ~8 (top-level commands) |
| Build warnings | 8 | 0 |
| Cyclomatic complexity (main.rs) | 793 branches | < 50 |
| Fix engine false positive rate | ~15% (debt fixer) | < 5% |
| Test coverage (core crates) | ~0% | > 80% |

---

## 6. Architecture Decision Records

### ADR-001: Library Crate Decomposition

**Status:** Proposed

**Context:** cogent-cli main.rs is 11,463 lines with 65+ functions spanning 5 distinct responsibilities. Zero unit tests possible. The file exceeds human working memory for reasoning.

**Decision:** Decompose into 4 library crates (engine, report, config, fix) + thin CLI binary.

**Alternatives Considered:**
1. **Module split within cogent-cli** — Simpler, but still a single compilation unit. Tests remain in-process but the god-crate smell persists.
2. **Rewrite from scratch** — Tempting but wasteful. The 28 tool crates are good. Only the orchestration layer needs restructuring.
3. **Plugin architecture** — Over-engineered for current needs. The tool set is fixed.

**Consequences:**
- (+) Each crate independently testable
- (+) Compilation parallelized across crates
- (+) Clear dependency graph
- (-) More Cargo.toml files to maintain
- (-) Initial migration effort (3-4 days)

### ADR-002: Trait-Based Tool Registry

**Status:** Proposed

**Context:** 46 match arms dispatch commands, each doing the same pattern (parse args → run binary → parse output → format). Adding a new tool requires adding to the enum, the match, and the help text.

**Decision:** `AuditTool` trait + `ToolRegistry` struct. Tools register themselves. Dispatch is `registry.get(name)?.run()`.

**Alternatives Considered:**
1. **Keep match arms** — Works, but O(n) per tool added, no dynamic discovery.
2. **Inventory/ctor pattern** — Allows self-registration, but adds unsafe dependency.
3. **Build script generation** — Generate match arms from a manifest. Clever but opaque.

**Consequences:**
- (+) Adding a tool = implement trait + register
- (+) Runtime tool discovery
- (+) Eliminates 38 nearly-identical match arms
- (-) Slightly more boilerplate per tool (trait impl)
- (-) Runtime errors instead of compile-time if trait unimplemented

### ADR-003: Fix Engine Isolation

**Status:** Proposed

**Context:** The fix engine (2,045 lines in audit.rs) is mixed with audit check logic. Fixers should be independently testable and versionable.

**Decision:** Extract into `cogent-fix` crate with per-fixer modules.

**Consequences:**
- (+) Fixers can be unit-tested in isolation
- (+) Fix engine can be used as a library by other tools
- (+) Clean separation of concerns (auditing vs fixing)
- (-) One more crate in workspace
