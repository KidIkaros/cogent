# exception

## Purpose

Manage policy exceptions — approved deviations from standard security and quality rules. Essential for legacy code, third-party integrations, and accepted risk scenarios.

## What it does

- Lists active exceptions with justification and expiry
- Adds new exceptions with required metadata
- Removes expired or obsolete exceptions
- Validates exceptions against current codebase

## Flags/options

| Flag | Description |
|------|-------------|
| `list` | Show all active exceptions (default) |
| `add` | Add a new exception |
| `remove <id>` | Remove an exception by ID |
| `expire` | Mark expired exceptions |
| `audit` | Validate exceptions still apply |
| `--tool <name>` | Filter by tool (e.g., `secrets`, `sast`) |
| `--file <path>` | Filter by file path |
| `--format <format>` | Output: `text`, `json` |

## Adding exceptions

When adding an exception, you must provide:

| Field | Required | Description |
|-------|----------|-------------|
| `--tool` | Yes | Tool/check name |
| `--file` | Yes | File path (glob patterns supported) |
| `--line` | No | Specific line number (omit for entire file) |
| `--rule` | Yes | Rule ID being excepted |
| `--reason` | Yes | Business justification |
| `--ticket` | Yes | Tracking ticket (JIRA, GitHub issue, etc.) |
| `--expires` | Yes | Expiry date (YYYY-MM-DD) |
| `--approved-by` | Yes | Approver name/email |

## Output format

### List exceptions

```
ID          Tool     File                    Rule           Expires    Ticket
----------  -------  ----------------------  -------------  ---------  --------
EX-001      secrets  src/legacy/config.rs    hardcoded_key  2026-12-31  SEC-1234
EX-002      sast     src/vendor/*.js         eval_usage     2026-06-30  VENDOR-89
EX-003      debt     tests/                  todo_allowed   2026-12-31  TECH-567
```

### Exception details

```
Exception: EX-001
  Tool:        secrets
  File:        src/legacy/config.rs
  Rule:        hardcoded_key
  Reason:      Legacy integration key, no rotation possible
  Ticket:      SEC-1234
  Approved by: security@company.com
  Expires:     2026-12-31
  Days left:   222
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Command succeeded |
| `1` | Exception validation failed |
| `2` | Invalid exception ID |

## Examples

```bash
# List all exceptions
cogent exception list

# Filter by tool
cogent exception list --tool secrets

# Add new exception
cogent exception add \
  --tool secrets \
  --file src/legacy/config.rs \
  --rule hardcoded_key \
  --reason "Legacy integration key" \
  --ticket "SEC-1234" \
  --expires 2026-12-31 \
  --approved-by "security@company.com"

# Remove exception
cogent exception remove EX-001

# Audit exceptions (find stale/invalid)
cogent exception audit

# Export for compliance review
cogent exception list --format json > exceptions.json
```

## Compliance notes

- All exceptions require business justification and ticket reference
- Maximum exception duration: 1 year (requires re-approval)
- Security-critical rules (secrets, vulnerabilities) require CISO approval
- Expired exceptions automatically trigger audit failures

## See also

- `cogent policy` — view policy rules being excepted
- `cogent audit` — runs validate exceptions as part of audit
- `cogent remediate` — try auto-fix before requesting exception
