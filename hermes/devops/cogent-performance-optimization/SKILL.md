---
name: cogent-performance-optimization
title: Cogent Performance Optimization
category: devops
usage:
  - When optimizing tool-based CLI workspaces (linters, analyzers, formatters)
  - When migrating from process-spawning to in-process library execution
  - When implementing batch RPC interfaces with concurrent execution
  - When integrating UTCP (Universal Tool Calling Protocol) for tool discovery
description: Systematic approach to optimizing Rust-based tool workspaces by eliminating process spawn overhead, implementing concurrent batch operations, and adopting UTCP for standardized tool calling.
version: 1.0.0
triggers:
  - task involves optimizing performance of CLI tool wrappers
  - task involves reducing latency of tool invocation in a multi-tool system
  - user mentions Cogent, tree-sitter, or batch RPC patterns
  - user wants to integrate UTCP manual endpoint for tool discovery
---

## Summary

Optimization pattern for Rust tool workspaces that wrap external binaries (debt-scan, crap, dupfind, etc.). Achieves ~14x speedup by moving from `std::process::Command` spawning to in-process library linkage, then adds concurrent batch execution (~3x additional speedup), and finally integrates UTCP manual + HTTP transport for standard agent discovery.

**Three-phase escalation:**
1. **In-process linkage** — eliminate spawn + tree-sitter reinit overhead (14x improvement)
2. **Concurrent batch RPC** — parallelize CPU-bound work with `tokio::spawn` (3x improvement)
3. **UTCP standardization** — publish manual endpoint + HTTP transport plugin

**Core insight:** Every tool that can be structured as `lib.rs` + thin `main.rs` should be in-process. Only fall back to process spawning for uncooperative binaries. Batch operations must flatten results (always-return value-with-error-field) to avoid partial-failure cascades.

---

## Phase 1: In-Process Migration

### 1.1 Audit tool execution paths

Identify all tools using `std::process::Command` or similar spawn mechanisms. Catalogue:
- Tool name
- Current invocation method (spawn vs library call)
- Whether the tool crate exposes a library target (`lib.rs`)
- Dependencies that benefit from reuse (tree-sitter parsers, grammar caches)

Example output from Cogent:
```bash
debt-scan    → lib.rs present  → can be in-process
crap         → lib.rs present  → can be in-process
dupfind      → lib.rs present  → can be in-process
complexity   → spawn only      → keep process fallback
```

### 1.2 Split binary crates into lib + main

For each tool that exposes library functionality but executes as binary:
1. Create `src/lib.rs` with core logic exposed as public functions
2. Refactor `src/main.rs` into thin wrapper that calls lib
3. Move CLI argument parsing to main; keep analysis in lib
4. Ensure lib returns structured results (not just stdout)

Pattern:
```rust
// src/lib.rs
pub fn analyze(path: &Path) -> Result<AnalysisResult, AnalysisError> { … }

// src/main.rs
fn main() {
  let args = Cli::parse();
  let result = analyze(args.path);
  println!("{}", serde_json::to_string(&result).unwrap());
}
```

### 1.3 Wire in-process dispatch in server

Update central server (e.g., `cogent-server/src/main.rs`) to call library functions directly:

```rust
// Before: process spawn
let output = Command::new(tool).arg(path).output().await?;

// After: in-process call
match tool_name {
  "debt-scan" => debt_scan::analyze(path).await,
  "crap" => crap::compute(path).await,
  _ => bail!("unknown tool"),
}
```

**Verification:** Compare wall-clock time for single-tool invocation before and after. Expected: ~10-20x improvement due to no fork/exec + preserved grammar caches.

---

## Phase 2: Concurrent Batch RPC

### 2.1 Design flattened result type

Critical decision: **batch must never partially fail**. Change from `Result<Value, Error>` to `ToolResult` with separate `value` and `error` fields.

```rust
#[derive(Serialize, Deserialize)]
pub struct ToolResult {
  pub tool: String,
  pub ok: bool,           // true if success
  pub value: Option<Value>, // result on success
  pub error: Option<String>, // error message on failure
  pub duration_ms: u64,
}
```

Rationale: If one tool fails, others still return their results; aggregator sees individual `ok` flags instead of single `Result` that aborts entire batch.

