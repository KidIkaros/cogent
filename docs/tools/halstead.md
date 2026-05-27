# halstead — Halstead Complexity Metrics

Computes Halstead software science metrics for code complexity measurement.

## What it measures

- **Program Length (N)** — total operators + operands
- **Program Vocabulary (n)** — unique operators + operands
- **Volume (V)** — N * log2(n)
- **Difficulty (D)** — (n1/2) * (N2/n2)
- **Effort (E)** — V * D
- **Time to Program (T)** — E / 18 (in seconds)
- **Bugs Delivered (B)** — V / 3000

## Configuration

```toml
[halstead]
max_effort = 5000
max_volume = 1000
```

## Examples

```bash
halstead ./src
halstead ./src --format json
```

## Remediation

- Break high-volume functions into smaller ones
- Reduce operator/operand complexity
- Extract repeated expressions into variables
