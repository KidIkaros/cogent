---
name: cogent
description: AI-native code quality, security, and compliance audit toolkit - 21 automated checks for CI/CD pipelines and AI agents
version: 0.1.0
author: KidIkaros
license: Apache-2.0 OR OPL-1.1
platforms: [macos, linux]
metadata:
  hermes:
    tags: [Code-Quality, Rust, Metrics, Security, Testing, CI-CD]
    related_skills: [claude-code, opencode, codex]
    requires_toolsets: [terminal]
    fallback_for_tools: []
---

# cogent

21-check audit toolkit covering quality, security, and compliance. Designed for CI/CD pipelines and AI agents.

## When to Use

- User asks to audit code quality, security, or license compliance
- Before merging significant changes
- User asks about test coverage, risk, or hardcoded secrets
- Setting up CI/CD quality gates
- Generating an SBOM or checking OSS license exposure
- Evaluating code maintainability

## Quick Reference

| Command | Purpose |
|---------|----------|
| `cogent init` | Detect ecosystem, write `.quality.toml` |
| `cogent init --ci` | Full CI wiring (GHA + hook + baseline) |
| `cogent check .` | All 21 checks, auto-loads `.quality.toml` |
| `cogent check . --format json` | Machine-readable results for agents |
| `cogent check . --only sast,secrets,crypto` | Run specific checks only |
| `cogent check . --verbose` | Print file:line offenders for all checks |
| `cogent check . --ci` | CI mode: JSON out, no colors, exits 1 on fail |
| `cogent report .` | Generate HTML audit report |
| `cogent report . --open` | Generate + open in browser |
| `cogent diff old.json new.json` | Compare two check snapshots |
| `cogent watch . --no-tests` | Fast metrics-only watch loop |
| `cogent watch . --full` | Watch with all 21 checks |
| `cogent install-hooks --fast` | Lightweight pre-commit hook |
| `cogent run . --format sarif` | Full batch audit (SARIF output) |
| `cogent crap ./src --recursive` | CRAP scores only |
| `cogent mutate . -p {crate} --max-mutants 5` | Test quality |
| `cogent riskmap . --format json` | High-risk files |
| `cogent debt ./src --recursive` | TODOs/FIXMEs |
| `cogent doccov ./src --recursive` | Doc coverage |
| `cogent taint ./src --recursive` | Security taint |
| `cogent sbom .` | Generate SBOM (CycloneDX / SPDX) |

## Prerequisites

Build and install the binary:

```bash
cargo build --release
# Or install to PATH:
cargo install --path crates/cogent-cli
```

## Procedure

### 0. Zero-Config Setup (do once per repo)

```bash
cogent init        # detect ecosystem, write .quality.toml
cogent init --ci  # also wire GitHub Actions + pre-commit hook + baseline
```

### 1. Full Audit (Recommended for CI/CD)

```bash
# Run all 10 tools, output SARIF for GitHub Security tab
cogent run . --format sarif > results.sarif

# Or quick gate with .quality.toml thresholds
cogent check . --format json
```

### 2. Quick Risk Check

```bash
# Find high-risk functions (CRAP > 15)
cogent crap ./src --recursive --format json

# Find complex/churned files
cogent riskmap . --format json
```

### 3. Test Quality Check

```bash
# Requires: cargo test must pass first
cogent mutate . -p ast-parse-ts --max-mutants 5 --format json
```

### 4. Technical Debt

```bash
cogent debt ./src --recursive --format json
```

### 5. Security Spot-Check

```bash
# Run only security checks
cogent check . --only sast,secrets,crypto,taint,errhandle,vulnscan
```

### 6. License / Compliance Audit

```bash
cogent check . --only licenses,sbom
cogent sbom .   # standalone SBOM output
```

### 7. Watch Mode (live dev loop)

```bash
cogent watch .            # runs tests + coverage + checks on every change
cogent watch . --full     # all 21 checks every cycle
cogent watch . --no-tests # metrics-only, faster
```

### 8. Snapshot comparison

```bash
cogent check . --format json > before.json
# ... make changes ...
cogent check . --format json > after.json
cogent diff before.json after.json
```

## Tool Details

