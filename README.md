# Cogent

<img src="logo.svg" alt="Cogent Logo" width="400"/>

[![CI](https://github.com/KidIkaros/cogent/actions/workflows/quality.yml/badge.svg)](https://github.com/KidIkaros/cogent/actions/workflows/quality.yml)
[![Coverage](https://img.shields.io/badge/coverage-report-green)](https://github.com/KidIkaros/cogent/actions/workflows/quality.yml)
[![Release](https://img.shields.io/github/v/release/KidIkaros/cogent)](https://github.com/KidIkaros/cogent/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange)](https://rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache--2.0%20%7C%20OPL--1.1-blue)](LICENSE)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue)](https://github.com/KidIkaros/cogent/pkgs/container/cogent)

**The unified security audit & compliance platform** — 31 audit tools across 5 categories that replace SonarQube, CodeQL, Snyk, and Slither with a single, zero-config CLI. Designed for CI/CD gatekeeping, compliance reporting, and autonomous AI agent integration.

📖 **[Website & Docs](https://kidikaros.github.io/cogent/)** · 🗺️ **[Roadmap](ROADMAP.md)** · 📋 **[Changelog](CHANGELOG.md)** · 🤖 **[Agent Integration](AGENTS.md)**

---

## Contents

- [Why Cogent](#why-cogent)
- [Cogent vs. Incumbents](#cogent-vs-incumbents)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [The 31-Tool Engine Suite](#the-31-tool-engine-suite)
- [Understanding Your Score](#understanding-your-score)
- [Fixing Failures](#fixing-failures)
- [CI/CD Integration](#cicd-integration)
- [AI Agents & Headless Audit](#ai-agents--headless-audit)
- [Output Formats](#output-formats)
- [Typical Workflows](#typical-workflows)
- [Documentation](#documentation)
- [Project Status](#project-status)
- [Contributing](#contributing)

---

## Why Cogent

Modern security auditing is fragmented. Each tool requires its own CI job, its own config file, and its own dashboard — the result is **pipeline sprawl**, inconsistent thresholds, and blind spots between quality and security.

| Incumbent | What it does | What it costs you |
|---|---|---|
| **SonarQube** | Monolithic quality suite | JVM bloat, opaque rules, per-line pricing |
| **CodeQL** | Deep semantic analysis | GitHub-only, steep query DSL, slow CI times |
| **Snyk** | Dependency & container scanning | Cloud-only for full features, seat-based pricing |
| **Slither** | Smart contract static analysis | Python-only, no CI-native output formats |
| **Semgrep** | Lightweight pattern matching | Rules are YAML regex, no coverage-aware scoring |

**Cogent replaces the entire stack with one CLI** that speaks JSON, NDJSON, and SARIF natively:

```
Quality        —  crap · debt · doccov · riskmap · dupfind · coupling · complexity · linelen · halstead · deadcode · cohesion · comments · propcov · typecov · fuzz · mutate
Security       —  secrets · taint · errhandle · vulnscan · sast · crypto · access-control
Compliance     —  licenses · sbom
Supply Chain   —  supply-chain · outdated
Operations     —  observability · test-quality · design-docs · debuggability
```

No JVM. No cloud token. No per-seat pricing. Install the binary, run `cogent check .`, and get a deterministic pass/fail gate with structured JSON, SARIF for GitHub Security, or an HTML audit report. Built in Rust with zero external runtime dependencies, designed from the start for **AI agent consumption** and **CI/CD integration**.

---

## Cogent vs. Incumbents

| Feature | Cogent | SonarQube | CodeQL | Snyk | Slither |
|---|---|---|---|---|---|
| **Installation** | Single binary, no JVM | JVM + DB + web server | GitHub Actions only | Cloud SaaS | Python + pip |
| **Languages** | Rust, Python, JS/TS, Go, Java, C/C++, PHP, Ruby, C#, Solidity | 25+ | 10+ | 7 | Solidity only |
| **Smart Contracts** | ✅ Built-in SAST rules | ❌ | ❌ | ❌ | ✅ Deep semantic |
| **CI Exit Codes** | 0=pass, 1=fail, 2=error | HTTP API polling | SARIF only | SARIF only | JSON only |
| **Local / Air-gapped** | ✅ Fully offline | ⚠️ Enterprise only | ❌ | ❌ | ✅ |
| **Pricing** | Free / Open Source | Per-line or seat | Free for public repos | Per-seat | Free |
| **Output Formats** | JSON, NDJSON, SARIF, HTML, Markdown | SARIF, SonarQube format | SARIF | SARIF, JSON | JSON, markdown |
| **Agent Integration** | ✅ Hermes skills + MCP server | ❌ | ❌ | ❌ | ❌ |
| **Coverage-aware** | ✅ CRAP, mutation testing | Partial | ❌ | ❌ | ❌ |

---

## Installation

### macOS (Homebrew)
```bash
brew tap kidikaros/cogent
brew install cogent
```

### Linux (Binary)
```bash
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
```

### From Source (Cargo)
```bash
git clone https://github.com/KidIkaros/cogent.git
cd cogent
cargo build --release --workspace
export PATH="$PWD/target/release:$PATH"
```

Optionally install the binaries directly:
```bash
cargo install --path crates/cogent-cli
cargo install --path crates/cogent-server   # optional MCP server
```

### Shell Completions
```bash
cogent completions bash > /etc/bash_completion.d/cogent
cogent completions zsh  > /usr/local/share/zsh/site-functions/_cogent
cogent completions fish > ~/.config/fish/completions/cogent.fish
```

---

## Quick Start

```bash
# 1. Auto-detect your ecosystem and write .quality.toml
cogent init

# 2. Run all 31 checks (auto-loads thresholds from .quality.toml)
cogent check .

# 3. Generate a visual HTML audit report
cogent report . --open

# 4. Wire up GitHub Actions + a pre-commit hook (one-time)
cogent init --ci
```

Other handy invocations:

```bash
cogent check . --force            # ignore .quality.toml, use defaults
cogent check . --format json --ci # CI-friendly JSON, no color/progress, exits 1 on failure
cogent check . --format sarif     # SARIF for the GitHub Security tab
cogent check . --no-cache         # bypass the cache for a fresh gate
cogent <tool> ./src               # run a single tool, e.g. `cogent crap ./src`
```

**Sample output:**

```
  ✓ detected: Rust  (Cargo.toml found)  test: cargo test
  ✓ wrote .quality.toml  (0.8ms)

  ▶ Key thresholds chosen:
    · max_crap    = 15.0
    · min_doc     = 95%
    · max_debt    = 0
    · max_complexity_violations = 0

  ... (checks run with live progress) ...

  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  PASSED ✓                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  31/31 checks passed  ·  5.1s total                  ║
  ║  Score: 100/100  A                                   ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝
```

If anything fails, Cogent prints a **Quick Fixes** box and groups failures by category (Security 🔴, Quality 🟡, Compliance 🔵) so you know what to tackle first.

---

## The 31-Tool Engine Suite

Invoke any tool individually (`cogent crap src/`) or run the full battery (`cogent check .`).

**Quality (16)** — crap · debt · doccov · riskmap · dupfind · coupling · complexity · linelen · halstead · deadcode · cohesion · comments · propcov · typecov · fuzz · mutate

**Security (7)** — secrets · taint · errhandle · vulnscan · sast · crypto · access-control

**Compliance (2)** — licenses · sbom

**Supply Chain (2)** — supply-chain · outdated

**Operations (4)** — observability · test-quality · design-docs · debuggability

| Engine | Promise | Output |
|---|---|---|
| **crap** | CRAP score per function (complexity × coverage risk) | Function-level rankings |
| **mutate** | Mutation testing — test suite kill rate | Score % + surviving mutants |
| **debt** | Technical debt inventory (TODO/FIXME/HACK/XXX) | Author-grouped heatmap |
| **riskmap** | Churn × complexity hot spot identification | Ranked file list |
| **doccov** | Public API documentation coverage | Module-level % |
| **taint** | Sensitive data flow tracing (secrets, logs) | Paths with source→sink |
| **secrets** | Hardcoded credential detection | File + line findings |
| **sast** | SAST: SQLi, XSS, path traversal, command injection | Severity-ranked findings |
| **crypto** | Weak crypto: MD5/SHA1, ECB, hardcoded IVs, insecure random | Rule violations |
| **vulnscan** | Known CVE audit via cargo-audit / pip-audit | CVE list with CVSS |
| **errhandle** | Unhandled error / swallowed exception patterns | Violation count |
| **licenses** | OSS license compliance (GPL/AGPL deny-list) | Package classifications |
| **sbom** | SBOM generation (CycloneDX 1.4 / SPDX 2.3) | XML or text manifest |
| **fuzz** | Fuzzable entry point detection | Fuzzability scores |
| **coupling** | Dependency analysis (cycles, fan-in/out) | Coupling matrix |
| **dupfind** | AST-based duplication detection | Duplicate blocks |
| **propcov** | Property test coverage | Coverage % |
| **deadcode** | Unused symbols and unreachable branches | Finding count |
| **linelen** | Long-function / long-file violations | Violation count |
| **complexity** | Cyclomatic complexity violations | Function list |
| **halstead** | Halstead complexity metrics (bugs estimated) | Per-file estimates |
| **cohesion** | LCOM4 cohesion analysis | Module cohesion scores |
| **comments** | Comment-to-code ratio analysis | Per-file ratios |
| **typecov** | Type annotation coverage (Python/JS/TS) | Coverage % |
| **outdated** | Direct deps ≥1 major version behind latest | Stale package list |
| **access-control** | Missing auth guards, hardcoded creds, IAM policies, CORS | Finding count + remediation |
| **supply-chain** | Dependency integrity, typosquatting, unpinned deps, lockfile checks | Package risk list |
| **observability** | Structured logging coverage (tracing/logging detection) | Violation count |
| **test-quality** | Non-determinism in tests (time/random/order-dependent) | Score % |
| **design-docs** | Design documentation pillar check (CHANGELOG, README, architecture) | Pillar count |
| **debuggability** | Contextless unwrap/panic detection | Violation count |

---

## Understanding Your Score

Cogent computes a **weighted health score (0–100)** and a letter grade:

| Score | Grade | Meaning |
|-------|-------|---------|
| 90–100 | **A** | Excellent — all critical checks pass |
| 80–89  | **B** | Good — minor quality issues |
| 70–79  | **C** | Fair — several checks failing |
| 60–69  | **D** | Poor — major issues need attention |
| < 60   | **F** | Critical — security or compliance failures |

**Security checks are weighted 3×** (secrets, vulnscan, sast, crypto, taint, errhandle)
**Compliance checks are weighted 2×** (licenses, sbom)
**Quality checks are weighted 1×** (everything else)

This weighting ensures a single exposed secret or critical CVE fails the gate even if every quality check passes.

---

## Fixing Failures

When a check fails, you have three ways to learn what to do:

1. **`cogent explain <tool>`** — instant terminal documentation. Run `cogent explain crap`, `cogent explain debt`, etc. Each prints what the tool measures, how to read the output, and 3 concrete fixes.
2. **`cogent <tool> ./src --format json`** — the full list of offenders with file paths and line numbers.
3. **`docs/tools/<tool>.md`** — deep-dive guides with before/after code examples for the most common checks.

---

## CI/CD Integration

### GitHub Actions
```yaml
- name: Cogent Security Audit
  run: |
    cogent check . --format json --ci
    cogent check . --format sarif --output cogent.sarif
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: cogent.sarif
```

### GitLab CI
```yaml
cogent-audit:
  script:
    - cogent check . --format json --ci
  artifacts:
    reports:
      junit: cogent-junit.xml
```

### Pre-commit Hook
```bash
# .git/hooks/pre-commit
cogent check . --format json || exit 1
```

---

## AI Agents & Headless Audit

Cogent is designed for autonomous agent integration with structured output and deterministic exit codes. It ships with **Hermes Agent skill definitions** under `hermes/` and an MCP server (`cogent-server`) compatible with Claude Desktop, Cursor, and Windsurf.

```bash
# NDJSON stream for agent consumption
cogent audit . --format ndjson

# Auto-close resolved findings
cogent audit . --verify

# Compliance workflow
cogent policy .           # view policy
cogent exception add ...  # request a deviation
cogent remediate .        # auto-fix where possible
```

**Agent consumption pattern** — stream critical findings for triage:

```bash
cogent audit . --format ndjson | \
  jq -c 'select(.severity=="critical")' | \
  while read finding; do
    echo "$finding" | jq -r '.file + ":" + (.line|tostring)'
  done
```

See [`AGENTS.md`](AGENTS.md) for the complete skill catalog and integration patterns.

---

## Output Formats

| Format | Use Case |
|---|---|
| **JSON** | Structured parsing, agent workflows |
| **NDJSON** | Streaming pipelines, log aggregation |
| **SARIF** | GitHub Security tab, static analysis tooling ecosystem |
| **HTML** | Visual audit report with health score, drill-downs, remediation checklist |
| **Markdown** | Readable report for PRs and wikis |
| **PDF** | Printable report via headless Chrome/Chromium |
| **Human** | Terminal review (default) |

All tools accept `--format <json|ndjson|sarif|text>`.
Reports: `cogent report . --format <html|markdown|pdf> [--open]`.

---

## Typical Workflows

- **Pre-commit quality gate** — fail PRs if `crap` threshold exceeded or mutation score drops
- **Refactoring prioritization** — `riskmap` pinpoints files with highest change-complexity risk
- **Security audit** — `--only sast,secrets,crypto,taint,vulnscan` runs the security subset in seconds
- **Smart contract audit** — `cogent sast ./contracts --recursive` scans Solidity for reentrancy, access control, and timestamp issues
- **Access control audit** — `cogent access-control ./src --recursive` finds missing auth guards, hardcoded credentials, and overly permissive IAM policies
- **Supply chain audit** — `cogent supply-chain .` checks lockfile integrity, typosquatting, and unpinned dependencies
- **Documentation audit** — `doccov` surfaces undocumented public APIs before release
- **Test strength assessment** — `mutate` measures defect detection capability beyond coverage numbers
- **Snapshot comparison** — `cogent diff before.json after.json` shows regressions and fixes between any two check runs

---

## Documentation

- [Website](https://kidikaros.github.io/cogent/) — project landing page and overview
- [User Guide](./docs/user-guide.md) — CLI reference and output interpretation
- [Developer Guide](./docs/developer-guide.md) — crate architecture and extending the suite
- [Metrics Explained](./docs/metrics-explained.md) — how each score is computed
- [Hermes Integration](./docs/utcp-integration.md) — wiring Cogent into AI agent workflows
- [Tool Guides](./docs/tools/) — per-tool deep dives with before/after examples
- [Schema Reference](./schemas/) — JSON/NDJSON/SARIF output contracts
- [Roadmap](./ROADMAP.md) · [Changelog](./CHANGELOG.md) · [Project Status](./PROJECT_STATUS.md)

---

## Project Status

| Status | Detail |
|---|---|
| Current Release | Stable **v1.2.0** |
| CI Pipeline | Lint → build → test → audit across Linux/macOS/Windows |
| Self-Hosting | Runs `cogent check .` on its own codebase every commit |
| Schema Validation | JSON schemas published in `schemas/` |
| Test Suite | `cargo test --workspace` — workspace-wide passing |

See [`ROADMAP.md`](ROADMAP.md) for what's next and [`PROJECT_STATUS.md`](PROJECT_STATUS.md) for the detailed health snapshot and known limitations.

---

## Contributing

Cogent is open source under **Apache-2.0 / OPL-1.1** dual licensing. Contributions are welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) and the [Developer Guide](./docs/developer-guide.md) for development setup and guidelines.
