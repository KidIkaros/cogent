# cogent-cli

Unified headless CLI for Cogent.

## What's inside

- `main.rs` — CLI entry point, subcommand dispatch, and orchestration
- `config.rs` — project detection, `.quality.toml` loading, threshold parsing
- `formatters.rs` — terminal UI helpers (progress bars, box drawing, colorized output)

## Key commands

| Command | Purpose |
|---------|---------|
| `cogent run <path>` | Run full audit suite |
| `cogent check <tool> <path>` | Run a single tool |
| `cogent setup` | Initialize project (config, CI, hooks) |
| `cogent discover` | List available tools |
| `cogent serve` | HTTP server for report viewing |
| `cogent doctor` | Dump diagnostic info for support |

## Architecture

The CLI is intentionally thin:

1. Parse CLI args (`clap`)
2. Load config (`config.rs`)
3. Delegate to `cogent-engine` for tool execution
4. Format results via `cogent-report`

Business logic lives in `cogent-engine`; formatting lives in `cogent-report`.
