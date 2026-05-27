---
name: cogent-http-debug-patterns
description: Debugging patterns for embedding Cogent CLI tools into in-process HTTP servers — payload field mismatches, status code misrouting, metrics handler omissions, and script path overrides
version: 1.1.0
author: Hermes Agent
license: OPL-1.1
metadata:
  hermes:
    tags: [cogent, http-debugging, axum, metrics, Cogent]
    related_skills: [cogent-explore, cogent-workspace]
---

# Cogent HTTP Debug Patterns

Use this skill when embedding Cogent tools into an in-process HTTP server (`cogent-server` or similar) and encountering common integration issues.

## When to use

- Tool results always zero/empty despite valid CLI output
- HTTP 500 responses for tools that should return structured pass/fail data
- `/metrics` endpoint returning empty body
- Helper scripts failing to locate binaries or readiness checks

## Root cause patterns

### 1. Wrong request payload field

**Symptom**: Tools return empty/default results via HTTP but CLI works.
**Cause**: `ToolRequest` has field `args`, not `input`. Using `{"input": {...}}` results in empty argument map; tools default to `path="."`, `recursive=false`.
**Fix**: POST JSON with `{"tool": "...", "args": {...}}`.

### 2. Business failures mapped to HTTP 500

**Symptom**: Tools that fail a quality check return HTTP 500 `{"error":"unknown error"}`.
**Cause**: Handler matches on `ToolResult { success: true, ... }` specially; `success: false` with `error: None` falls through to catch-all 500 branch.
**Fix**: Reorder match arms:

```rust
match result {
    ToolResult { error: Some(err), .. } => {
        // True system error → 500
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": err}))).into_response()
    },
    ToolResult { success, data, .. } => {
        if !success { inc_total_errors(); }  // still count
        (StatusCode::OK, Json(data)).into_response()  // always 200 with data
    }
}
```

### 3. Metrics handler builds body but never returns it

**Symptom**: `GET /metrics` returns HTTP 200 with empty body.
**Cause**: Function ends with `let body = format!(...);` — missing expression without semicolon.
**Fix**: Replace trailing `);` with `); body` before closing brace.

### 4. Helper script paths mismatched

**Symptom**: Scripts like `memory_profile.py` or `compare_transports.py` fail to start server or find binaries.
**Cause**: Scripts assume `target/release/cogent-server` relative to repo root; when `CARGO_TARGET_DIR=/tmp/...` the binary lives elsewhere.
**Fix**: Override `SERVER_BIN` and tool binary lookup to `/tmp/cogent-build/release/...` or make configurable via env vars.

## Verification checklist

- [ ] Check the actual `ToolRequest` struct definition — field is `args`, not `input`.
- [ ] Verify HTTP handler returns 200 for `success==false` (only 500 when `error: Some`).
- [ ] Ensure `metrics_handler` last expression is `body`.
- [ ] Update scripts to actual binary paths (`CARGO_TARGET_DIR` override).
- [ ] Prefer SSE `/tools/run_stream` for long-running tools; batch via `/tools/call_batch` for throughput.

## Related files

- `crates/cogent-server/src/main.rs` — HTTP handlers, metrics, batch orchestration
- `scripts/memory_profile.py` — RSS profiling under load
- `scripts/compare_transports.py` — direct binary vs HTTP in-process benchmark