### 2.2 Implement concurrent batch handler

```rust
pub async fn run_tool_batch(
  requests: Vec<ToolRequest>
) -> Vec<ToolResult> {
  let handles: Vec<_> = requests.into_iter()
    .map(|req| tokio::spawn(async move {
      run_single_tool(req).await
    }))
    .collect();

  // Join all concurrently
  let mut results = Vec::new();
  for handle in handles {
    match handle.await {
      Ok(result) => results.push(result),
      Err(join_err) => results.push(ToolResult {
        tool: "unknown".into(),
        ok: false,
        value: None,
        error: Some(format!("join error: {}", join_err)),
        duration_ms: 0,
      }),
    }
  }
  results
}
```

**Important:** `tokio::spawn` leverages full threadpool; CPU-bound tree-sitter work runs on blocking threads (`tokio::task::spawn_blocking`) inside individual tool implementations if needed.

### 2.3 Expose batch RPC endpoint

Add to server:
```rust
methods.add_method("tools/run_batch", |params| {
  let batch: ToolBatchRequest = params.parse()?;
  let results = if batch.parallel {
    run_tool_batch_parallel(reqs).await
  } else {
    run_tool_batch_sequential(reqs).await
  };
  Ok(serde_json::to_value(results).unwrap())
});
```

### 2.4 Benchmark and verify

Test with identical tool set:
- Sequential batch: ~3.0ms total (sum of individual times)
- Parallel batch: ~0.9ms total (max of individual times)

Expected speedup scales with number of tools: `O(n)` → `O(parallel_max)`.

**Unit tests required:**
- `test_tools_run_batch_sequential` — verifies correct ordering/concat
- `test_tools_run_batch_parallel` — verifies concurrent execution + flattening
- `test_tools_run_batch_partial_failure` — verifies one tool failure doesn't kill others

---

## Phase 4: Real-Time Progress Streaming via SSE

### 4.1 Add progress channels to tool libraries

**Goal:** Enable telemetry during long-running tool executions (mutation testing, fuzzing) without changing the tool's return type.

**Pattern:** Add an optional progress sender parameter to library entry points:

```rust
// In tool library (e.g., mutation-test/src/lib.rs)
pub async fn run_mutation_with_progress(
    config: MutationConfig,
    progress_tx: Option<UnboundedSender<ProgressEvent>>
) -> Result<MutationReport, MutationError> {
    // Emit progress at key milestones
    if let Some(tx) = &progress_tx {
        let _ = tx.unbounded_send(ProgressEvent {
            stage: "scanning".into(),
            current: 0,
            total: None,
            message: "Discovering mutant targets".into(),
        });
    }

    // Hook into existing loops
    for (i, file) in files.iter().enumerate() {
        if let Some(tx) = &progress_tx {
            let _ = tx.unbounded_send(ProgressEvent {
                stage: "analyzing".into(),
                current: i + 1,
                total: Some(files.len()),
                message: format!("Processing {}", file.display()),
            });
        }
        // ... existing per-file work
    }
}
```

**Key decisions:**
- Use `futures::channel::mpsc::unbounded()` instead of `std::sync::mpsc` — provides a `Stream` compatible with axum's SSE requirements
- `Option<UnboundedSender<>>` allows existing callers to remain unchanged (backward compatible)
- `unbounded_send()` returns `Result`; ignore send errors with `let _ = tx.unbounded_send(...)` (client disconnections are normal)

### 4.2 Wire progress channels through thin wrappers

Thin binary wrappers (`src/main.rs`) do NOT need modification — they continue calling the original library functions without the progress parameter. Progress is only used by the server/agent layer:

```rust
// mutation-test/src/main.rs — unchanged
fn main() {
    let config = Cli::parse().into();
    let report = analyze(config).await;  // calls original analyze(), not run_with_progress
    println!("{}", serde_json::to_string(&report).unwrap());
}
```

The server calls the progress-aware variant directly.

### 4.3 Implement SSE streaming endpoint

Add a new RPC method or HTTP route that spawns the tool with a progress channel and streams events:

