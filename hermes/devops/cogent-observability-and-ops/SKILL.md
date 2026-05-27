---
name: cogent-observability-and-ops
title: Cogent Observability & Operational Hardening
category: devops
usage:
  - Adding Prometheus-style metrics to an async Rust HTTP server
  - Limiting concurrent batch execution with semaphores to prevent OOM
  - Building operational CLI utilities (UTCP client, profiling scripts)
  - Configuring release LTO profiles and feature-gating optional transports
description: Instrument and harden a Cogent server with metrics endpoints, concurrency limits, profiling utilities, and client tooling after core in-process optimization is complete.
version: 1.0.0
triggers:
  - task involves adding metrics/monitoring to a Rust axum server
  - task involves preventing resource exhaustion in parallel tool execution
  - task requires building CLI utilities for server interaction
  - task requires release build hardening (LTO) and feature-gate discipline
---

## Summary

Once a tool workspace has been migrated to in-process execution and UTCP HTTP transport (Phase 5a–b), this skill adds the **observability and operational layer**: atomic Prometheus metrics, RAII request guards, semaphore-limited concurrency, profiling utilities, and release hardening. This is Phase 5c+ in the Cogent roadmap.

**Core insight:** Instrumentation must be non-intrusive and feature-gated. Use atomic counters for zero-cost metrics when disabled, and RAII guards to ensure accurate timing and cleanup. Concurrency limits protect against OOM from unbounded task spawning. Client utilities lower the barrier to server operation.

---

## 1. Atomic Metrics with RAII Guards

### 1.1 Define atomic counters at module level

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static TOTAL_CALLS: AtomicU64 = AtomicU64::new(0);
static TOTAL_ERRORS: AtomicU64 = AtomicU64::new(0);
static TOTAL_DURATION_MS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
```

Use `Relaxed` ordering — metrics are approximate anyway, and we want minimal overhead.

### 1.2 Helper functions

```rust
fn inc_total_calls() { TOTAL_CALLS.fetch_add(1, Ordering::Relaxed); }
fn inc_total_errors() { TOTAL_ERRORS.fetch_add(1, Ordering::Relaxed); }
fn add_duration_ms(ms: u64) { TOTAL_DURATION_MS.fetch_add(ms, Ordering::Relaxed); }
fn inc_active_connections() { ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed); }
fn dec_active_connections() { ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed); }
```

### 1.3 RAII guard for per-request timing

```rust
struct MetricsGuard {
    start: Instant,
}
impl Drop for MetricsGuard {
    fn drop(&mut self) {
        add_duration_ms(self.start.elapsed().as_millis() as u64);
        dec_active_connections();
    }
}
```

Guard is created at the start of each handler and lives until response completes.

### 1.4 Instrument handlers

```rust
#[cfg(feature = "http")]
async fn call_tool_handler(
    State(_state): State<QualityServerState>,
    Json(req): Json<ToolRequest>,
) -> impl IntoResponse {
    inc_total_calls();
    inc_active_connections();
    let _metrics_guard = MetricsGuard { start: Instant::now() };

    match run_single_tool(req, Instant::now()).await {
        ToolResult { success: true, data, .. } => (StatusCode::OK, Json(data)).into_response(),
        ToolResult { success: false, error: Some(err), .. } => {
            inc_total_errors();
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": err }))).into_response()
        }
        _ => {
            inc_total_errors();
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "unknown error" }))).into_response()
        }
    }
}
```

**Key:** Insert `inc_total_errors()` *before* building the error tuple to avoid nested-parenthesis pitfalls.

### 1.5 Expose `/metrics` endpoint (Prometheus text format)

```rust
#[cfg(feature = "http")]
async fn metrics_handler() -> impl IntoResponse {
    let (calls, errors, duration_ms, active) = get_metrics_snapshot();
    let body = format!(
        "# HELP quality_total_calls Total HTTP requests received\n\
         # TYPE quality_total_calls counter\n\
         quality_total_calls {}\n\
         # HELP quality_total_errors Total errors returned\n\
         # TYPE quality_total_errors counter\n\
         quality_total_errors {}\n\
         # HELP quality_total_duration_ms Total request processing time (ms)\n\
         # TYPE quality_total_duration_ms counter\n\
         quality_total_duration_ms {}\n\
         # HELP quality_active_connections Current active connections\n\
         # TYPE quality_active_connections gauge\n\
         quality_active_connections {}\n",
        calls, errors, duration_ms, active
    );
    (StatusCode::OK, body).into_response()
}
```

Add to router: `.route("/metrics", get(metrics_handler))`.

---

## 2. Concurrency-Limited Batch Execution

### 2.1 Problem: unbounded spawn can OOM

Naive parallel batch:
```rust
for tool_req in batch.tools {
    let h = tokio::spawn(async move { run_single_tool(tool_req).await });
    handles.push(h);
}
```
Spawns N tasks immediately — could exceed memory if N is large or tools are heavy.

### 2.2 Solution: semaphore-gated acquisition

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

const MAX_CONCURRENT: usize = 4;  // tune to your memory budget

let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
for tool_req in batch.tools {
    let permit = semaphore.clone().acquire_owned().await.unwrap();
    let h = tokio::spawn(async move {
        let result = run_single_tool(tool_req).await;
        drop(permit);  // release slot
        result
    });
    handles.push(h);
}
```

