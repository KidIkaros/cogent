# fuzz — Fuzz Surface Analysis

Analyzes code to identify functions suitable for fuzzing and maps the fuzz surface.

## What it measures

- Functions accepting raw byte slices or strings
- Functions with complex parsing logic
- Public API entry points
- Deserialization functions
- File format parsers

## Configuration

```toml
[fuzz]
max_surface = 50  # Max recommended fuzz targets
```

## Examples

```bash
fuzz ./src
fuzz ./src --format json
```

## Remediation

- Add fuzz tests for high-surface functions
- Validate all inputs at API boundaries
- Use property-based testing for parsers
