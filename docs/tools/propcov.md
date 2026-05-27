# propcov — Property-Based Test Coverage

Analyzes code for property-based testing opportunities and coverage.

## What it measures

- Functions suitable for property-based testing
- Input domains and invariants
- Edge case coverage gaps

## Configuration

```toml
[propcov]
min_coverage = 80.0
```

## Examples

```bash
propcov ./src
propcov ./src --format json
```

## Remediation

- Add property tests for pure functions
- Test invariants and preconditions
- Use fuzzing for complex input validation