```rust
// cogent-server/src/main.rs — axum 0.7
use axum::response::sse::{Sse, Event, KeepAlive};
use futures::stream::Stream;

#[cfg(feature = "http")]
async fn run_tool_stream_handler(
    Json(req): Json<ToolRequest>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let (tx, rx) = futures::channel::mpsc::unbounded();

    // Spawn tool execution on blocking pool
    let handle = tokio::spawn(async move {
        let result = match req.tool.as_str() {
            "mutation-test" => {
                run_mutation_with_progress(req.args, Some(tx.clone())).await
            }
            "fuzz-surface" => {
                run_fuzz_with_progress(req.args, Some(tx.clone())).await
            }
            _ => bail!("unknown tool"),
        };
        // Optionally send final result as event
        let _ = tx.unbounded_send(ProgressEvent::done(result));
    });

    // Convert receiver into SSE stream
    let stream = rx.inspect(|event| {
        // debug log optional
    }).map(|event| {
        Ok(Event::default().json(json!({
            "stage": event.stage,
            "current": event.current,
            "total": event.total,
            "message": event.message,
        })))
    });

    Sse::new(stream).keep_alive(KeepAlive::new().with_interval(Duration::from_secs(15)))
}
```

**Important axum 0.7 API notes:**
- SSE support is built into axum 0.7; no separate `sse` feature flag
- Use `Event::default()` not `Event::new()` — default constructor sets type="message" and id=None
- `Sse::new(stream)` accepts any `Stream<Item = Result<Event, E>>` where `E: Into<axum::Error>`
- Keep-alive ping interval recommended to prevent proxy timeouts

### 4.4 Add `futures` dependency to affected crates

```toml
# mutation-test/Cargo.toml, fuzz-surface/Cargo.toml, cogent-server/Cargo.toml
[dependencies]
futures = "0.3"
```

The `futures` crate provides `channel::mpsc::unbounded` and `Stream` extension traits.

### 4.5 Client consumption (curl / agent)

```bash
curl -N http://localhost:9879/tools/run_stream \
  -H "Content-Type: application/json" \
  -d '{"tool":"mutation-test","args":{"path":"."}}' \
  | while IFS= read -r line; do
      echo "[${line}]"
    done
```

Output is line-delimited SSE data lines:
```
data: {"stage":"scanning","current":0,"total":null,"message":"Discovering..."}
data: {"stage":"analysis","current":1,"total":42,"message":"Analyzing src/lib.rs"}
data: {"stage":"mutation","current":1,"total":150,"message":"b41k2 → b41k2!"}
data: {"done":true,"result":{…}}
```

**Agents:** Parse SSE stream line-by-line, emit progress callbacks upon receipt of each `data:` line, complete when a `done:true` event arrives.

### 4.6 Verification

**Unit tests:** Mock `UnboundedSender` and verify correct number of events emitted for each stage:
```rust
#[tokio::test]
async fn test_mutation_progress_events() {
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let config = MutationConfig::test();
    run_mutation_with_progress(config, Some(tx)).await.unwrap();

    let events: Vec<_> = rx.collect().await;
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| e.stage == "scanning"));
    assert!(events.iter().any(|e| e.stage == "analysis"));
}
```

**Integration test:** Start `cogent-server --http-port 9879`, call `/tools/run_stream` with fuzz-surface, verify ≥4 SSE data events received.

**Performance:** Streaming adds <1ms overhead per event when client consumes quickly; localhost network stack dominates when client is remote.

---

## Phase 3: UTCP Integration

### 3.1 Understand UTCP architecture

UTCP (Universal Tool Calling Protocol) replaces custom JSON-RPC with standardized tool discovery and transport plugins.

**Key components:**
- **Manual endpoint:** `GET /utcp` returns UTCP Manual (JSON) describing all available tools
- **Transport plugins:** HTTP, CLI, gRPC, MCP — tools declare which transports they support
- **Call templates:** Machine-readable invocation patterns for each transport

Philosophy: "no wrapper tax" — agents call tools directly via native protocol (HTTP/MCP) rather than through a central gateway.

### 3.2 Generate UTCP manual from tool catalog

Create module `utcp_manual.rs`:

