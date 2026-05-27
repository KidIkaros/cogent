# linelen — Line Length Check

Enforces maximum line length for readability.

## What it measures

- Lines exceeding configured maximum length
- Average line length per file
- Longest lines in codebase

## Configuration

```toml
[line_length]
max_length = 100
```

## Examples

```bash
linelen ./src
linelen ./src --format json
```

## Remediation

- Break long lines at logical points
- Extract complex expressions into variables
- Use multi-line formatting for function calls
