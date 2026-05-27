# errhandle — Error Handling Analysis

Validates error handling patterns and checks for swallowed exceptions.

## What it detects

- Empty catch/except blocks
- Generic exception swallowing
- Missing error logging
- Silent failures
- Panic/recover misuse
- Error return values ignored

## Configuration

```toml
[error_handling]
max_swallowed = 0
```

## Examples

```bash
errhandle ./src
errhandle ./src --format json
```

## Remediation

- Always handle errors explicitly
- Log errors with context before recovery
- Never catch generic exceptions without action
- Return errors rather than swallowing them