Each spawn holds a permit; when dropped, another task can acquire. This limits simultaneous tool execution to 4 (or whatever your system can handle).

**Important:** Use `Arc<Semaphore>` so each task gets its own `OwnedSemaphorePermit`.

### 2.3 Adjust MAX_CONCURRENT based on environment

If tools are memory-heavy (mutation testing), set lower (2–4). For lightweight tools (debt-scan, doc-coverage), raise to 8–16. Benchmark with `profile.py` to find sweet spot.

---

## 3. Client Utilities (scripts/)

### 3.1 UTCP client (`scripts/utcp_client.py`)

```python
#!/usr/bin/env python3
"""UTCP client for cogent-server — discovery, invocation, metrics, streaming."""
import argparse, json, sys, requests
BASE = None

def main():
    global BASE
    p = argparse.ArgumentParser()
    p.add_argument('--host', default='127.0.0.1')
    p.add_argument('--port', type=int, default=9879)
    sub = p.add_subparsers(dest='cmd', required=True)
    sub.add_parser('utcp')
    sub.add_parser('tools')
    sub.add_parser('metrics')
    run = sub.add_parser('run'); run.add_argument('tool'); run.add_argument('--arg', action='append')
    stream = sub.add_parser('stream'); stream.add_argument('tool'); stream.add_argument('--arg', action='append')
    args = p.parse_args()
    BASE = f"http://{args.host}:{args.port}"
    if args.cmd == 'utcp':
        print(requests.get(f"{BASE}/utcp").text)
    elif args.cmd == 'tools':
        print(json.dumps(requests.post(f"{BASE}/tools/call", json={"jsonrpc":"2.0","id":1,"method":"tools/list","params":None}).json(), indent=2))
    elif args.cmd == 'metrics':
        print(requests.get(f"{BASE}/metrics").text)
    elif args.cmd == 'run':
        tool_args = {}; [tool_args.update({k: json.loads(v) if v.lstrip('{').rstrip('}')==v else v}) for k,v in (a.split('=',1) for a in args.arg)]
        print(json.dumps(requests.post(f"{BASE}/tools/call", json={"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"tool":args.tool,"args":tool_args}}).json(), indent=2))
    elif args.cmd == 'stream':
        tool_args = {}; # same as run
        with requests.post(f"{BASE}/tools/run_stream", json={"tool":args.tool,"args":tool_args}, stream=True) as r:
            for line in r.iter_lines():
                if line.startswith(b'data:'):
                    try: ev = json.loads(line[5:]); print(f"[{ev.get('stage','?')}] {ev.get('progress_pct',0):.1f}% — {ev.get('message','')}")
                    except: pass

if __name__ == '__main__':
    try: main()
    except requests.exceptions.ConnectionError as e: print(f"ERROR: {e}", file=sys.stderr); sys.exit(1)
```

**Subcommands:**
- `utcp` — GET /utcp manual
- `tools` — JSON-RPC tools/list
- `metrics` — GET /metrics
- `run <tool> [--arg key=val]` — POST /tools/call
- `stream <tool>` — POST /tools/run_stream with SSE parsing

### 3.2 Performance profiler (`scripts/profile.py`)

