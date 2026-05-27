---
name: agent-quality-workflow
description: End-to-end workflow for AI agents using code quality tools. Covers headless CLI design, coverage generation on constrained systems, CRAP metric interpretation, and the dogfood cycle.
version: 1.0.0
author: Hermes Agent
license: OPL-1.1
metadata:
  hermes:
    tags: [quality, agent-workflow, headless-cli, coverage, dogfood, ci]
    related_skills: [cogent-workspace, headless-cli-for-agents, state-machine, agent-work-dag]
---

# Agent Quality Workflow

Complete workflow for building, dogfooding, and using code quality tools as an AI agent.
Developed building Cogent (10 crates, 98 tests, 207 functions analyzed).

## The Dogfood Cycle

Run your tools on your own codebase FIRST, then fix what they find:

```
1. Build tool
2. Run tool on own code
3. Fix the worst offenders
4. Re-run tool
5. Repeat until passing own thresholds
```

This catches bugs in the tools themselves (the double-fn bug, symlink paths, UTF-8 panic
were all found by dogfooding).

## Key Bugs Found by Dogfooding

### Double-fn Bug (line numbers always 1)
The AST visitor calls `estimate_line(source, &name)` which prepends "fn " internally.
If visitor passes `&format!("fn {}", name)`, the pattern becomes "fn fn foo" which
never matches. ALL functions report line 1.

**Fix:** Visitor passes just the name, `estimate_line` adds "fn ".

### Symlink Path Mismatch
lcov uses resolved paths (`/media/mo/BUENO/`), tools use symlinks (`$HOME/`).
`find_coverage` uses `ends_with` which fails when directories differ.

**Fix:** Canonical path comparison via `std::fs::canonicalize`.

### LH Missing for Binary Crates
`cargo llvm-cov` sometimes produces LF (lines found) but no LH (lines hit) for binary
crates. The DA fallback only triggered when `lines_found == 0`, but LF exists without LH.

**Fix:** Trigger DA fallback when `lines_found == 0 || lines_hit == 0`.

### UTF-8 Truncate Panic
`&s[s.len() - max + 1..]` panics when slicing into multi-byte characters (emoji like "✓").

**Fix:** Use `is_char_boundary()` to find safe split points.

## Memory-Safe Coverage Generation

`cargo llvm-cov --workspace` OOMs on constrained systems (fills swap on 32GB RAM).

**Per-crate approach:**
```bash
export CARGO_BUILD_JOBS=2
for crate in "${CRATES[@]}"; do
    CARGO_TARGET_DIR=/tmp/cov-build \
    cargo llvm-cov --lcov -o "/tmp/$crate.info" -p "$crate" --tests --quiet
    cat "/tmp/$crate.info" >> coverage.info
    sync  # free memory between crates
done
```

**Key insight:** Each crate's coverage is small. Merging lcov files is trivial concatenation.

## Integration Tests Don't Capture Binary Coverage

`assert_cmd` runs the binary as a subprocess. `cargo llvm-cov` only captures coverage
from the TEST process, not the subprocess. Integration tests verify output but don't
contribute to coverage metrics.

**To get coverage on binary code:**
1. Extract logic to library crate (preferred)
2. Use `main() -> run()` pattern where `run()` is testable via unit tests
3. Accept that binary `main` functions will have low coverage

## CRAP Metric Interpretation

CRAP = comp^2 * (1 - coverage/100)^3 + comp

**Structural complexity ceiling:** Functions that handle N enum variants have cyclomatic
complexity >= N. A 17-arm match has complexity 17. With complexity 18, CRAP >= 18 even
at 100% coverage. This is a METRIC LIMITATION, not bad code.

**Practical thresholds:**
- CRAP <= 10: Excellent (most utility functions)
- CRAP <= 20: Good (CLI entry points with some branching)
- CRAP <= 30: Acceptable (complex parsers, output formatting)
- CRAP > 30: Needs refactoring or tests

