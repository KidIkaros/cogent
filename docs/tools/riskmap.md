# riskmap — Risk Heatmap Analysis

Generates a risk heatmap by combining complexity, churn, and issue density.

## What it measures

- High-complexity + high-churn files (highest risk)
- Files with many past bugs
- Hotspots for refactoring priority

## Configuration

```toml
[riskmap]
max_risk = 50  # Maximum risk score
```

## Examples

```bash
riskmap ./src
riskmap ./src --format json
```

## Remediation

- Prioritize high-risk files for testing
- Refactor highest-risk components first
- Add monitoring/observability to risky areas