```rust
pub fn generate_manual(tools: &[ToolCatalogEntry]) -> UtcpManual {
  UtcpManual {
    version: "1.1".into(),
    name: "Cogent".into(),
    description: "Cogent analysis toolkit".into(),
    tools: tools.iter().map(|tool| UtcpTool {
      name: tool.name.clone(),
      description: tool.description.clone(),
      input_schema: tool.args_schema.clone(), // JSON Schema
      output_schema: json_schema_for::<ToolResult>(),
      call_templates: vec![
        UtcpCallTemplate {
          transport: "http".into(),
          method: "POST".into(),
          url: format!("http://{{host}}:{{port}}/tools/call"),
          body_template: Some(serde_json::json!({
            "tool": "{{tool.name}}",
            "args": {{args}}
          })),
        },
        UtcpCallTemplate {
          transport: "cli".into(),
          command: format!(""cogent {tool.name} --path {{path}}"),
        },
      ],
    }).collect(),
    transports: vec!["http", "cli"],
  }
}
```

**Manual validation:** Unit test that generated manual passes UTCP schema validation.

### 3.3 Add HTTP transport endpoint

Extend cogent-server with optional HTTP layer:

```rust
// New CLI flag: --http-port <port>
if let Some(port) = args.http_port {
  start_http_server(port, tool_catalog).await?;
}

// HTTP route handlers
{
  let app = axum::Router::new()
    .route("/utcp", get(handle_get_manual))
    .route("/tools/call", post(handle_tool_call))
    .route("/tools/call_batch", post(handle_batch_call))
    .into_make_service();
  axum::Server::bind(&addr).serve(app).await?;
}

async fn handle_tool_call(
  State(state): State<ServerState>,
  Json(req): Json<ToolRequest>,
) -> Json<ToolResult> {
  Json(run_single_tool(state, req).await)
}
```

**Routing:** UTCP manual at `GET /utcp` (root of UTCP spec). Tool calls at `POST /tools/call`. Optional batch at `POST /tools/call_batch`.

### 3.4 Keep existing transports

Do **not** remove stdio/TCP modes. Maintain backward compatibility:
- Default: server runs over stdio (existing)
- `--http-port` starts additional HTTP listener
- CLItransport always available (binaries on PATH)

UTCP manual declares all three: `["http", "cli", "stdio"]`.

### 3.5 Client migration path

For agents consuming Cogent:
1. `GET /utcp` → discover tools + schemas
2. Choose transport (HTTP preferred for performance)
3. POST tool calls with JSON payloads
4. Handle `ToolResult` with flattened `error` field

**No changes needed for stdio consumers** — they continue using existing protocol.

---

## Pitfalls & Lessons Learned

### Pitfall 1: Partial-failure cascades (fixed by result flattening)

**Problem:** Initial batch RPC returned `Result<Vec<Value>, Error>`. One tool failure returned early, dropping successful results from other tools.

**Solution:** Changed `run_single_tool` to return `ToolResult` directly. Batch returns `Vec<ToolResult>` where each entry has `ok` boolean. Caller decides how to handle partial failures.

**Code change:**
```rust
// Before
async fn run_single_tool(...) -> Result<serde_json::Value, Error>

// After
async fn run_single_tool(...) -> ToolResult  // always returns ToolResult { ok: bool, … }
```

### Pitfall 2: Tree-sitter grammar reinitialization (fixed by in-process)

**Problem:** Each process spawn reinitializes tree-sitter parsers, costing ~8-12ms per tool. With 5 tools, overhead dominated actual analysis time.

**Solution:** Link library directly. Parsers live in shared memory; grammar loads once at server startup. Process spawn eliminated entirely for in-process tools.

**Evidence:** Single-tool call: 22ms (spawn) → 1.5ms (in-process). Batch of 3 tools: 66ms (sequential spawn) → 0.9ms (parallel in-process).

### Pitfall 3: UTCP spec misinterpretation

**Initial assumption:** UTCP replaces the server entirely → use MCP plugin only.
**User correction:** "No, UTCP https://www.utcp.io/" — UTCP is the standard protocol; cogent-server should publish manual + optionally host HTTP transport, not disappear.

**Adjustment:** Server persists as UTCP Manual provider + optional HTTP endpoint; stdio/TCP modes remain for backward compatibility.

