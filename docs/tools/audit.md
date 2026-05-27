# audit

## Purpose

Run a comprehensive security and compliance audit on a codebase. The `audit` command is designed for headless/agent consumption, producing structured output that can be consumed programmatically.

## What it does

- Executes all 26 analyzers (quality, security, compliance, supply chain)
- Produces machine-readable output (JSON/NDJSON)
- Supports verification mode to auto-close resolved findings
- Optimized for CI/CD pipelines and AI agent workflows

## Flags/options

| Flag | Description |
|------|-------------|
| `<path>` | Directory to audit (default: current directory) |
| `--format <format>` | Output format: `json`, `ndjson`, `sarif`, `text` (default: text) |
| `--verify` | Re-run and auto-close findings that are now resolved |
| `--recursive` | Scan subdirectories recursively |
| `--only <checks>` | Run only specified checks (comma-separated) |
| `--force` | Run without `.quality.toml` (uses default thresholds) |
| `-o, --output <file>` | Write output to file instead of stdout |

## Output format

### NDJSON (agent consumption)

Each line is a standalone JSON object, suitable for streaming:

```ndjson
{"tool":"secrets","file":"src/config.rs","line":23,"finding":"hardcoded_api_key","severity":"high"}
{"tool":"sast","file":"src/db.rs","line":45,"finding":"sql_injection","severity":"critical"}
```

### JSON (structured report)

```json
{
  "summary": {
    "total_checks": 26,
    "passed": 22,
    "failed": 4,
    "score": 85,
    "grade": "B"
  },
  "findings": [
    {
      "tool": "secrets",
      "category": "security",
      "file": "src/config.rs",
      "line": 23,
      "message": "Hardcoded API key detected"
    }
  ]
}
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | One or more checks failed |
| `2` | Error (invalid path, tool failure) |

## Examples

```bash
# Basic audit with human-readable output
cogent audit .

# JSON output for CI integration
cogent audit . --format json --ci

# NDJSON stream for agent consumption
cogent audit . --format ndjson

# Verify mode — auto-close resolved findings
cogent audit . --verify

# Security-only audit
cogent audit . --only secrets,sast,crypto,taint,vulnscan

# Output to file
cogent audit . --format json -o audit-report.json
```

## Agent consumption pattern

```bash
# Stream findings to an agent for triage
cogent audit . --format ndjson | \
  jq -c 'select(.severity=="critical")' | \
  while read finding; do
    # Process critical findings
    echo "$finding" | jq -r '.file + ":" + (.line|tostring)'
  done
```

## See also

- `cogent check` — interactive check with progress display
- `cogent policy` — view and manage compliance policies
- `cogent remediate` — auto-fix supported findings
- [`AGENTS.md`](../../AGENTS.md) — full agent integration guide
