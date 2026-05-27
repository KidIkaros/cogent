# access-control — Authentication & Authorization Audit

Validates access control patterns, role checks, and permission enforcement in code.

## What it detects

- Missing authentication checks on sensitive endpoints
- Inconsistent authorization patterns across codebase
- Hardcoded role strings instead of constants/enums
- Missing rate limiting annotations
- Privilege escalation paths

## Configuration

```toml
[access_control]
max_violations = 0
```

## Examples

```bash
access-control ./src
access-control ./src --format json
```

## Remediation

- Centralize authorization in middleware/guards
- Use role/permission enums, not string literals
- Add rate limiting to all public endpoints
- Audit admin endpoints quarterly
