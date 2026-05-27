---
name: rust-cogent-analysis
description: Rust code quality metrics using Cogent CLI — CRAP, doc coverage, debt markers, cyclomatic complexity.
version: 1.0.0
author: Hermes Agent
license: OPL-1.1
---

# Rust Quality Analysis

## Tools

### Cogent CLI
```bash
cogent <command>
```

**Commands:**
- `check` — All metrics (CRAP, debt, doc coverage, complexity)
- `crap` — CRAP metric only
- `doccov` — Documentation coverage
- `debt` — Technical debt markers
- `complexity` — Cyclomatic complexity report
- `dupfind` — Code duplication

### Quality Analysis Workflow
```bash
# 1. Full quality check (all metrics)
cogent check /path/to/project

# Example output:
# {
#   "passed": true,
#   "checks": [
#     {"name": "crap", "score": 0.0, "threshold": 30.0},
#     {"name": "doc_coverage", "score": 100.0, "threshold": 50.0},
#     ...
#   ]
# }

# 2. CRAP only (requires lcov)
CARGO_TARGET_DIR=/tmp/build cargo llvm-cov --lcov -o coverage.info
cogent crap /path/to/project

# 3. Doc coverage only
cogent doccov /path/to/project

# 4. Technical debt markers
cogent debt /path/to/project
```

## Metrics Explained

### CRAP (Change Risk Anti-Patterns)
Estimates maintenance risk by combining cyclomatic complexity with test coverage:

**Formula:** `CRAP = comp² × (1 - coverage/100)³ + comp`

| Metric | Score Range | Meaning |
|--------|-------------|----------|
| CRAP | ≤ 30 | Excellent — low maintenance risk |
| CRAP | 31-60 | Acceptable |
| CRAP | > 60 | Crappy — refactor needed |

### Documentation Coverage
Percentage of public functions with doc comments:
```bash
cogent doccov /path/to/project
# Output: Doc coverage 100% >= 50%
```

## Typical Results

| Project | CRAP | Doc Cov | Debt | Complexity |
|---------|------|---------|------|------------|
| rl-stack | 0.0 | 100% | 0 | 0 |
| origin-cli | 25.0 | 85% | 3 | 42 |

## When to Use

- After completing major refactoring — validate improvement
- Before releasing a crate — ensure quality standards met
- During code review — check metrics alongside manual review
- Quarterly audits — track maintenance risk trends