**The normalize_expr pattern:** Split a 36-complexity match into:
- `normalize_simple_expr` (complexity 2, uses const lookup table)
- `expr_tag` (complexity 18 -- unavoidable for 17 variants)
- `TAG_TO_LABEL` (const array, zero runtime cost)

The lookup table approach gives CRAP 2 for the caller function, shifting the structural
complexity to a dedicated function that exists only to hold the match.

## Headless CLI Design for Agents

```rust
// Exit codes: 0=pass, 1=fail, 2=error
// JSON by default, text with --format text
// Thresholds as CLI flags: --max-crap 30 --min-doc 50
// Skip checks: --skip complexity,debt

cogent run ./src --max-crap 25 --min-doc 80 --skip complexity
// stdout: JSON with structured results
// exit: 0 if all checks pass, 1 if any fail
```

**Agent consumption pattern:**
```bash
# Agent calls this, parses JSON, checks exit code
result=$(cogent run ./my-project --max-crap 25)
if [ $? -eq 0 ]; then
    echo "Quality passed"
else
    echo "Quality failed: $(echo $result | jq '.checks[] | select(.passed==false)')"
fi
```

## Applying Quality-Tool Findings (The Refactor Loop)

After running cogent tools, convert findings into concrete fixes. This session pattern:

```
1. Run: dupfind, mutate, riskmap, coupling
2. Pick actionable clusters (skip architectural skeleton duplication)
3. Extract helpers → re-run dupfind → verify count drops
4. Extract pure functions + add count-assertion tests → kill mutation survivors
5. cargo test → commit
```

### Extracting hex-decode patterns

When dupfind reports repeated `hex::decode` + `len` check + `copy_from_slice`, extract a
const-generic helper once and replace all call sites:

```rust
// In shared command module
pub fn decode_hex_to_array<const N: usize>(hex: &str, field: &str) -> CliResult<[u8; N]> {
    let bytes = hex::decode(hex).map_err(|e| format!("invalid {field} hex: {e}"))?;
    if bytes.len() != N {
        return Err(format!("{field} must be {N} bytes ({} hex chars)", N * 2).into());
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}
```

Replaced 6 call sites across stealth, revoke, and disclosure commands.

### Killing mutation survivors in counter logic

When `mutate` shows survivors in counting code (e.g., `missing`, `invalid` computed by
`saturating_sub`), extract the computation into a pure function and add tests that assert
exact counts for known inputs:

```rust
pub fn compute_audit_stats(content: &str, audit_key: &[u8; 32])
    -> Result<(usize, usize, usize, usize, usize), String> { ... }

#[test]
fn test_compute_audit_stats_missing_hmac() {
    let (total, valid, missing, invalid, broken) = compute_audit_stats(content, &key).unwrap();
    assert_eq!(total, 2);
    assert_eq!(missing, 2);  // kills the >=0 -> >0 mutation
    assert_eq!(invalid, 0);  // kills the -0 -> -1 mutation
}
```

## Metric-Driven God Function Extraction

When cogent tools flag a single function with extreme complexity (CC > 50, CRAP > 1000),
the fastest remediation is extraction, not decomposition. Decomposition requires
understanding domain logic; extraction is mechanical.

### Target selection

Use the metrics together to pick the ONE function that matters most:

```
1. cogent run ./src --max-crap 30          → lists violations
2. riskmap ./src                              → shows churn × complexity heatmap
3. coupling ./src                             → identifies pure orchestration files (I≈1.0)
```

Priority: highest CRAP in a file with high churn (riskmap "DANGER ZONE"). Orchestration
files (zero internal reuse, I=1.0) are prime extraction candidates because they contain
no domain logic worth preserving — just dispatch.

### The `main() -> run()` extraction pattern

For a binary entry point with N command arms:

**Before:**
```rust
fn main() {
    // setup (tracing, env vars, gpg check)
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { .. } => { /* 50 lines */ }
        Commands::Stealth { .. } => { /* 40 lines */ }
        // ... 60 arms, 900+ lines total
    }
}
// CC 104, CRAP 10920
```

