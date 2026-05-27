# install-hooks

## Purpose

Install Git hooks that run Cogent checks before commits and pushes. Ensures code quality gates are enforced at the repository level.

## What it does

- Installs pre-commit hook (fast checks: debt, doccov, crap)
- Installs pre-push hook (full check suite)
- Configures hook behavior via `.quality.toml`
- Supports custom hook scripts and chaining

## Flags/options

| Flag | Description |
|------|-------------|
| `--fast` | Install lightweight hooks (metrics only, no tests) |
| `--full` | Install comprehensive hooks (default) |
| `--force` | Overwrite existing hooks |
| `--global` | Install to `~/.git-template/` for new repos |
| `--hook <name>` | Install specific hook only: `pre-commit`, `pre-push` |

## Installed hooks

### Pre-commit (default)

Runs on `git commit`, before message editor:

```bash
#!/bin/sh
# Fast feedback — blocks commit on quality issues
cogent check . --only debt,doccov,crap --ci || exit 1
```

Duration: ~2–4 seconds

### Pre-push

Runs on `git push`, before transfer:

```bash
#!/bin/sh
# Full validation — blocks push on any failure
cogent check . --ci || exit 1
```

Duration: ~6–12 seconds

## Output format

### Installation

```
Installing Cogent Git hooks...

  ✓ .git/hooks/pre-commit  (fast mode: debt, doccov, crap)
  ✓ .git/hooks/pre-push    (full suite: all 26 checks)

Hook behavior configured from .quality.toml:
  max_crap: 15.0
  min_doc_coverage: 95%
  max_debt: 0

To bypass hooks (emergencies only):
  git commit --no-verify
  git push --no-verify
```

### Commit-time feedback

```
Running pre-commit hook...

  ✓ debt   0.2s
  ✓ doccov 0.8s
  ✗ crap   1.2s  Average CRAP 18.5 exceeds threshold 15.0

  ╔══════════════════════════════════════════════════════╗
  ║  PRE-COMMIT FAILED  ·  1 check failed               ║
  ╚══════════════════════════════════════════════════════╝

  • Add tests to reduce CRAP scores
  • Or commit with --no-verify (not recommended)

commit aborted
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Hooks installed successfully |
| `1` | Installation failed (no git repo, permission denied) |
| `2` | Would overwrite existing hooks (use `--force`) |

## Examples

```bash
# Install default hooks (pre-commit + pre-push)
cogent install-hooks

# Fast mode — metrics only, no build/test
cogent install-hooks --fast

# Overwrite existing hooks
cogent install-hooks --force

# Install only pre-commit
cogent install-hooks --hook pre-commit

# Global template for all new repos
cogent install-hooks --global

# Uninstall (remove hooks)
rm .git/hooks/pre-commit .git/hooks/pre-push
```

## Bypassing hooks

**Emergency use only** — commits bypassing hooks should be rare:

```bash
# Skip pre-commit
git commit --no-verify -m "Emergency fix"

# Skip pre-push
git push --no-verify
```

## Custom hook integration

To chain Cogent with existing hooks, source the generated script:

```bash
#!/bin/sh
# .git/hooks/pre-commit

# Run existing checks
./run-existing-checks.sh || exit 1

# Run Cogent
cogent check . --only debt,doccov,crap --ci || exit 1
```

## CI vs hooks

| Approach | When it runs | Scope |
|----------|--------------|-------|
| Hooks | Local, per-commit | Individual developer |
| CI | Remote, per-PR | Entire team |

**Best practice:** Use both — hooks catch issues early, CI enforces team standards.

## See also

- `cogent check` — what hooks run
- `cogent watch` — alternative: continuous monitoring
- Git documentation: `git help hooks`
