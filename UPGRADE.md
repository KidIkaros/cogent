# Upgrade Guide

Migrating from `quality-tools` (pre-rebrand) to `cogent` v1.0.0.

---

## TL;DR

| Before (quality-tools) | After (cogent) |
|---|---|
| `crap ./src` | `cogent crap ./src` |
| `quality run .` | `cogent run .` |
| `.quality-baseline.sarif` | `.cogent-baseline.sarif` |
| `~/.quality-history/` | `~/.cogent-history/` |
| Separate binaries per tool | Unified `cogent` CLI with subcommands |

**Action steps:**
1. Update your CI scripts and docstrings to use `cogent` instead of `quality`
2. Rename `.quality-baseline.sarif` → `.cogent-baseline.sarif` if present
3. Update any scripts that parse old output filenames or paths
4. (Optional) Remove old `quality-tools` installation (`cargo uninstall quality-tools`)
5. Install Cogent from source or wait for crates.io release

---

## Detailed Changes

### 1. CLI Unification

**Before:** Each tool was its own binary:
```bash
crap ./src
mutate .
debt .
taint .
# …etc
```

**After:** Single `cogent` binary with subcommands:
```bash
cogent crap ./src
cogent mutate .
cogent debt .
cogent taint .
# Or run all tools:
cogent run .
```

**Why:** Simplifies PATH management, makes AI agent integration cleaner, and enables compound workflows (subcommand → arguments → output format flags in one place).

---

### 2. File Renames

Paths created or written to by Cogent have changed. Update any scripts that read/write these files:

| Old Path | New Path |
|---|---|
| `.quality-baseline.sarif` | `.cogent-baseline.sarif` |
| `.quality-history/` (dir) | `.cogent-history/` |
| `quality-<tool>.report.json` (legacy) | No longer auto-named — use `--output` flag |

**Action:** If you reference these in CI or docs, update them. You can keep both files temporarily during transition, but new runs will write only the new names.

---

### 3. Output Format Differences

**JSON field names** are mostly stable, but some tool outputs have minor schema improvements:
- `crap`: CRAP score now explicitly typed as float (previously mixed int/float)
- `mutate`: mutant list includes `location` object with `start`/`end` line numbers
- `taint`: `source` and `sink` fields now uniformly structured across languages

**Action:** If you parse JSON output in scripts, validate against schemas in `schemas/`. Most field names remain unchanged.

---

### 4. Error Exit Codes

Exit code policy is unchanged (non-zero on failure), but failure conditions are now consistent across all subcommands:
- `1` — tool execution error or panic
- `2` — quality threshold failure (e.g., CRAP > configured cap)
- `3` — invalid arguments or project configuration

**Action:** Review any CI checks that rely on specific exit codes; they will still work, but check that your thresholds match the new tool defaults (some thresholds were tightened).

---

### 5. Configuration Files

No YAML config is required. If you were using any hidden config files from dev builds of `quality-tools`, they are no longer read. All behavior is now explicit via CLI flags.

**Action:** Migrate flags into your command lines:
```bash
# Before (if you used it):
quality run . --threshold-crap 30

# After:
cogent run . --threshold-crap 30
```

---

### 6. Unchanged / Compatible

| Aspect | Status |
|--------|--------|
| Tool engine algorithms | Unchanged — same analysis logic |
| Language support | Identical (15+ languages) |
| SARIF schema | Same schema, different baseline filename |
| JSON schemas | Fully compatible (minor additions only) |
| Performance | Comparable or slightly faster (CLI unification overhead negligible) |

---

## Breaking Changes Summary

If you are upgrading from any `quality-tools` version, watch for:

1. **CLI command rename** — all invocations must switch to `cogent <subcommand>`
2. **Baseline filename** — `.quality-baseline.sarif` → `.cogent-baseline.sarif`
3. **History directory** — `~/.quality-history/` → `~/.cogent-history/`
4. **Tool-crate names** — workspace crates renamed from `quality-*` to `cogent-*`

No data migrations are needed — results are ephemeral per-run unless you explicitly save them.

---

## Need Help?

- Read the full documentation in `docs/`
- File an issue on GitHub: https://github.com/KidIkaros/cogent/issues
- Check `PROJECT_STATUS.md` for known limitations and roadmap
