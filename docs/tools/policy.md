# policy

## Purpose

View, validate, and manage compliance policies. Policies define the security and quality standards your codebase must meet.

## What it does

- Displays current policy configuration
- Validates `.quality.toml` against schema
- Shows policy violations from last audit
- Manages organization-wide policy templates

## Flags/options

| Flag | Description |
|------|-------------|
| `show` | Display current policy (default) |
| `validate` | Check `.quality.toml` for errors |
| `template` | Generate a policy template |
| `init` | Create default policy from current settings |
| `--strict` | Fail validation on warnings |
| `--output <file>` | Write template/policy to file |

## Policy structure

A policy is defined in `.quality.toml`:

```toml
[thresholds]
max_crap = 15.0
min_doc_coverage = 95.0
max_debt = 0
max_complexity_violations = 0

[security]
block_on_secrets = true
block_on_vulnerabilities = true
allowed_licenses = ["MIT", "Apache-2.0", "BSD-3-Clause"]

[ignore]
paths = ["tests/", "examples/"]
```

## Output format

### Policy show

```
Active Policy: .quality.toml

Thresholds:
  max_crap                    15.0
  min_doc_coverage            95%
  max_debt                    0
  max_complexity_violations   0

Security:
  block_on_secrets            true
  block_on_vulnerabilities    true

Ignored paths:
  - tests/
  - examples/
```

### Validation output

```
Validating .quality.toml...

✓ Schema valid
✓ All thresholds within acceptable range
✓ License list compatible with Apache-2.0

Policy ready for enforcement.
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Policy valid / command succeeded |
| `1` | Policy has errors |
| `2` | No policy file found |

## Examples

```bash
# Show current policy
cogent policy show

# Validate policy file
cogent policy validate

# Generate template for new project
cogent policy template --output .quality.toml

# Create policy from current project
cogent policy init

# Strict validation (warnings as errors)
cogent policy validate --strict
```

## Policy templates

Cogent includes built-in templates for common scenarios:

- **strict**: Maximum security posture (all checks enabled, low thresholds)
- **balanced**: Reasonable defaults for most projects
- **minimal**: Essential checks only (security + compliance)
- **custom**: Your organization's specific requirements

## See also

- `cogent audit` — run audit against current policy
- `cogent exception` — manage policy exceptions
- `.quality.toml` reference in user guide
