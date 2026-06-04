# Tracing & Observability

## Purpose

Cogent emits structured tracing spans for every check execution, enabling performance profiling and observability. By default, traces are written to stderr. With the optional OpenTelemetry feature, spans can be exported to any OTLP-compatible collector (Jaeger, Grafana Tempo, Honeycomb, etc.).

## How it works

1. On startup, `cogent` initializes a `tracing-subscriber` with an `EnvFilter`
2. `#[tracing::instrument(level = "info")]` spans are placed on all key functions:
   - `dispatch` — top-level command routing (records the command name)
   - `run_check_subcommand` — full check run (records path, format, recursive flag)
   - `run_parallel_checks` — parallel execution (records number of checks)
   - `workspace_fingerprint` — cache fingerprint computation
   - `prune_stale_entries` — stale cache cleanup
   - `enforce_max_size` — cache size enforcement
3. Individual `tracing::info!` / `tracing::warn!` calls record cache hits, expired entries, and errors
4. Without the OpenTelemetry feature, all output goes to stderr
5. With the OpenTelemetry feature and `OTEL_EXPORTER_OTLP_ENDPOINT` set, spans are also exported via OTLP/gRPC

## Configuration

### `RUST_LOG` environment variable

Controls which spans are emitted. The default is `cogent=info,warn`, which means:
- `info`-level spans from the `cogent` crate are visible
- `warn`-level for everything else

```bash
# Default — info-level spans from cogent
cogent check .

# Debug-level — includes per-tool execution spans
RUST_LOG=cogent=debug cogent check .

# Trace everything (very verbose)
RUST_LOG=trace cogent check .

# Only warnings (suppress all tracing spans)
RUST_LOG=warn cogent check .

# Debug for cogent, info for specific dependencies
RUST_LOG=cogent=debug,tower=info cogent check .
```

### Span levels

| Level | What's recorded |
|-------|----------------|
| `info` | Command dispatch, check run lifecycle, cache hits, expired entries, stale pruning |
| `debug` | Per-tool execution (`run_tool`), individual file scanning |
| `warn` | Cache write failures, directory cleanup errors |
| `error` | Serialization failures, report output errors |

### Environment variables reference

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `cogent=info,warn` | Controls tracing verbosity. Uses the standard `env_filter` directive syntax. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | *(unset)* | When set (e.g. `http://localhost:4317`), enables OTLP span export. Only works with `--features opentelemetry`. |

## OpenTelemetry export (optional)

The OpenTelemetry feature exports spans to any OTLP-compatible collector. It is **not** enabled by default.

### Building with OpenTelemetry

```bash
# Build with OTLP support
cargo build --release -p cogent-cli --features opentelemetry
```

This adds the following optional dependencies:
- `opentelemetry` — core API
- `opentelemetry_sdk` — SDK with Tokio runtime
- `opentelemetry-otlp` — OTLP exporter (gRPC/tonic)
- `tracing-opentelemetry` — bridge between `tracing` and OpenTelemetry
- `tokio` — async runtime for the gRPC exporter

### Running with an OTLP collector

```bash
# Start your OTLP collector (e.g. Jaeger, Grafana Tempo)
# Then set the endpoint and run cogent:
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
  cogent check .

# With debug-level spans for detailed profiling
RUST_LOG=cogent=debug \
  OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
  cargo run --release -p cogent-cli --features opentelemetry -- check .
```

### Quick start with Jaeger

```bash
# 1. Start Jaeger with OTLP support (Docker)
docker run -d --name jaeger \
  -e COLLECTOR_OTLP_ENABLED=true \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest

# 2. Build cogent with OpenTelemetry
cargo build --release -p cogent-cli --features opentelemetry

# 3. Run checks with tracing
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
  ./target/release/cogent check .

# 4. View traces at http://localhost:16686
```

### Quick start with Grafana Tempo

```bash
# 1. Start Tempo (see https://grafana.com/docs/tempo/latest/getting-started/)
# Ensure it listens on port 4317 for OTLP gRPC

# 2. Build and run
cargo build --release -p cogent-cli --features opentelemetry
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
  ./target/release/cogent check .
```

## Architecture

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  cogent CLI │────▶│ tracing-subscriber│────▶│ stderr (fmt)    │
│  (spans)    │     │  + EnvFilter      │     └─────────────────┘
└─────────────┘     │                   │
                    │  ┌─ fmt layer ────▶│──▶ stderr (always)
                    │  │                 │
                    │  └─ OTel layer ──▶│──▶ OTLP collector (optional)
                    └──────────────────┘
```

- The **fmt layer** always writes to stderr (controlled by `RUST_LOG`)
- The **OTel layer** is only added when `--features opentelemetry` is enabled AND `OTEL_EXPORTER_OTLP_ENDPOINT` is set
- If OTLP initialization fails, cogent falls back to fmt-only tracing (no error exit)

## Performance impact

| Mode | Overhead |
|------|----------|
| Default (`cogent=info,warn`) | Negligible — a few microseconds per span |
| `RUST_LOG=cogent=debug` | Low — more spans recorded but not exported |
| `RUST_LOG=warn` (suppress spans) | None — spans are filtered at the subscriber level |
| OTLP export enabled | Low — spans exported synchronously on completion |

## Examples

```bash
# Profile which checks are slowest
RUST_LOG=cogent=debug cogent check . 2>&1 | grep "running tool"

# Export to Jaeger for visual flame graphs (binary must be built with --features opentelemetry)
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
  cogent check .

# CI: suppress tracing noise, only show warnings
RUST_LOG=warn cogent check . --ci

# Combine with cache to profile cache hits vs misses
RUST_LOG=cogent=info cogent check . 2>&1 | grep -E "cache hit|expired"
```

## See also

- `docs/tools/cache.md` — incremental cache (also emits tracing spans)
- `cogent check .` — run all quality checks
- `cogent run .` — full batch audit
- [tracing crate documentation](https://docs.rs/tracing)
- [OpenTelemetry Rust](https://opentelemetry.io/docs/languages/rust/)
