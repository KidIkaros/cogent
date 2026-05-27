# comments — Comment Ratio Analysis

Measures the ratio of comment lines to code lines.

## What it measures

- Percentage of commented lines vs total lines
- Documentation coverage (doc comments vs code)
- Files with zero comments

## Configuration

```toml
[comments]
min_ratio = 10.0  # Minimum 10% comment ratio
```

## Examples

```bash
comments ./src
comments ./src --format json
```

## Remediation

- Add doc comments to public APIs
- Explain complex algorithms inline
- Document business logic decisions
