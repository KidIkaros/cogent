# coupling — Dependency Coupling Analysis

Analyzes module and file coupling to detect architectural violations.

## What it measures

- Afferent coupling (fan-in): how many modules depend on a given module
- Efferent coupling (fan-out): how many modules a given module depends on
- Instability metric (I = Ce / (Ca + Ce))
- Circular dependencies

## Configuration

```toml
[coupling]
max_circular = 0
max_instability = 0.8
```

## Examples

```bash
coupling ./src
coupling ./src --format json
coupling ./src --graph  # outputs dependency graph
```

## Remediation

- Break circular dependencies with interfaces/events
- Apply Dependency Inversion Principle
- Move shared code to a common module
