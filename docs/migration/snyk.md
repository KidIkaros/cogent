# Migrating from Snyk to Cogent

This guide helps you migrate from Snyk to Cogent, showing how Snyk features map to Cogent tools and how to set comparable thresholds.

---

## Why Migrate?

| Aspect | Snyk | Cogent |
|--------|------|--------|
| Cost | Enterprise pricing expensive | Open-source, free forever |
| Privacy | Cloud-based, proprietary | Local-first, no cloud upload |
| Customization | Limited rule customization | Full configuration via .quality.toml |
| Multi-language | Excellent | Excellent (9 languages) |
| CI/CD Integration | Good | Excellent (native SARIF, GitHub Actions) |
| Self-hosted | Paid tier | Fully self-hosted |

---

## Feature Mapping

| Snyk Feature | Cogent Tool | Command |
|--------------|-------------|---------|
| Vulnerability scanning | vulnscan | `cogent vulnscan .` |
| License compliance | licenses | `cogent licenses .` |
| Supply chain risks | supply-chain | `cogent supply-chain .` |
| SAST (code vulnerabilities) | sast | `cogent sast .` |
| Weak cryptography | crypto | `cogent crypto .` |
| Secrets detection | secrets | `cogent secrets .` |
| Code quality (complexity) | complexity | `cogent complexity .` |
| Technical debt | debt | `cogent debt .` |

---

## Snyk Thresholds vs Cogent Configuration

### Snyk Policy: Critical/High Vulnerabilities

**Snyk config:**
```yaml
# .snyk
severity: critical
severity: high
```

**Cogent equivalent (.quality.toml):**
```toml
[vulnscan]
max_critical = 0
max_high = 0
```

---

### Snyk Policy: License Violations

**Snyk config:**
```yaml
# .snyk
license:
  allow:
    - MIT
    - Apache-2.0
    - BSD-3-Clause
  deny:
    - GPL-3.0
    - AGPL-3.0
```

**Cogent equivalent (.quality.toml):**
```toml
[licenses]
max_violations = 0

[licenses.whitelist]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause"]
deny = ["GPL-3.0", "AGPL-3.0"]
```

---

### Snyk Policy: Code Quality

**Snyk config:**
```yaml
# Snyk Code
complexity:
  threshold: 10
```

**Cogent equivalent (.quality.toml):**
```toml
[complexity]
threshold = 10
```

---

## 6-Step Migration Process

### Step 1: Install Cogent

**macOS:**
```bash
brew install cogent
```

**Linux:**
```bash
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
```

**Docker:**
```bash
docker pull ghcr.io/kidikaros/cogent:latest
```

---

### Step 2: Initialize Your Project

```bash
cd your-project
cogent init
```

This auto-detects your ecosystem and generates `.quality.toml`.

---

### Step 3: Migrate Thresholds

Copy your Snyk policy thresholds to `.quality.toml`:

**Snyk policy:**
```yaml
# .snyk
severity: critical
license:
  allow: [MIT, Apache-2.0]
```

**Cogent equivalent (.quality.toml):**
```toml
[vulnscan]
max_critical = 0

[licenses]
max_violations = 0

[licenses.whitelist]
allow = ["MIT", "Apache-2.0"]
```

---

### Step 4: Run Your First Audit

```bash
cogent check .
```

This runs all 31 checks (vulnerabilities, licenses, code quality, security, etc.).

---

### Step 5: Compare Results

| Snyk Result | Cogent Equivalent |
|-------------|-------------------|
| `snyk test` | `cogent check .` |
| `snyk code test` | `cogent sast .` |
| `snyk test --docker` | `cogent vulnscan .` (for deps) |
| `snyk monitor` | `cogent check . --format sarif` (upload to GitHub Security tab) |

---

### Step 6: Update CI/CD

**Snyk (GitHub Actions):**
```yaml
- name: Run Snyk to check for vulnerabilities
  uses: snyk/actions/node@master
  env:
    SNYK_TOKEN: ${{ secrets.SNYK_TOKEN }}
```

**Cogent (GitHub Actions):**
```yaml
- name: Run Cogent audit
  run: cogent check . --format sarif --ci

- name: Upload SARIF to GitHub Security tab
  uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: cogent-results.sarif
```

No API tokens required!

---

## CI/CD Replacement Example

### Before (Snyk):

```yaml
name: Security Scan
on: [push, pull_request]

jobs:
  snyk:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Run Snyk
        uses: snyk/actions/node@master
        env:
          SNYK_TOKEN: ${{ secrets.SNYK_TOKEN }}
```

### After (Cogent):

```yaml
name: Security Scan
on: [push, pull_request]

jobs:
  cogent:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Cogent
        run: |
          curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
          tar xzf cogent-linux-x86_64.tar.gz
          sudo cp cogent-linux-x86_64/cogent /usr/local/bin/

      - name: Run Cogent audit
        run: cogent check . --format sarif --ci

      - name: Upload SARIF to GitHub Security tab
        uses: github/codeql-action/upload-sarif@v2
        with:
          sarif_file: cogent-results.sarif
```

**Benefits:**
- No API tokens (works out of the box)
- Runs locally first (faster PR feedback)
- Comprehensive (31 tools vs just vulnerabilities)
- Full control over thresholds

---

## Migration Checklist

- [ ] Install Cogent
- [ ] Run `cogent init` in your project
- [ ] Map Snyk policy to `.quality.toml` thresholds
- [ ] Run `cogent check .` and compare results with Snyk
- [ ] Update CI/CD workflows
- [ ] Remove Snyk API token from GitHub Secrets
- [ ] Cancel Snyk subscription

---

## Common Issues

### Issue: Cogent finds fewer vulnerabilities than Snyk

**Cause:** Snyk's proprietary database vs Cogent's OSV/CVE integration.

**Solution:** Cogent uses OSV (Google) + NVD for CVE data. If you need Snyk's proprietary intelligence, you can run both in parallel during migration.

---

### Issue: Cogent finds MORE issues than Snyk

**Cause:** Cogent includes code quality, technical debt, and design docs — not just security.

**Solution:** This is intentional! Cogent is a full quality audit, not just security. If you only want security, run specific tools:

```bash
cogent vulnscan .  # Vulnerabilities only
cogent sast .      # SAST only
cogent secrets .   # Secrets only
```

---

### Issue: I need Snyk's container scanning

**Cogent doesn't support container scanning yet.** Workaround:

1. Use Trivy for container scanning:
   ```bash
   trivy image your-image:latest
   ```

2. Use Cogent for code auditing:
   ```bash
   cogent check .
   ```

3. Combine both in CI/CD.

---

## Getting Help

- **Quickstart:** See [quickstart.md](../quickstart.md)
- **Installation:** See [installation.md](../installation.md)
- **Troubleshooting:** See [troubleshooting.md](../troubleshooting.md)
- **GitHub Issues:** https://github.com/KidIkaros/cogent/issues

---

## What's Next?

After migrating from Snyk:

1. **Explore all Cogent tools:** You now have access to 31 tools, not just security!
2. **Set up pre-commit hooks:** `cogent install-hooks` for local feedback
3. **Enable watch mode:** `cogent watch .` for live re-checking on file save
4. **Read other migration guides:**
   - [SonarQube](sonarqube.md)
   - [CodeQL](codeql.md) (coming soon)

---

**Welcome to Cogent! 🚀**