### Pitfall 10: Cron jobs build on FAT32 or restricted dirs

**Observed:** `cargo` build-script permissions errors when building in workspace `target/` (located on FAT32 mount).

**Workaround:** Set `CARGO_TARGET_DIR=/tmp/cogent-build` env var for builds triggered from Hermes. Failing that, use `cargo build --target-dir /tmp/cogent-build`.

---

## Phase 3: UTCP Integration (implementation learned)

### 3.1 UTCP Manual generation

Create a feature-gated module `utcp_manual.rs` that reads the existing `tool_catalog()` and converts each `ToolCatalogEntry` into a UTCP tool definition.

Key types (Serialize only, no Deserialize needed):

```rust
#[derive(Serialize)]
pub struct UtcpManual {
    pub manual_version: String,   // e.g., "0.1.0"
    pub utcp_version: String,     // e.g., "1.1"
    pub info: Option<UtcpInfo>,
    pub tools: Vec<UtcpTool>,
}

#[derive(Serialize)]
pub struct UtcpTool {
    pub name: String,
    pub description: String,
    pub inputs: serde_json::Value,           // JSON Schema from catalog.args_schema
    pub outputs: Option<serde_json::Value>,  // ToolResult JSON Schema
    pub tool_call_template: UtcpCallTemplate,
}

#[derive(Serialize)]
#[serde(tag = "call_template_type")]
pub enum UtcpCallTemplate {
    Http(UtcpHttpTemplate),
    Cli(UtcpCliTemplate),  // optional, for documentation
}

#[derive(Serialize)]
pub struct UtcpHttpTemplate {
    call_template_type: &'static str,  // "http"
    pub url: String,                   // relative path (host supplied by client)
    #[serde(rename = "http_method")]
    pub method: String,
    pub content_type: Option<String>,
    pub body_field: Option<String>,    // None → whole body is ToolRequest
    pub header_fields: Option<Vec<String>>,
    pub auth: Option<UtcpAuth>,
}
```

