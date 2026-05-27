# Cogent — AI Agent Quick Reference

This project uses **Cogent** for automated code quality analysis.
Run `cogent init` once and it handles everything else automatically.

## Zero-Config Bootstrap

```bash
# Detect project type and write .quality.toml with language-tuned thresholds
cogent init

# Full CI bootstrap: config + GitHub Actions workflow + pre-commit hook + baseline SARIF
cogent init --ci
```

## Command Reference

| Command | What it does | When to use |
|---------|-------------|-------------|
| `cogent init` | Detect ecosystem, write `.quality.toml` | First time setup |
| `cogent init --ci` | Full CI wiring (GHA + hook + baseline) | Repo bootstrap |
| `cogent check .` | Run all checks, auto-loads `.quality.toml` | Before PR / after changes |
| `cogent check . --format json` | Machine-readable check results | Agent consumption |
| `cogent watch .` | Watch changes, run tests + coverage + checks | Local dev loop |
| `cogent watch . --no-tests` | Watch without running tests | Fast metrics-only loop |
| `cogent install-hooks` | Install full pre-commit hook (tests + coverage + check) | Once per repo |
| `cogent install-hooks --fast` | Install lightweight hook (metrics only) | Fast commit workflow |
| `cogent run . --format sarif` | Full 10-tool batch audit | CI / deep audit |
| `cogent run . --format json` | Full audit, JSON output | Agent / pipeline |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | One or more checks failed |
| `2` | Error (binary not found, parse failure) |

## Agent Consumption Pattern

```bash
result=$(cogent check . --format json)
if [ $? -eq 0 ]; then
  echo "Quality passed"
else
  echo "$result" | jq '.checks[] | select(.passed==false) | {name, message, score, threshold}'
fi
```

## Tool Priority for AI Agents

1. **`cogent check .`** — fast gate, auto-uses `.quality.toml` thresholds
2. **`cogent crap`** — find high-risk functions (CRAP > 30 = fix before proceeding)
3. **`cogent mutate`** — verify tests catch mutations (requires passing `cargo test`)
4. **`cogent debt`** — zero tolerance for TODO/FIXME/HACK markers
5. **`cogent riskmap`** — identify complex + churned files (bug hotspots)

## Individual Tool Commands

```bash
cogent crap ./src --recursive --format json
cogent debt ./src --recursive --format json
cogent doccov ./src --recursive --format json
cogent mutate . -p <crate-name> --max-mutants 5 --format json
cogent riskmap . --format json
cogent taint ./src --recursive --format json
```

## Output Formats

| Flag | Use Case |
|------|----------|
| `--format json` | Programmatic / agent parsing |
| `--format sarif` | GitHub Security tab upload |
| `--format ndjson` | Streaming / incremental pipeline |
| `--format text` | Human-readable terminal |

## Key Notes

- `cogent check .` **automatically loads** `.quality.toml` — no threshold flags needed
- CLI flags **override** `.quality.toml` values when provided
- `watch` **auto-detects** test runner (Cargo, pytest, jest/vitest, go test)
- `mutate` requires `cargo test` to pass on the original code first
- Use `-p crate-name` with `mutate` in workspace repos

## MCP Server

```bash
cogent-server --mode stdio   # MCP stdio server for Claude Desktop, Cursor, Windsurf
```

```json
{ "mcpServers": { "cogent": { "command": "cogent-server", "args": ["--mode", "stdio"] } } }
```

## See Also

- `docs/utcp/cogent.json` — UTCP tool definitions
- `cogent discover --format json` — live tool catalog
- `docs/utcp-integration.md` — MCP / Hermes integration guide