```python
#!/usr/bin/env python3
"""Profile a command's CPU/memory usage (wall, user, sys, max RSS)."""
import subprocess, time, json, argparse, resource, sys

def profile(cmd, cwd=None):
    ru0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.perf_counter()
    proc = subprocess.Popen(cmd, cwd=cwd)
    code = proc.wait()
    t1 = time.perf_counter()
    ru1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {
        "command": " ".join(cmd), "exit_code": code,
        "wall_sec": t1 - t0,
        "user_sec": ru1.ru_utime - ru0.ru_utime,
        "sys_sec": ru1.ru_stime - ru0.ru_stime,
        "maxrss_kb": ru1.ru_maxrss,
        "cpu_pct": ((ru1.ru_utime-ru0.ru_utime)+(ru1.ru_stime-ru0.ru_stime))/(t1-t0)*100,
    }

def main():
    p = argparse.ArgumentParser()
    p.add_argument('command', nargs='+')
    p.add_argument('--cwd')
    p.add_argument('--json', action='store_true')
    args = p.parse_args()
    res = profile(args.command, cwd=args.cwd)
    if args.json: print(json.dumps(res, indent=2))
    else: print(f"Wall: {res['wall_sec']:.3f}s, CPU: {res['cpu_pct']:.1f}%, RSS: {res['maxrss_kb']} KB")

if __name__ == '__main__': main()
```

Run: `python3 scripts/profile.py cargo run -p mutation-test -- --path . --max-mutants 20`

Use `--json` for machine parsing (CI integration).

---

## 4. Release Build Hardening

### 4.1 `.cargo/config.toml` release profile

```toml
[profile.release]
lto = true           # link-time optimization across crates
codegen-units = 1   # single unit for maximum optimization
# Optional: strip = "debuginfo" to reduce size
```

### 4.2 Benchmarking with release builds

Always profile with `--release`:

```bash
CARGO_TARGET_DIR=/tmp/cogent-build cargo build -p cogent-server --release --features http
/tmp/cogent-build/release/cogent-server --http-port 9879
```

Or use the provided `scripts/profile.py` wrapper which automatically measures.

---

## 5. Feature-Gating Optional HTTP Transport

### 5.1 Cargo.toml feature flags

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
# ... common deps ...

axum = { version = "0.7", optional = true }
tower-http = { version = "0.5", features = ["cors"], optional = true }
http = { version = "1", optional = true }

[features]
default = ["tcp"]   # stdio/TCP only — no heavy HTTP deps
tcp = []
stdio = []
http = ["axum", "tower-http", "dep:http"]
```

### 5.2 Gate code with `#[cfg(feature = "http")]`

- `mod utcp_manual;` — module definition
- All `use axum::...` statements
- All HTTP handler functions (`async fn ..._handler`)
- The entire `if cli.http_port > 0 { ... }` startup block in `main()`

**Pattern:**

```rust
#[cfg(feature = "http")]
use axum::{Router, routing::{get, post}, Json, extract::State, response::IntoResponse, http::StatusCode};

#[cfg(feature = "http")]
mod utcp_manual;

#[cfg(feature = "http")]
async fn utcp_manual_handler(...) -> impl IntoResponse { ... }

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    #[cfg(feature = "http")]
    if cli.http_port > 0 {
        // build and serve HTTP
        return;
    }

    // fall through to stdio/TCP
}
```

### 5.3 Verify both configurations build

```bash
cargo check                           # default (no http)
cargo check --features http           # with HTTP
cargo test -p cogent-server          # default tests
cargo test -p cogent-server --features http -- --nocapture  # HTTP tests
```

---

## Pitfalls & Fixes

### Pitfall 1: Broken tuple when inserting call inside match arm

**Symptom:** Compiler error: `expected expression, found let statement` or `unexpected closing delimiter`.

**Cause:** Inserting `inc_total_errors();` directly inside a tuple expression without parentheses:
```rust
ToolResult { success: false, .. } => {
    inc_total_errors();                      // ← statement in expression position
    (StatusCode::INTERNAL_SERVER_ERROR, ...) // compiler thinks this is continuation of previous expr
}
```

**Fix:** Ensure the `StatusCode` tuple is parenthesized:
```rust
ToolResult { success: false, .. } => {
    inc_total_errors();
    (StatusCode::INTERNAL_SERVER_ERROR, Json(...)).into_response()
}
```
Also avoid duplicate inserted calls that create `(inc_total_errors(); inc_total_errors(); StatusCode...)` — clean up to single call.

### Pitfall 2: Missing semicolon after router chain

**Symptom:** `error: expected one of 'move', 'use', '{', '|', or '||', found keyword 'let'` — points to next line after `.with_state(QualityServerState)`.

**Cause:** Router chain line missing trailing semicolon:
```rust
let app = Router::new()
    .route(...)
    .with_state(QualityServerState)   // ← no semicolon
let listener = ...
```

**Fix:** Add semicolon after `.with_state(...);`.

### Pitfall 3: HTTP code compiled in default build

**Symptom:** Default `cargo check` fails with `cannot find module or crate 'axum'`.

