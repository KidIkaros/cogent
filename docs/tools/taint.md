# taint — Data Flow & Taint Analysis

Tracks untrusted user input through the application to find injection points.

## What it detects

- Untrusted data reaching SQL queries
- User input in command execution
- Reflected XSS data flows
- Unsafe deserialization paths

## Configuration

```toml
[taint]
max_taint = 0
```

## Examples

```bash
taint ./src
taint ./src --format json
```
