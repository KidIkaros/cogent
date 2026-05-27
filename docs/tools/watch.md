# watch

## Purpose

Continuous monitoring mode. Runs checks on file changes, providing instant feedback during development.

## What it does

- Watches filesystem for changes to tracked files
- Runs fast subset of checks on each change (debt, doc, crap by default)
- Shows cycle diff (↑ now passing, ↓ now failing)
- Supports `--full` mode for complete analysis
- Ideal for pre-commit quality feedback

## Flags/options

| Flag | Description |
|------|-------------|
| `<path>` | Directory to watch (default: current directory) |
| `--full` | Run all 26 checks every cycle (slower, comprehensive) |
| `--format <format>` | Output: `text`, `json` |
| `--debounce <ms>` | Delay before running (default: 500ms) |
| `--clear` | Clear terminal between runs |
| `--no-color` | Disable colored output |

## Output format

### Default mode (fast)

```
Watching: ./src
Press Ctrl+C to stop

[Change detected: src/engine.rs]

  ✓ debt   0.2s  (unchanged)
  ✓ doccov 0.8s  (unchanged)
  ✓ crap   1.2s  ↑ now passing

  ╔══════════════════════════════════════════════════════╗
  ║  CYCLE 12  ·  3.2s  ·  Score: 100/100  A             ║
  ╚══════════════════════════════════════════════════════╝

Cycle diff:
  ↑ crap now passing (was failing)
```

### Full mode (--full)

```
[Change detected: src/db.rs]

Running full analysis (26 checks)...

  ✓ debt      0.2s
  ✓ doccov    0.8s
  ✓ crap      1.2s
  ✓ secrets   0.3s
  ✓ sast      1.5s
  ...

  ╔══════════════════════════════════════════════════════╗
  ║  FULL CYCLE  ·  8.5s  ·  Score: 92/100  A            ║
  ╚══════════════════════════════════════════════════════╝
```

## Cycle diff notation

| Symbol | Meaning |
|--------|---------|
| `↑` | Check now passing (was failing) |
| `↓` | Check now failing (was passing) |
| `=` | Unchanged status |
| `✓` | Still passing |
| `✗` | Still failing |

## Exit codes

Watch mode runs indefinitely until interrupted (Ctrl+C). Exit code reflects last cycle:

| Code | Meaning |
|------|---------|
| `0` | Last cycle passed (clean exit) |
| `1` | Last cycle failed |
| `130` | Interrupted (Ctrl+C) |

## Examples

```bash
# Basic watch mode (fast feedback)
cogent watch .

# Full analysis on every change
cogent watch . --full

# JSON output for editor integration
cogent watch . --format json

# Clear terminal between runs
cogent watch . --clear

# Custom debounce for rapid typists
cogent watch . --debounce 1000
```

## Editor integration

Watch mode can drive editor plugins:

```bash
# VS Code task integration
cogent watch . --format json | \
  jq -c 'select(.status=="failed")' | \
  while read issue; do
    # Send to editor problem panel
    echo "$issue"
  done
```

## Performance

| Mode | Checks | Typical duration | Use case |
|------|--------|-----------------|----------|
| Default | 3 (debt, doccov, crap) | 2–4s | Live feedback |
| `--full` | 26 | 6–12s | Pre-commit validation |

## See also

- `cogent check` — one-time full analysis
- `cogent report` — generate historical reports
- Editor plugin documentation
