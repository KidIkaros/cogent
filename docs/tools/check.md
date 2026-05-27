# check

## Purpose

Run all 26 Cogent analyzers with an interactive progress display. The primary user-facing command for quality gates.

## What it does

- Runs the complete analyzer suite (quality, security, compliance, supply chain)
- Shows real-time progress with checkmarks and spinners
- Computes weighted health score (0–100) and letter grade (A–F)
- Provides inline fix suggestions for failed checks
- Exits with appropriate code for CI integration

## Flags/options

| Flag | Description |
|------|-------------|
| `<path>` | Directory to check (default: current directory) |
| `--format <format>` | Output: `text`, `json`, `ndjson`, `sarif`, `html` |
| `--output <file>` | Write report to file |
| `--only <checks>` | Run subset: `cogent check . --only secrets,sast` |
| `--ci` | CI mode: JSON output, no colors/progress |
| `--force` | Ignore missing `.quality.toml` |
| `--verbose` | Show file:line offenders inline |
| `--recursive` | Scan subdirectories |

## Output format

### Interactive (default)

```
  ✓ debt    0.4s  0 markers found
  ✓ doccov  1.2s  96% coverage (threshold: 95%)
  ✗ crap    2.1s  Average CRAP 24.5 exceeds threshold 15.0
    src/engine.rs:42  calculate_score  CRAP 45.2
    src/parser.rs:12  parse_token      CRAP 31.4
  ✓ secrets 0.3s  No secrets found
  ✗ sast    1.5s  2 security issues
    src/db.rs:45  SQL injection possible
    src/auth.rs:12  Weak comparison
  ...

  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  FAILED ✗                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  22/26 checks passed  ·  6.8s total                  ║
  ║  Score: 85/100  B                                    ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝

  Quick fixes:
    • Run tests to lower CRAP scores
    • Review sast findings in src/db.rs
```

### CI mode (--ci)

```json
{
  "summary": {
    "total": 26,
    "passed": 22,
    "failed": 4,
    "score": 85,
    "grade": "B",
    "duration_ms": 6800
  },
  "checks": [...]
}
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | One or more checks failed |
| `2` | Error (invalid path, config error) |

## Weighted scoring

Security checks are weighted 3×, compliance 2×, quality 1×:

| Category | Checks | Weight |
|----------|--------|--------|
| Security | secrets, sast, crypto, taint, vulnscan | 3× |
| Compliance | licenses, sbom | 2× |
| Quality | crap, debt, doccov, ... | 1× |

A single security failure can drop an A to a B or C.

## Examples

```bash
# Basic check with interactive output
cogent check .

# CI-friendly JSON
cogent check . --format json --ci

# Security-only gate
cogent check . --only secrets,sast,crypto,taint,vulnscan --ci

# Verbose with inline offenders
cogent check . --verbose

# HTML report
cogent check . --format html -o report.html

# SARIF for GitHub Security tab
cogent check . --format sarif -o cogent.sarif
```

## CI/CD integration

### GitHub Actions

```yaml
- name: Cogent Quality Gate
  run: cogent check . --format json --ci
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: cogent.sarif
```

### Pre-commit hook

```bash
#!/bin/sh
cogent check . --format json || exit 1
```

## See also

- `cogent audit` — headless version for agents
- `cogent watch` — continuous monitoring mode
- `cogent report` — generate visual/HTML reports
