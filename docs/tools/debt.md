# Technical Debt Markers

## What it measures

Scans source code for known debt markers:

- `TODO` — planned work, often legitimate
- `FIXME` — known bug or broken behavior
- `HACK` — workaround that should be replaced
- `XXX` — danger or uncertainty marker

Each marker is a known unaddressed issue that accumulates over time. Projects with hundreds of markers often have no tracking system for them, leading to forgotten bugs and surprise regressions.

## Threshold meaning

| Ecosystem | `max_markers` | Meaning |
|-----------|---------------|---------|
| Rust | 0 | Zero tolerance; every marker must be tracked |
| Go | 0 | Zero tolerance |
| Python / JS | 0 | Zero tolerance |
| Unknown | 100 | Lenient fallback for legacy codebases |

## Example output (text)

```
  ✗ debt  0.4s  7 markers found (threshold: 0)
    src/cache.rs:44   TODO: replace with LRU cache when load > 1k rps
    src/db.rs:112     FIXME: race condition on concurrent writes
    src/auth.rs:8     HACK: bypassing auth for legacy endpoint
    src/main.rs:203   TODO: add metrics exporter
```

## How to fix

### 1. Convert TODOs into tracked issues

```bash
# Find all markers
cogent debt ./src --format json | jq '.markers[] | {file, line, text}'

# Create GitHub issues for each TODO
grep -rn "TODO" ./src | while read line; do
  gh issue create --title "Debt: ${line}" --body "Found in: ${line}"
done

# Remove markers from code after creating issues
```

### 2. Schedule a debt sprint

Dedicate one day per sprint to resolving FIXMEs. Unlike TODOs, FIXMEs represent active bugs:

```
Sprint goal: resolve all FIXMEs
- Prioritize by file churn (use cogent riskmap)
- Write regression tests before fixing
- Remove marker + close issue in the same commit
```

### 3. Prevent new markers with a pre-commit hook

```bash
cogent install-hooks .   # blocks commits with new TODO/FIXME
```

Or add a lighter check to CI:

```yaml
- name: Debt check
  run: cogent debt ./src --format json
```

## Common pitfalls

- **Allowing "temporary" TODOs to persist for years.** If a TODO is older than the last release, it is not temporary.
- **Using TODO for design notes.** Use `// NOTE:` or architecture decision records (ADRs) instead.
- **Ignoring FIXMEs.** FIXME means something is actively broken. Treat them as P1 bugs.

## Related

- `cogent explain riskmap` — find which files with debt markers change most often
- `cogent explain complexity` — complex files with many TODOs are doubly risky
- `cogent watch .` — catch new markers during development
