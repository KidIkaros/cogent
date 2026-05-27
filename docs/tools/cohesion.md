# cohesion — LCOM (Lack of Cohesion of Methods) Analysis

Measures class cohesion by analyzing method field usage overlap.

## What it measures

- LCOM metric per class/struct
- Methods sharing fields vs isolated methods
- God classes with low cohesion

## Configuration

```toml
[cohesion]
max_lcom = 50  # 0 = perfect cohesion, 100 = no cohesion
```

## Examples

```bash
cohesion ./src
cohesion ./src --format json
```

## Remediation

- Split low-cohesion classes into focused components
- Group related methods with shared fields
- Extract unrelated functionality into separate classes
