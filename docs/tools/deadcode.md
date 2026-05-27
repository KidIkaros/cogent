# deadcode — Dead Code Detection

Finds unused functions, variables, imports, and unreachable code.

## What it detects

- Unused functions and methods
- Unused variables and parameters
- Unused imports/includes
- Unreachable code blocks
- Duplicate code

## Configuration

```toml
[deadcode]
max_dead = 0
```

## Examples

```bash
deadcode ./src
deadcode ./src --format json
```

## Remediation

- Remove unused code (don't just comment out)
- Mark intentionally unused parameters with `_`
- Use IDE/refactoring tools to safely remove