**After:**
```rust
fn main() {
    // setup (unchanged)
    let cli = Cli::parse();
    run(cli);
}

fn run(cli: Cli) {
    match cli.command {
        Commands::Init { .. } => { /* 50 lines */ }
        // ... same arms, same logic, same imports
    }
}
// main: CC 7, CRAP 56
// run:  CC 95, CRAP 9120
```

`cargo check` passes immediately. The entry point drops from "unmaintainable" to "trivial",
and `run()` becomes testable via unit tests (call `run(parsed_cli)` instead of spawning a
subprocess).

**Key insight:** CRAP and complexity move with the logic. The goal is NOT to eliminate
high complexity — a 60-arm dispatcher *should* have high complexity. The goal is to get
the complexity OUT of the untestable binary entry point so it can be unit-tested.

### Before/after measurement

Re-run the same cogent runs after extraction to validate the delta:

```bash
# Before
cogent run -p origin-cli  # Avg CRAP 50.9, 18 complex functions
# After
cogent run -p origin-cli  # Avg CRAP 45.6, 18 complex functions
```

Average CRAP drops even though the same 18 functions exist — because the worst one was
the entry point, and entry-point CRAP was inflating the average. The remaining violations
are now in handler functions that can be refactored independently.

### Incremental type migration with shim helpers

When a workspace-wide type migration (e.g., `MemoryTier` moved from `origin_pqc` to
`origin_core`) would create circular dependencies if forced across all crates at once,
introduce local conversion shims instead:

```rust
// In the crate that still uses the old type
use origin_core::tier::MemoryTier as CoreTier;

fn core_tier_to_pqc(tier: CoreTier) -> origin_pqc::MemoryTier {
    match tier {
        CoreTier::Hot => origin_pqc::MemoryTier::Hot,
        CoreTier::Warm => origin_pqc::MemoryTier::Warm,
        CoreTier::Cold => origin_pqc::MemoryTier::Cold,
    }
}
```

This avoids a big-bang refactor, keeps `cargo check` green, and the shim can be deleted
once downstream crates migrate to the canonical type.

### Coverage timeouts on large crates

`cargo llvm-cov` may TIME OUT (not just OOM) on crates with very large test binaries or
slow link times. When this happens, the CRAP tool falls back to `CC² + CC` (zero coverage
assumed), which wildly overstates risk. A function with CC 18 gets CRAP 342 instead of
the true value if it has 80% coverage (which would be ~36).

**Workaround:** Run coverage per-crate with generous timeouts, or accept that CRAP scores
for large crates are ceiling estimates until coverage is manually injected.

### `#[cfg(test)]` errors are invisible to `cargo check`

`cargo check` skips test compilation. After a refactor that changes signatures or types,
the library code may pass `cargo check` while tests fail with `cargo test`:

```
error[E0433]: failed to resolve: use of undeclared crate or module `origin_sdk`
  --> origin-cli/src/main.rs:111:9
```

These are pre-existing test rot, not regressions, but they block verification. Always run
`cargo test --no-run` (compile tests without running) after a refactor that touches types
used in test modules.

### Tool gotchas

- **FAT32 target dir:** Build scripts fail with "Permission denied" on FAT32 or shared
  drives. Fix: `CARGO_TARGET_DIR=/tmp/quality-target cargo build`
- **Async fn mutation testing:** The `mutate` tool may error on async functions when
  running in isolated temp builds (`async fn` is not permitted in Rust 2015). This is a
  tool limitation — verify with `cargo test` instead for async-heavy crates.
- **Dupfind group 1 noise:** Command-handler skeletons (let-bindings → if/call) always
  appear as duplication. Skip these; focus on domain-specific patterns (hex decode,
  file writes, JSONL append).

## Security Fix → Simulation → Quality Gate Pipeline