Build and conversion: `UtcpManual::from_tool_catalog()` clones `tool_catalog()`, maps each entry to an HTTP call template (`POST /tools/call`), leaves `body_field = None` because the full request body is the `ToolRequest` struct (this matches axum's `Json<ToolRequest>` extractor), and sets content-type to `application/json`.

**Why both Http and Cli variants?** UTCP spec expects at least one transport; Cli is kept as documentation even if not served. It can be removed later or filled if adding a CLI transport bridge.

### 3.2 HTTP server with axum (feature-gated)

Cargo features:

```toml
[dependencies]
axum = { version = "0.7", optional = true }
tower-http = { version = "0.5", features = ["cors"], optional = true }
http = { version = "1", optional = true }

[features]
default = ["tcp"]
tcp = []
stdio = []
http = ["axum", "tower-http", "dep:http"]
```

Server entry in `main()`:

```rust
#[cfg(feature = "http")]
if cli.http_port > 0 {
    let addr = SocketAddr::from(([0, 0, 0, 0], cli.http_port));
    println!("cogent-server HTTP listening on {}", addr);
    let app = Router::new()
        .route("/utcp", get(utcp_manual_handler))
        .route("/tools/call", post(call_tool_handler))
        .route("/tools/call_batch", post(call_batch_handler))
        .with_state(QualityServerState);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
    return;  // blocks forever
}
```

Handler implementations:

```rust
#[cfg(feature = "http")]
async fn utcp_manual_handler(State(_): State<QualityServerState>) -> impl IntoResponse {
    let manual = UtcpManual::from_tool_catalog();
    Json(manual)  // axum serializes with correct Content-Type
}

#[cfg(feature = "http")]
async fn call_tool_handler(
    State(_): State<QualityServerState>,
    Json(req): Json<ToolRequest>,
) -> impl IntoResponse {
    match run_single_tool(req, Instant::now()).await {
        ToolResult { success: true, data, .. } => (StatusCode::OK, Json(data)).into_response(),
        ToolResult { success: false, error: Some(err), .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": err }))).into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "unknown" }))).into_response(),
    }
}

#[cfg(feature = "http")]
async fn call_batch_handler(
    State(_): State<QualityServerState>,
    Json(batch): Json<ToolBatchRequest>,
) -> impl IntoResponse {
    // run_tool_batch expects Option<Value> from RPC; wrap it
    let payload = serde_json::to_value(&batch).unwrap();
    match run_tool_batch(Some(payload)).await {
        Ok(serde_json::Value::Array(arr)) => (StatusCode::OK, Json(arr)).into_response(),
        Ok(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "batch returned non-array" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e }))).into_response(),
    }
}
```

**State type:** Empty struct `#[derive(Clone)] struct QualityServerState;` — axum requires the state type to be `Clone + Send + Sync + 'static`. With no shared resources yet, empty unit-like struct works; derive `Clone` satisfies the bound.

**Important:** `Json<ToolRequest>` sent by client becomes a `ToolRequest` value. `run_single_tool` already accepts `ToolRequest`. On success we return `Json(data)` (the `data` field inside `ToolResult`) rather than the full `ToolResult` envelope, matching UTCP's expectation that the tool returns its domain result directly. HTTP error codes distinguish tool failure (500) from success (200).

### 3.3 Testing and validation

**Unit tests (http feature enabled):**
- `test_manual_has_http_templates` — ensures all tools have HTTP call templates with `POST /tools/call`
- `test_manual_serializes` — validates JSON structure contains `manual_version`, `utcp_version`, `tools[0]` fields

**Integration test via curl:**
```
cargo build -p cogent-server --features http
/tmp/cogent-build/debug/cogent-server --http-port 9878 &
curl -s http://localhost:9878/utcp | jq '.tools[].name'
# → lists all tools
curl -X POST http://localhost:9878/tools/call \
  -H "Content-Type: application/json" \
  -d '{"tool":"debt-scan","args":{"path":"."}}'
# → {"details":…,"message":"…","passed":true}
curl -X POST http://localhost:9878/tools/call_batch \
  -H "Content-Type: application/json" \
  -d '{"tools":[{"tool":"complexity","args":{"path":"."}},{"tool":"debt-scan","args":{"path":"."}}],"parallel":false}'
# → [{"data":{…},"success":true,…},{"data":{…},"success":true,…}]
```

### 3.4 Backward compatibility guarantees

- **Default build** (`cargo build`) produces a binary with **only** stdio/TCP modes. No additional dependencies are pulled.
- **HTTP mode** (`--http-port`) requires building with `--features http`. The flag is ignored if the feature is not enabled at compile time.
- Existing JSON-RPC over stdio/TCP is unchanged; agents can continue using the original protocol.
- UTCP manual dynamically reflects whatever is in `tool_catalog()`. Adding tools to the catalog automatically makes them visible via HTTP without extra work.

---

## Summary of results

Completed three-phase optimization of Cogent:
1. **In-process execution** (~14× speedup, grammar warm caches shared)
2. **Concurrent batch RPC** (~3× additional speedup on multi-tool runs)
3. **UTCP 1.1 HTTP endpoint** for standardized agent discovery and invocation (`GET /utcp`, `POST /tools/call`, `POST /tools/call_batch`)

Build is feature-gated; default mode retains original stdio/TCP behavior. All tests pass; workspace builds clean with `--all-features`.

**Solution:** Declare dependencies with `optional = true`, then reference with `dep:name`:

```toml
[dependencies]
axum = { version = "0.7", optional = true }
tower-http = { version = "0.5", features = ["cors"], optional = true }
http = { version = "1", optional = true }

[features]
default = ["tcp"]
tcp = []
stdio = []
http = ["axum", "tower-http", "dep:http"]
```

Code side uses `#[cfg(feature = "http")]` at module, use, and function levels. Declare module conditionally in main.rs: `#[cfg(feature = "http")] mod utcp_manual;`.

**Evidence:** Default build (no features) produces zero warnings of unused optional code; `cargo build --features http` compiles full server.

### Pitfall 6: UTCP manual types should be Serialize-only

**Problem:** UTCP manual is *generated* by the server but never deserialized from client input. Marking structs with `#[derive(Serialize, Deserialize)]` caused trait-not-implemented errors in unit tests that round-tripped through `serde_json::from_str`.

**Solution:** Derive only `Serialize` (plus `Debug`, `Clone`). If a test requires round-trip, serde_json::Value parsing is sufficient; manual structure only needs to serialize.

```rust
#[derive(Debug, Clone, Serialize)]
pub struct UtcpManual { … }

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "call_template_type")]
pub enum UtcpCallTemplate {
    Http(UtcpHttpTemplate),
    Cli(UtcpCliTemplate),
}
```

**Note:** Unused enum variants (like `Cli`) are fine — they document potential transports. Suppress `dead_code` warnings or accept them as noise.

### Pitfall 7: Axum router state must be Clone

**Problem:** Compilation failed: `the trait bound 'S: Clone' is not satisfied` for Router::with_state.

**Solution:** Newtype struct deriving Clone:

```rust
#[derive(Clone)]
struct QualityServerState;
```

Empty struct is fine; state exists to allow future shared resources.

### Pitfall 8: Result flattening for batch consistency

**Initial design:** `run_tool_batch` returned `Result<Value, Error>` with an inner `Vec<Value>`. One tool's error short-circuited the entire batch.

**Correction:** `run_single_tool` returns `ToolResult` (not `Result`). `run_tool_batch` always returns `Vec<ToolResult>`. Each tool's success/failure is captured in its own result object.

**Endpoint mapping:** HTTP `POST /tools/call_batch` accepts `ToolBatchRequest`, converts to `Value`, calls `run_tool_batch(Some(batch_value))` (note the `Some` wrapper required by legacy RPC shape), returns raw JSON array of `ToolResult`.

### Pitfall 9: UTCP manual vs live tool catalog drift

**Problem:** Manual is generated from `tool_catalog()` function. If `tool_catalog` is missing entries (e.g., `complexity` was absent), those tools don't appear in UTCP discovery, making them invisible to agents.

**Mitigation:** Expand UTCP manual unit test to assert `tools.len() >= N` and spot-check expected names. Alternatively, generate `tool_catalog` from a single source-of-truth array to avoid forgetting entries during manual edits.

### Pitfall 10: Cron jobs build on FAT32 or restricted dirs

**Observed:** `cargo` build-script permissions errors when building in workspace `target/` (located on FAT32 mount).

**Workaround:** Set `CARGO_TARGET_DIR=/tmp/cogent-build` env var for builds triggered from Hermes. Failing that, use `cargo build --target-dir /tmp/cogent-build`.

---

## Verification Checklist

- [ ] **In-process migration:** All tools with `lib.rs` use direct function calls; no `Command::new` for those tools
- [ ] **Benchmark suite:** Measure before/after for single-tool and batch scenarios
- [ ] **Batch RPC tests:** At least 2 tests covering sequential + parallel modes
- [ ] **Result flattening:** Every `ToolResult` has `ok` bool; batch never short-circuits
- [ ] **UTCP manual endpoint:** `GET /utcp` returns valid UTCP 1.1 JSON
- [ ] **Transport declarations:** Manual lists all available transports (http/cli/stdio)
- [ ] **HTTP call endpoint:** `POST /tools/call` works for at least one tool
- [ ] **Backward compatibility:** Existing stdio/TCP clients continue functioning when HTTP disabled
- [ ] **Documentation:** README updated with UTCP discovery flow and HTTP usage

---

## When to Apply

Use this skill when:
- You have a Rust workspace of CLI tools that wrap external analysis binaries
- Process spawn overhead is the primary latency source
- You need to support concurrent batch execution of multiple tools
- You want to publish a standardized discovery mechanism (UTCP) for agent consumption

**Not appropriate when:**
- Tools must remain fully isolated (security sandbox required)
- Tools are written in languages without library linkage (pure shell scripts)
- Single-tool latency is already sub-millisecond (optimization cost exceeds benefit)

---

## Related Skills

- `Cogent-workspace` — workspace setup and build configuration
- `rust-cli-sdk-wiring` — general pattern for extracting libraries from CLI binaries
- `origin-layer-consolidation` — similar in-process migration pattern for crypto crates
- `agent-work-dag` — dependency management for agent task orchestration (batch RPC consumer side)