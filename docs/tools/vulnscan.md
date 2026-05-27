# vulnscan — Vulnerability Scanning

Scans dependencies for known security vulnerabilities using advisory databases.

## What it detects

- Known CVEs in dependencies
- Outdated packages with security fixes available
- Vulnerable transitive dependencies
- Ecosystem-specific advisories (RustSec, npm audit, etc.)

## Configuration

```toml
[vulnscan]
max_vulns = 0  # Zero tolerance
severity_filter = "medium"  # Only report medium+ severity
```

## Examples

```bash
vulnscan ./
vulnscan ./ --format json
```

## Remediation

- Update vulnerable dependencies immediately
- Use `cargo audit` (Rust) or `npm audit` (Node)
- Enable Dependabot or Renovate for auto-updates
- Maintain a vulnerability response plan
