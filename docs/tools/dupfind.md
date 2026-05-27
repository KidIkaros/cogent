# dupfind — Code Duplication Detection

Finds duplicate code blocks and near-duplicates across the codebase.

## What it detects

- Exact code duplicates (copy-paste)
- Near-duplicates with minor variations
- Duplicate logic across files
- Repeated patterns that should be extracted

## Configuration

```toml
[duplication]
max_duplication = 3  # Max % of duplicated code
```

## Examples

```bash
dupfind ./src
dupfind ./src --format json
dupfind ./src --min-lines 5  # Minimum lines to consider
```

## Remediation

- Extract duplicated code into shared functions
- Use template methods for algorithm families
- Create utility libraries for common operations
