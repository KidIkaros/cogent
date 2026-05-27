# supply-chain — Supply Chain Security Audit

Audits dependencies for supply chain risks: typosquatting, unmaintained packages, and suspicious authors.

## What it detects

- Typosquatting attacks (packages with names similar to popular ones)
- Unmaintained dependencies (no updates in > 2 years)
- Unknown/suspicious package authors
- Packages with no source repository link
- Excessive dependency depth

## Configuration

```toml
[supply_chain]
max_risk = 0
```

## Examples

```bash
supply-chain ./
supply-chain ./ --format json
```

## Remediation

- Pin dependency versions
- Use private registries/mirrors
- Audit new dependencies before adding
- Subscribe to security advisories
