# remediate

## Purpose

Auto-remediate supported findings. Applies safe, automated fixes to reduce manual cleanup effort.

## What it does

- Scans for auto-fixable issues
- Applies safe transformations
- Reports what was fixed and what requires manual review
- Creates a diff/patch for review before applying

## Supported fixes

| Tool | Auto-fixable issues |
|------|---------------------|
| `debt` | Remove stale TODO/FIXME comments (configurable age) |
| `linelen` | Wrap long lines (basic heuristics) |
| `comments` | Add missing doc comment stubs |
| `format` | Apply `rustfmt`/`gofmt`/`black`/etc. |
| `licenses` | Update license headers where missing |

**Not auto-fixable:** Security issues (secrets, SAST, crypto) require human review.

## Flags/options

| Flag | Description |
|------|-------------|
| `<path>` | Directory to remediate |
| `--dry-run` | Show what would be fixed without applying |
| `--tool <name>` | Remediate only specific tool's findings |
| `--apply` | Apply fixes (required — safety guard) |
| `--backup` | Create `.bak` files before modifying |
| `--format` | Auto-format after fixing |

## Output format

### Dry run

```
DRY RUN — No changes applied

Found 12 auto-fixable issues:

  debt (8):
    src/old.rs:45  TODO from 2025-01 (stale)
    src/old.rs:67  FIXME from 2024-12 (stale)
    ...

  linelen (4):
    src/wide.rs:12  145 chars → wrap at 100
    ...

Run with --apply to fix these issues.
```

### Applied fixes

```
Applied 12 fixes:

  ✓ debt: Removed 8 stale TODO/FIXME comments
  ✓ linelen: Wrapped 4 long lines

Files modified:
  - src/old.rs (2 changes)
  - src/wide.rs (4 changes)

Backups created:
  - src/old.rs.bak
  - src/wide.rs.bak
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All fixable issues resolved (or none found) |
| `1` | Some issues fixed, some require manual review |
| `2` | Error applying fixes |

## Examples

```bash
# Preview what would be fixed
cogent remediate . --dry-run

# Apply all safe fixes
cogent remediate . --apply --backup

# Fix only debt issues
cogent remediate . --tool debt --apply

# Fix and format
cogent remediate . --apply --format
```

## Safety guidelines

1. **Always use `--dry-run` first** — review the proposed changes
2. **Use `--backup`** — easy rollback if something goes wrong
3. **Commit before remediating** — version control is your safety net
4. **Review the diff** — automated fixes can have edge cases
5. **Run tests after** — ensure fixes don't break functionality

## What requires manual review

| Issue | Why not auto-fixed |
|-------|-------------------|
| Secrets | Could break integrations; needs rotation planning |
| SAST findings | Security-critical; semantic understanding required |
| Crypto issues | Algorithm choice needs architectural decision |
| Complex refactoring | CRAP reduction often needs human design |
| Vulnerabilities | Version bumps may have breaking changes |

## See also

- `cogent audit` — identify issues to remediate
- `cogent exception` — when auto-fix isn't possible
- `cogent diff` — compare before/after snapshots
