# UTCP Integration Guide

Cogent is built around the **Universal Tool Calling Protocol (UTCP)**: agents call the CLI directly with no wrapper process. A separate `cogent-server` MCP shim exists only for GUI clients (Claude Desktop, Cursor, Windsurf) that cannot invoke CLI tools natively.

## Integration Architecture

```
Coding agents (Claude Code, Hermes, OpenCode, Codex, custom)
  └── UTCP / CLI  ← primary path — zero-dependency, full feature set
        ├── docs/utcp/cogent.json   (static manifest)
        └── cogent discover         (live catalog from binary)

GUI clients (Claude Desktop, Cursor, Windsurf)
  └── MCP shim  ← compatibility layer only
        └── cogent-server --mode stdio
```

Choose **UTCP/CLI** unless your client physically cannot run subprocess commands.

---

## UTCP — Primary Integration

### 1. Static Manifest

`docs/utcp/cogent.json` is the machine-readable UTCP manual. It contains every tool's call syntax, input/output schema, and description — everything an agent needs to invoke Cogent without any server.

```bash
cat docs/utcp/cogent.json
```

Programmatic usage:

```python
import json, subprocess

with open("docs/utcp/cogent.json") as f:
    manual = json.load(f)

for tool in manual["tools"]:
    print(tool["name"], "->", tool["call"]["syntax"])
```

### 2. Live Discovery

The binary itself is the authoritative source of truth:

```bash
cogent discover --format json   # machine-readable
cogent discover --format text   # human-readable
```

Use this in agent bootstrapping to confirm installed version and available tools.

### 3. Direct CLI Calls

All tools emit structured JSON — agents parse it, no server needed:

```bash
# Zero-config quality gate (auto-loads .quality.toml)
cogent check . --format json

# Individual tools
cogent crap ./src --recursive --format json
cogent debt ./src --recursive --format json
cogent riskmap . --format json

# Full batch audit
cogent run . --format sarif > results.sarif
```

### Agent-Specific Setup

**Claude Code / Codex** — `CLAUDE.md` at the repo root is loaded automatically. No setup needed.

**OpenCode** — `AGENTS.md` at the repo root is loaded automatically. No setup needed.

**Hermes** — install the skill:

```bash
cp -r hermes/ ~/.hermes/skills/cogent
```

The skill references `docs/utcp/cogent.json` directly and calls the CLI. See `hermes/SKILL.md` for full documentation.

---

## MCP — GUI Client Compatibility Shim

`cogent-server` wraps the CLI tools behind a JSON-RPC 2.0 / MCP interface. Use this **only** for GUI clients that have no native CLI tool support.

```bash
cogent-server --mode stdio   # for MCP clients
cogent-server --mode tcp --port 9876   # for TCP clients
```

The server exposes the same 10 tools as UTCP but adds latency (subprocess-per-call). It supports both MCP `tools/call` and the legacy `tools/run` method for backward compatibility.

### Claude Desktop

`~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "cogent": {
      "command": "cogent-server",
      "args": ["--mode", "stdio"]
    }
  }
}
```

### Cursor

`.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global):

```json
{
  "mcpServers": {
    "cogent": {
      "command": "cogent-server",
      "args": ["--mode", "stdio"]
    }
  }
}
```

### Windsurf

`~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "cogent": {
      "command": "cogent-server",
      "args": ["--mode", "stdio"]
    }
  }
}
```

---

## Tool Reference

| Command | Purpose | Output |
|---------|---------|--------|
| `cogent init` | Detect ecosystem, write `.quality.toml` | text |
| `cogent init --ci` | Full CI bootstrap (GHA + hook + baseline) | text |
| `cogent check . --format json` | All checks, auto-loads `.quality.toml` | JSON |
| `cogent run . --format sarif` | Full 10-tool batch audit | SARIF/JSON |
| `cogent crap ./src --recursive` | CRAP risk scores | JSON |
| `cogent mutate . -p pkg --max-mutants 5` | Mutation testing | JSON |
| `cogent debt ./src --recursive` | TODOs/FIXMEs | JSON |
| `cogent riskmap .` | Risk files (churn × complexity) | JSON |
| `cogent doccov ./src --recursive` | Doc coverage | JSON |
| `cogent taint ./src --recursive` | Security taint | JSON |
| `cogent coupling .` | Module dependencies | JSON |
| `cogent dupfind ./src --recursive` | Code duplication | JSON |
| `cogent fuzz ./src --recursive` | Fuzz surface | JSON |
| `cogent watch . --no-tests` | Live metrics watch loop | text |
| `cogent install-hooks --fast` | Lightweight pre-commit hook | text |

## CI/CD Integration

### GitHub Actions

```bash
cogent init --ci   # auto-generates workflow + pre-commit hook + baseline SARIF
```

Or add manually:

```yaml
- name: Quality Audit
  run: cogent run . --format sarif > results.sarif
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Pre-commit Hook

```bash
cogent install-hooks        # full hook (tests + coverage + check)
cogent install-hooks --fast # lightweight (metrics only)
```

## See Also

- [UTCP Specification](https://github.com/universal-tool-calling-protocol/utcp-specification)
- `docs/utcp/cogent.json` — UTCP manifest (all tools, schemas, call syntax)
- `CLAUDE.md` — Claude Code / Codex context (repo root)
- `AGENTS.md` — OpenCode / agentic harness context (repo root)
- `hermes/SKILL.md` — Hermes Agent skill definition