### crap (CRAP Score Calculator)
- **Purpose**: Find functions with high maintenance risk
- **Formula**: CRAP = comp² × (1 - coverage/100)³ + comp
- **Threshold**: > 15 is risky, > 30 is critical
- **Requires**: Test coverage data (optional)

### mutate (Mutation Testing)
- **Purpose**: Evaluate test suite quality
- **Precondition**: `cargo test` must pass
- **Output**: Mutation score (0-100%)
- **Notes**: Won't work on crates with failing tests

### riskmap (Risk Map)
- **Purpose**: Identify files that change often AND are complex
- **Data**: Cross-references git churn with code complexity
- **Use case**: Prioritize code reviews

### taint (Taint Analysis)
- **Purpose**: Detect sensitive data flow
- **Checks**: passwords, keys, PII to sinks
- **Use case**: Security audits

### sast (Static Application Security Testing)
- **Purpose**: Detect injection flaws and dangerous patterns
- **Checks**: SQL injection, XSS, path traversal, command injection, eval, SSRF (25 rules)
- **Severity**: Critical / High / Medium per finding

### secrets
- **Purpose**: Find hardcoded credentials and API keys
- **Output**: File:line location of each finding

### crypto
- **Purpose**: Detect weak cryptography
- **Checks**: MD5/SHA1, insecure random, ECB mode, hardcoded IVs, deprecated TLS

### licenses
- **Purpose**: OSS license compliance
- **Sources**: Cargo.lock, package.json, requirements.txt
- **Output**: GPL/AGPL/LGPL/MIT/Apache classification; deny-list violations

### sbom
- **Purpose**: Generate Software Bill of Materials
- **Formats**: CycloneDX 1.4 XML, SPDX 2.3 text
- **Sources**: Same lock files as `licenses`

## CI/CD Integration

### GitHub Actions (auto-generated by `cogent init --ci`)

```yaml
- name: Quality Audit
  run: |
    cogent run . --format sarif > results.sarif
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Pre-commit Hook

```bash
# Install full hook (runs tests + coverage + check)
cogent install-hooks

# Install fast hook (metrics only, no test run)
cogent install-hooks --fast
```

## MCP / Hermes Server Integration

The `cogent-server` crate exposes all tools as an MCP stdio server:

```bash
# Start MCP server (stdio transport)
cogent-server --mode stdio

# Start with TCP transport
cogent-server --mode tcp --port 9876
```

Hermes MCP tool provider config:
```json
{ "mcpServers": { "cogent": { "command": "cogent-server", "args": ["--mode", "stdio"] } } }
```

The server supports both legacy `tools/run` (JSON-RPC) and standard MCP `tools/call` — existing Hermes skills using `tools/run` continue to work.

## Output Formats

| Format | Use Case | Command |
|--------|----------|---------|
| `json` | Programmatic | `--format json` |
| `sarif` | GitHub Security | `--format sarif` |
| `ndjson` | Streaming | `--format ndjson` |
| `text` | Human readable | `--format text` |

## Pitfalls

1. **mutate fails**: Ensure `cargo test` passes first; use `-p crate-name` in workspaces
2. **Coverage required for accurate CRAP**: Run `cogent init` to auto-detect coverage command
3. **No `.quality.toml`**: Run `cogent init` — `check` will use generic defaults without it
4. **Binary not on PATH**: Run `cargo install --path crates/cogent-cli` and `cargo install --path crates/cogent-server`

## Verification

```bash
# Check all tools work
cogent run . --format json | jq '.summary'

# Check specific tool
cogent crap ./src --recursive --format json | head

# Verify MCP server responds
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}' | cogent-server --mode stdio
```

## Rules

1. Run `cogent check .` before every PR (exit 0 = good to merge)
2. Fix CRAP > 30 immediately before proceeding
3. Use `mutate` to verify test suites catch bugs
4. Zero tolerance for TODO/FIXME in production code
5. Address riskmap findings in code reviews

## See Also

- Repository: https://github.com/KidIkaros/cogent
- UTCP Manual: `docs/utcp/cogent.json`
- Claude Code: `CLAUDE.md` (repo root)
- OpenCode: `AGENTS.md` (repo root)
- MCP / UTCP integration: `docs/utcp-integration.md`