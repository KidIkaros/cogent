# secrets — Hardcoded Credential & Secret Detection

Detects hardcoded credentials, API keys, tokens, and other sensitive data in source code.

## What it detects

- **API keys** — Generic API key patterns, AWS, Google, Azure, Stripe
- **Database connection strings** — Passwords in connection URLs
- **Authentication tokens** — Bearer tokens, JWT secrets, OAuth tokens
- **Private keys** — RSA, DSA, EC private keys, SSH keys
- **Passwords** — Hardcoded passwords and password hashes
- **Secrets in comments** — Token values in code comments
- **Environment variable leaks** — `.env` files committed to source control

## Configuration

```toml
[secrets]
max_secrets = 0  # Zero tolerance by default

# Exclude paths from scanning (comma-separated substrings)
# Supports TOML array syntax and bare comma-separated values:
secrets_exclude = ["vendor", "tests", "docs"]
# or:
secrets_exclude = vendor, tests, docs
# or multi-line:
secrets_exclude = [
  "vendor",
  "tests",
  "docs"
]
```

Path exclusion works by substring matching — any file whose path contains one
of the listed patterns will be skipped. Empty strings in the exclude list are
automatically filtered out.

### Runtime override

The `--secrets-exclude` CLI flag overrides `.quality.toml` at runtime:

```bash
cogent check . --secrets-exclude vendor,tests
# or standalone:
cogent secrets ./src --secrets-exclude vendor,tests
```

Set the `COGENT_SECRETS_EXCLUDE` environment variable for CI pipelines:

```bash
COGENT_SECRETS_EXCLUDE=vendor,tests cogent check .
```

## Output format

```json
{
  "tool": "secrets",
  "passed": false,
  "message": "3 secrets found in 2 files",
  "details": {
    "findings_count": 3,
    "files_scanned": 150
  },
  "findings": [
    {
      "file": "src/config.rs",
      "line": 42,
      "message": "Hardcoded API key: sk_live_...",
      "severity": "critical",
      "rule_id": "secrets-api-key"
    }
  ]
}
```

## Examples

```bash
# Basic scan
secrets ./src

# JSON output for CI
secrets ./src --format json

# Recursive scan
secrets ./ --recursive
```

## Exit codes

- `0` — No secrets found
- `1` — Secrets found (threshold exceeded)

## Remediation

1. Remove hardcoded secrets from source code
2. Use environment variables or secret management tools (Vault, AWS Secrets Manager)
3. Add `.env` files to `.gitignore`
4. Rotate any exposed credentials immediately
5. Consider using pre-commit hooks to block secret commits

## See also

- `taint` — Data flow analysis for secret propagation
- `crypto` — Weak cryptography detection
- `sast` — General security vulnerability scanning