**Cause:** `use axum::...` or HTTP handler function not behind `#[cfg(feature = "http")]`.

**Fix:** Wrap every axum-dependent item: use statements, module declarations, handler functions, and the `if cli.http_port` block. Ensure tests for HTTP are also gated with `#[cfg(feature = "http")]`.

### Pitfall 4: Metrics handler placed inside `main()`

**Symptom:** `error: expected expression, found fn` at the metrics handler definition.

**Cause:** Accidentally inserted `async fn metrics_handler()` inside `main()` body.

**Fix:** Extract the function and place at module level (before tests, after other handlers). Ensure it has `#[cfg(feature = "http")]`.

### Pitfall 5: Semaphore permit dropped too early

**Symptom:** Concurrent execution still unbounded despite semaphore code.

**Cause:** Permit dropped before task actually starts, or permit moved into task without being held during work.

**Fix:** Acquire permit **before** spawning, then move the permit into the task (so its Drop runs *after* tool completes):
```rust
let permit = semaphore.clone().acquire_owned().await.unwrap();
let h = tokio::spawn(async move {
    let _permit_guard = permit;  // held for entire async block
    run_single_tool(req).await
});
```

### Pitfall 6: Metrics guard never drops

**Symptom:** `active_connections` counter never decrements.

**Cause:** `MetricsGuard` stored in a variable that lives beyond request (e.g., `let _metrics_guard = ...` inside a closure that escapes). Or guard dropped only on success path, not error paths.

**Fix:** Ensure `MetricsGuard` is created at the very start of every handler, on all code paths. The guard's `Drop` runs when the function returns (whether via `into_response()` or panic). Verify in error arms that guard is still in scope (place guard before `match`).

### Pitfall 7: Unbounded channel backpressure ignored

**Symptom:** Progress SSE stream ends prematurely or events buffered indefinitely.

**Cause:** `tx.unbounded_send()` returns `Result`; if receiver dropped (client disconnected), send fails. Not handling the error may flood logs or panic in tests.

**Fix:** Ignore send errors intentionally: `let _ = tx.unbounded_send(event);`. This is normal — client disconnections are expected.

---

## Testing Verification

### Unit: metrics handler
```rust
#[tokio::test]
#[cfg(feature = "http")]
async fn test_metrics_endpoint() {
    let client = TestClient::new();
    // Hit endpoint twice
    client.get("/metrics").await;
    client.get("/metrics").await;
    let body = client.get("/metrics").await.text();
    assert!(body.contains("quality_total_calls 2"));
}
```

### Unit: concurrency limit
```rust
#[tokio::test]
async fn test_batch_concurrency_limit() {
    let semaphore = Arc::new(Semaphore::new(2));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        handles.push(tokio::spawn(async move {
            // sleep to simulate work
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(permit);
        }));
    }
    // Wait all — they should complete in ~500ms (5 batches of 2) not ~100ms
    for h in handles { h.await.unwrap(); }
}
```

### Integration: full server with metrics
1. Start `cogent-server --http-port 9879`
2. `curl http://localhost:9879/metrics` → baseline zeros
3. `curl -X POST http://localhost:9879/tools/call -d '{"tool":"debt-scan","args":{}}'`
4. `curl http://localhost:9879/metrics` → `total_calls 1`, `active_connections 0`, `duration_ms > 0`

### Integration: client utility
```bash
python3 scripts/utcp_client.py run debt-scan --arg path=.
python3 scripts/utcp_client.py stream mutation-test --arg max_mutants=10 | head -20
python3 scripts/utcp_client.py metrics
```

---

## When to Apply

**Apply this skill when:**
- You have an async Rust HTTP server (axum, warp, actix) that already serves tool results
- You need observability (request count, errors, latency, active connections)
- Your batch execution spawns tasks without limits and causes OOM
- You want CLI utilities to interact with the server without cargo runs
- You need release builds with maximum optimization

**Do NOT apply when:**
- The server is tiny and single-purpose (metrics overhead not justified)
- Concurrency is already limited at the caller side (e.g., single-agent)
- You cannot modify the server code (third-party binary only)
- Default build must stay dependency-free *and* you cannot feature-gate

---

## Related Skills

- `Cogent-performance-optimization` — prerequisite: in-process linkage, concurrent batch basics, UTCP manual endpoint
- `rust-cli-sdk-wiring` — extracting libraries from binaries (covered in the prerequisite)
- `Cogent-workspace` — workspace configuration and build setup
- `origin-layer-consolidation` — similar pattern of migration + instrumentation for crypto crates
