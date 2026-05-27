# licenses — License Compliance Check

Verifies open source license compatibility and detects unknown/conflicting licenses.

## What it detects

- Missing license files
- Incompatible license combinations (e.g., GPL in MIT project)
- Unknown/unrecognized licenses
- License conflicts in dependencies

## Configuration

```toml
[licenses]
allowed = ["MIT", "Apache-2.0", "BSD-3-Clause"]
rejected = ["GPL-3.0", "AGPL-3.0"]
```

## Examples

```bash
licenses ./
licenses ./ --format json
```

## Remediation

- Review and approve all dependency licenses
- Replace incompatible dependencies
- Document license decisions in LICENSE file