When applying security fixes (especially audit/logging/permission changes), run a
code-aware MiroFish simulation on the CHANGED files BEFORE declaring done. The
simulation catches regressions that unit tests miss.

**Session example (Origin Identity CLI):**
Five fixes applied, then 30-persona MiroFish simulation on diffs found:
- HIGH: stealth.jsonl created without mode restriction (leaked nonces via 0o644)
- MEDIUM: `let _ =` silently dropping audit failures on destructive ops
- MEDIUM: unbounded `std::fs::read()` in release sign (OOM vector)
- LOW: non-atomic output write, no git_commit validation, 1-hour DID expiry

**Fix loop:**
```
1. Apply security fixes
2. cargo check (catches signature changes across crates)
3. Run MiroFish on changed files
4. Address findings (repeat 1-3 if significant)
5. cargo clippy --all-targets -- -D warnings
6. cargo test --lib
7. Run cogent-cli (CRAP, debt, doc, complexity)
8. Commit only when all gates pass
```

**Cross-crate signature propagation:** Adding `actor: Option<&str>` to
`append_audit_entry()` broke 5 callers across 2 crates. Budget time for this.
`cargo check` immediately after signature changes prevents surprise failures.

## Building Cogent Alongside Agent Work Management

The agent work management system (state-machine, blocker-DAG, reply-verification, ratchet)
was used to manage the Cogent build itself:

1. Created KG entities for each task (task-shared-ast, task-crap-tool, etc.)
2. Added blocker edges (shared-ast blocks both tools, both tools block integration)
3. Used state machine to track lifecycle (pending -> in_progress -> completed)
4. Verified completion before proceeding to next task

This proved the system works under real development pressure.

## Cogent Integration Pipeline (Batch-First Approach)

When building a collection of cogent tools, shift from individual-tool validation to
**treating the entire suite as a single product** that must run cohesively. The goal:
all tools execute from one entry point, produce machine-readable output, and are
validated together.

### Pattern: Master Orchestration + E2E Batch Test

