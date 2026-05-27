# typecov — Type Coverage Analysis

Measures how much of the codebase uses explicit type annotations.

## What it measures

- Percentage of functions with type annotations
- Untyped parameters and return values
- Missing generic type constraints

## Configuration

```toml
[typecov]
min_coverage = 90.0
```

## Examples

```bash
typecov ./src
typecov ./src --format json
```

## Remediation

- Add type annotations to all public APIs
- Enable strict type checking in compiler/interpreter
- Use type inference for internal code where appropriate
