---
name: cogent-analyzer
description: Multi-language code quality orchestration
version: 1.0.0
maintainer: mo
category: devops
tags: ["code-cogent", "rust", "python", "javascript", "static-analysis", "technical-debt"]
dependencies:
  - Cogent (Rust workspace binary at target/release/cogent)
  - matplotlib
  - jinja2
  - rich
compatibility:
  hermes: ">=0.6.0"
  python: ">=3.9"
---

## Overview

Comprehensive code quality skill wrapping the **Cogent** Rust workspace (8.4K LOC, 15 crates). Exposes 12 specialized tools with unified Rich terminal output and HTML dashboard reports.

## Architecture

```
Hermes → cogent-analyzer (Python subprocess bridge) → cogent (Rust binary)
                              └─→ DashboardGen (matplotlib + Jinja2)
```

## Exposed Tools

cogent_check(path, recursive, coverage, max_crap, min_doc, max_debt, skip)
  Run all cogent checks bundled (cogent run CLI)

cogent_crap(path, recursive, coverage)
  CRAP metric — change-risk anti-patterns (Rust-only, cargo required)

cogent_debt(path, recursive, marker)
  Technical debt scan — TODO/FIXME/HACK markers (tree-sitter)

cogent_docs(path, recursive)
  Documentation coverage — public API docs % (tree-sitter)

cogent_complexity(path, recursive, min_complexity)
  Cyclomatic complexity — functions exceeding threshold (tree-sitter)

cogent_duplication(path, recursive, min_lines)
  Code duplication — copy-pasted blocks (tree-sitter)

cogent_coupling(path, min_coupling)
  Module coupling — fan-in/fan-out dependency graphs (tree-sitter)

cogent_risk(path, since, min_risk)
  Risk map — git churn × complexity hotspot scoring (tree-sitter)

cogent_taint(path, recursive, attribute, severity)
  Taint analysis — sensitive dataflow to sinks (tree-sitter)

cogent_mutation(path, files, max_mutants, timeout)
  Mutation testing — test suite cogent via intentional bugs (Rust-only)

cogent_fuzz(path, recursive, min_score, top)
  Fuzz surface analyzer — functions ideal for fuzzing (Rust-only)

cogent_propcov(path, recursive, only_tests, min_coverage)
  Property-based test coverage — proptest/quickcheck macro scan (tree-sitter)

cogent_languages()
  Returns language support matrix (Rust-only vs tree-sitter parity)

cogent_dashboard(report_json, output_path)
  Generate HTML dashboard from JSON results (matplotlib + Jinja2)
```

## Configuration

```toml
[cogent-analyzer]
# Binary location — auto-detected
cogent_binary = "target/release/cogent"
parallel_jobs = 4                    # Parallel file parsing
timeout_seconds = 120               # Per-tool timeout
dashboard_template = "templates/cogent_dashboard.html"
```

## Output

- **Terminal:** Rich live stages with spinners and color-coded status
- **JSON:** Structured ToolResult objects for each module
- **Dashboard:** `quality_report.html` with radar, bar, and hotspot charts

## Usage Example

```python
from hermes_skills.devops import cogent_analyzer

# Run bundled check
result = cogent_analyzer.cogent_check(
  path="$HOME/hermes-agent",
  recursive=True,
  coverage="/path/to/lcov.info",
  max_crap=30.0,
  min_doc=70.0,
  max_debt=100
)

# Generate HTML dashboard
html = cogent_analyzer.cogent_dashboard(result, "/tmp/report.html")
```

## Differential

vs `persona-spec`: 4 tools only — ~20% surface
vs `cogent-analyzer`: 12 tools — full feature matrix, language parity honest, visual dashboard

## Status

Phase 1 complete — skill launched with full toolset. Dashboard polish in progress.