**Step 1: Audit the existing batch orchestrator** (e.g., `cogent-cli`'s `run_batch`)

```rust
// Before: only 7 of 9 tools wired up
match command {
    "crap" => run_crap(&path),
    "debt" => run_debt(&path),
    // ... missing: mutation-test, taint-scan
}
```

Check: Are all intended tools present? Do any tools require special flags (mutation-test
needs original tests to pass)? Are output formats unified (JSON/Table)?

**Step 2: Add missing tools to the batch**

```rust
// After: full suite
"mutation" => run_mutation(&path, "--max-mantle bar").unwrap_or_else(|_| empty_batch_result("mutation")),
"taint" => run_taint(&path, "--recursive", "--format", "json").unwrap_or_else(|_| empty_batch_result("taint")),
```

Decide upfront whether a failing tool should **fail the batch** (taint-scan) or
**gracefully degrade** (mutation-test can fail if code under test has flaky tests;
consider `--skip` for CI).

**Step 3: Fix integration-only bugs**

Some defects only appear when tools run together or from a different working directory:
- prop-cov attribute parsing: strict string matching failed for `#[quickcheck]` vs `#[ quickcheck ]`
- mutation-test file paths: relative paths break when run from workspace root vs. crate dir
- dupfind path canonicalization: symlink vs resolved path mismatches

Run the full batch locally after each addition. The integration surface is real.

**Step 4: Create an e2e batch validation test**

Add `tests/e2e.rs` to the orchestrator crate that runs the batch against the actual
workspace and validates structure:

```rust
#[test]
fn test_batch_runs_all_tools() {
    let output = Command::new("cargo")
        .args(["run", "-p", "cogent-cli", "--", "run", ".", "--format", "json"])
        .output()
        .expect("batch must run");

    let report: BatchReport = serde_json::from_slice(&output.stdout).unwrap();
    let tool_names: Vec<_> = report.results.keys().collect();

    assert_eq!(tool_names.len(), 9);
    assert!(tool_names.contains(&"crap".into()));
    assert!(tool_names.contains(&"mutation".into()));
    // … all tool presence assertions

    // Every tool must produce either pass or fail (not error/panic)
    for (name, result) in &report.results {
        assert!(!result.error, "tool {} panicked: {:?}", name, result.error);
    }
}
```

This test prevents regressions where someone removes a tool from the batch or breaks
the orchestration logic.

**Step 5: Build a master `check-all` script**

Combine test execution + batch analysis + result aggregation:

```bash
#!/bin/bash
set -euo pipefail

# Build + test
echo "=== cargo test --all-features ==="
export CARGO_TARGET_DIR=/tmp/cogent-build
cargo test --all-features

# Quality batch (JSON → summarization)
echo "=== cogent run . --format json ==="
cargo run -p cogent-cli -- run . --format json > /tmp/quality.json

# Exit non-zero if any check failed (mutation, taint, or tool panic)
FAILED=$(jq -r '.results[] | select(.passed == false) | .tool' /tmp/quality.json | wc -l)
if (( FAILED > 0 )); then
    echo "FAIL: $FAILED cogent runs failed"
    exit 1
fi

echo "PASS: all checks green"
```

Make it executable, document it in TESTING.md, and wire it as your pre-push hook.
This is your **canonical quality gate**.

### Environment Constraints (apply universally)

On constrained or mounted filesystems:
- `target-dir` MUST be `/tmp` or another exec-capable location (FAT32/VFAT cannot execute build scripts)
- `threads = 1` in `.cargo/config.toml` to prevent OOM during parallel test runs
- Use `CARGO_TARGET_DIR=/tmp/cogent-build` consistently across all commands

### Interpreting Batch Results

The unified JSON report lets you triage systematically:

```json
{
  "results": {
    "crap":         {"passed": true,  "score": 14.9},
    "debt":         {"passed": true,  "count": 0},
    "doc-coverage": {"passed": true,  "percent": 94.2},
    "complexity":   {"passed": true,  "violations": 3},
    "dupfind":      {"passed": false, "groups": 12},   // investigate
    "mutate":       {"passed": false, "score": 67.5}, // mutation survivors
    "taint":        {"passed": true},
    "coupling":     {"passed": false, "files": 4},
    "riskmap":      {"passed": false, "danger": 2}
  }
}
```

**Do not treat all failures equally:**
- **Mutation** (`mutate`): Surviving mutants may be **false positives** from indexing mutations in crypto code. Cross-check with known-answer tests before adding new tests.
- **Duplication** (`dupfind`): Structural duplicates (benchmark harness, `main()` patterns) are expected. Only copy-paste business logic requires action.
- **Complexity** (`complexity`): Functions with large match arms (17+ variants) inherently have CC ≥ N; these are metric ceilings, not bad code. Focus on branching logic complexity, not variant count.

**Decision rule:** If the batch fails, examine *why* each tool failed before blindly
"fixing" numbers. Some failures reflect **tool artifacts**, not code quality issues.

### When to Extend the Batch vs. Run Tools Individually

**Extend batch when:**
- Tool is fast (debt, doc-coverage, complexity, taint-scan)
- Tool output is easily normalized (JSON/Table)
- Tool provides holistic workspace insight

**Run individually when:**
- Tool is slow (mutation, riskmap with large git history)
- Tool requires selective targeting (`--files src/signing.rs`)
- Tool produces overwhelming output without filter flags

The batch is your **baseline gate**; individual tools are your **deep-dive lenses**.

### Incremental Batch Expansion

Start with a minimal batch (crap, debt, doc) and add tools one by one:

```
Batch v1:  crap, debt, doc-coverage      (fast, stable)
Batch v2:  + duplication, coupling        (medium speed)
Batch v3:  + riskmap (git-dependent)      (slow, optional)
Batch v4:  + mutation, taint              (slow, flaky)
```

Run the e2e test with each addition to catch regressions early. This prevents
the "batch sprawl" where the orchestrator bitrots because no one runs it regularly.
