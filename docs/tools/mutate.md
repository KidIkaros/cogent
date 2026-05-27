# mutate — Mutation Testing

Verifies test quality by introducing code mutations and checking if tests catch them.

## What it measures

- Mutation score: % of mutations killed by tests
- Surviving mutations indicate weak test coverage
- Test effectiveness for edge cases

## Configuration

```toml
[mutation]
min_score = 80.0  # Minimum mutation score %
max_mutants = 50  # Limit mutations for speed
```

## Examples

```bash
mutate ./
mutate ./ --format json
mutate ./ --max-mutants 20  # Faster, fewer mutations
```

## Remediation

- Add tests for surviving mutations
- Cover boundary conditions
- Add assertions for all code paths
