# Migrating from SonarQube to Cogent

This guide helps you migrate from SonarQube to Cogent, showing how SonarQube rules map to Cogent tools and how to set comparable thresholds.

---

## Why Migrate?

| Feature | SonarQube | Cogent |
|---------|-----------|--------|
| **Installation** | JVM + database + web server | Single binary |
| **Startup time** | 10-30s | < 100ms |
| **CI integration** | HTTP API polling | CLI with exit codes |
| **Pricing** | Per-line or per-seat | Free & open source |
| **Output formats** | SonarQube format | JSON, NDJSON, SARIF, HTML |
| **Agent integration** | ❌ | ✅ Hermes skills + MCP |
| **Local/offline** | Enterprise only | ✅ Fully offline |

---

## Rule Mapping

### Code Smells

| SonarQube Rule | Cogent Tool | Threshold |
|----------------|-------------|-----------|
| `Cognitive Complexity` | `complexity` | `max_complexity = 15` |
| `Cyclomatic Complexity` | `complexity` | `max_complexity = 15` |
| `Function too long` | `linelen` | `max_function_lines = 50` |
| `File too long` | `linelen` | `max_file_lines = 500` |
| `Comment ratio` | `comments` | `min_ratio = 10` |
| `Public API documented` | `doccov` | `min_doc = 95` |
| `Unused parameters` | `deadcode` | `max_unused = 0` |
| `Duplicated blocks` | `dupfind` | `max_dup_tokens = 50` |
| `Commented-out code` | `comments` | `max_commented_lines = 10` |

### Bugs

| SonarQube Rule | Cogent Tool | Threshold |
|----------------|-------------|-----------|
| `Unused function` | `deadcode` | `max_unused = 0` |
| `Empty block` | `deadcode` | `max_empty_blocks = 0` |
| `Unreachable code` | `deadcode` | `max_unreachable = 0` |
| `Assertion failure` | `test-quality` | `max_flaky_score = 10` |

### Vulnerabilities

| SonarQube Rule | Cogent Tool | Threshold |
|----------------|-------------|-----------|
| `SQL Injection` | `sast` | `max_severity = "medium"` |
| `Hardcoded credentials` | `secrets` | `max_secrets = 0` |
| `Weak cryptography` | `crypto` | `max_weak = 0` |
| `Command injection` | `sast` | `max_severity = "medium"` |
| `Path traversal` | `sast` | `max_severity = "medium"` |
| `Open redirect` | `sast` | `max_severity = "medium"` |

### Security Hotspots

| SonarQube Rule | Cogent Tool | Threshold |
|----------------|-------------|-----------|
| `Making copies of array/clone` | `complexity` | Use `linelen` for long functions |
| `SQL string concatenation` | `sast` | `max_severity = "medium"` |
| `Logging sensitive data` | `taint` | `max_taint_paths = 0` |

---

## Threshold Translation

### SonarQube Quality Gate to Cogent Thresholds

SonarQube Quality Gate:

```
Coverage on New Code: > 80%
Quality Gate Status: OK
  Bug: 0
  Vulnerability: 0
  Code Smell: < 10
```

Equivalent `.quality.toml`:

```toml
[cogent]
score_min = 90  # Corresponds to "OK" gate status

# Vulnerability mapping
[secrets]
max = 0  # "Vulnerability: 0"

[sast]
max_severity = "medium"  # Catches SQLi, command injection

[crypto]
max_weak = 0  # No weak crypto

[taint]
max_taint_paths = 0  # No sensitive data leaks

# Bug mapping
[deadcode]
max_unused = 0  # No unused code (prevents bugs)

[test-quality]
max_flaky_score = 10  # < 10% flaky tests

# Code smell mapping
[complexity]
max = 10  # Stricter than SonarQube default

[debt]
max = 5  # "Code Smell: < 10"

[doccov]
min = 80  # "Coverage" analog for docs

[mutate]
min_score = 80  # Test quality analog
```

---

## Migration Steps

### Step 1: Install Cogent

```bash
brew install cogent  # macOS
# Or download binary from releases
```

### Step 2: Initialize Cogent

```bash
cd your-project
cogent init
```

This creates `.quality.toml` with defaults tuned for your language.

### Step 3: Run a Parallel Audit

Run both tools and compare results:

```bash
# Run SonarQube (existing)
mvn sonar:sonar  # or your existing SonarQube command

# Run Cogent
cogent check . --format json > cogent-report.json
```

### Step 4: Align Thresholds

If Cogent fails where SonarQube passes (or vice versa), adjust `.quality.toml`:

```toml
# If Cogent is too strict
[complexity]
max = 20  # Relax from 15

# If Cogent is too lenient
[debt]
max = 0  # Tighten from 5
```

### Step 5: Replace SonarQube in CI

Replace your SonarQube CI step:

```yaml
# BEFORE (SonarQube)
- name: SonarQube Scan
  run: mvn sonar:sonar

# AFTER (Cogent)
- name: Cogent Audit
  run: |
    cogent check . --format json --ci
    cogent check . --format sarif --output cogent.sarif
- name: Upload SARIF to GitHub Security
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: cogent.sarif
```

### Step 6: Remove SonarQube

```bash
# Uninstall SonarQube scanner
brew uninstall sonar-scanner  # or your package manager

# Remove SonarQube configuration
rm sonar-project.properties
```

---

## Advanced: Custom Rules

SonarQube has custom rules via plugins. Cogent supports custom checks via:

1. **Python scripts**: Write a script that outputs JSON in Cogent format
2. **Rust plugins**: Extend `cogent-engine` with custom analyzers

See [Developer Guide](./developer-guide.md) for details.

---

## Troubleshooting

### Q: Cogent finds more issues than SonarQube

**A:** This is expected. Cogent has 31 tools vs SonarQube's ~10 core rules. Review and adjust thresholds.

### Q: Cogent misses issues SonarQube finds

**A:** Check if you're using SonarQube plugins (e.g., Python rules). Cogent supports the same languages; ensure you're using the latest version.

### Q: How do I migrate SonarQube custom rules?

**A:** For Python, see `docs/tools/custom-checks.md`. For other languages, open a GitHub issue with your rule spec.

---

## Example: Full Migration

**Before (SonarQube):**

```yaml
name: SonarQube
on: [push]
jobs:
  sonarqube:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: SonarQube Scan
        run: mvn sonar:sonar
```

**After (Cogent):**

```yaml
name: Cogent Audit
on: [push, pull_request]
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Cogent
        run: curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz && tar xzf cogent-linux-x86_64.tar.gz && sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
      - name: Run Audit
        run: |
          cogent check . --format json --ci
          cogent check . --format sarif --output cogent.sarif
      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: cogent.sarif
```

Result: CI goes from 2-3 minutes (SonarQube) to 30 seconds (Cogent), with richer output and zero external dependencies.

---

## Get Help

- **Documentation:** https://kidikaros.github.io/cogent/
- **GitHub Issues:** https://github.com/KidIkaros/cogent/issues
- **Migration Support:** Open an issue with your SonarQube config

---

**You're now running Cogent instead of SonarQube!** 🚀