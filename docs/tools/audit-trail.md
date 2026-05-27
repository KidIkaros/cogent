# audit-trail

## Purpose

View and manage the audit history trail. Tracks all audit runs, findings, and resolutions over time for compliance reporting and trend analysis.

## What it does

- Lists historical audit runs with timestamps
- Shows which findings were introduced, resolved, or persist
- Generates compliance reports for auditors
- Supports filtering by date range, severity, and category

## Flags/options

| Flag | Description |
|------|-------------|
| `list` | Show all audit runs (default subcommand) |
| `show <id>` | Display detailed report for a specific audit |
| `diff <id1> <id2>` | Compare two audit runs |
| `export` | Export audit trail to CSV or JSON |
| `--since <date>` | Filter audits after date (YYYY-MM-DD) |
| `--until <date>` | Filter audits before date (YYYY-MM-DD) |
| `--format <format>` | Output format: `text`, `json`, `csv` |

## Output format

### List view

```
ID          Date                      Score  Grade  Duration
----------  ------------------------  -----  -----  --------
audit-123   2026-05-23T14:32:00Z      92     A      4.2s
audit-122   2026-05-22T09:15:00Z      88     B      3.8s
audit-121   2026-05-21T16:45:00Z      85     B      4.1s
```

### Diff view

```
Comparing audit-121 → audit-123

Fixed:
  ✓ secrets: 2 hardcoded keys removed
  ✓ sast: 1 SQLi vulnerability patched

New:
  ✗ crypto: 1 new weak hash introduced (src/auth.rs:45)

Persistent:
  ⚠ debt: 12 TODOs still present
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Command executed successfully |
| `1` | No audit trail found |
| `2` | Invalid audit ID or date range |

## Examples

```bash
# List all audits
cogent audit-trail list

# Show specific audit
cogent audit-trail show audit-123

# Compare two audits
cogent audit-trail diff audit-121 audit-123

# Export for compliance
cogent audit-trail export --since 2026-01-01 --format csv > q1-audits.csv

# Filter by date range
cogent audit-trail list --since 2026-05-01 --until 2026-05-23
```

## Compliance integration

The audit trail is stored in `.cogent-history/` and can be:

- Committed to version control for audit history
- Exported for external compliance tools
- Used in CI to track security posture over time

## See also

- `cogent audit` — run a new audit
- `cogent history` — view HTML historical reports
- `cogent policy` — compliance policy management
