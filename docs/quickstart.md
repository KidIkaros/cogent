# Cogent Quickstart — 5 Minutes to Better Code

Get started with Cogent in 5 minutes. This guide walks you through installing Cogent, running your first audit, and fixing common issues.

---

## Step 1: Install Cogent (1 min)

### macOS (Recommended)

```bash
brew tap kidikaros/cogent
brew install cogent
```

### Linux

```bash
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
```

### Windows

Download the latest `.zip` from https://github.com/KidIkaros/cogent/releases/latest and extract to your PATH.

### Verify Installation

```bash
cogent --version
# Should output: cogent 1.2.0
```

---

## Step 2: Try on Example Repo (2 min)

Clone a demo project with intentional bugs and see Cogent in action:

```bash
git clone https://github.com/KidIkaros/cogent-example.git
cd cogent-example

# Run the full audit
cogent check .
```

You'll see a report with findings across categories:

```
  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  FAILED ✗                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  23/31 checks passed  ·  Score: 74/100 C            ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝

🟡 QUALITY FINDINGS
  ┌──────────────────────────────────────────────────┐
  │ Tool            │ Score  │ Threshold │ Status   │
  ├──────────────────────────────────────────────────┤
  │ complexity       │ 8      │ 5         │ ✗ FAIL  │
  │ debt             │ 3      │ 0         │ ✗ FAIL  │
  │ doccov           │ 12%    │ 95%       │ ✗ FAIL  │
  │ deadcode         │ 1      │ 0         │ ✗ FAIL  │
  │ debuggability    │ 62%    │ 90%       │ ✗ FAIL  │
  │ linelen          │ 2      │ 0         │ ✗ FAIL  │
  └──────────────────────────────────────────────────┘

🔒 SECURITY FINDINGS
  ┌──────────────────────────────────────────────────┐
  │ Tool            │ Score  │ Threshold │ Status   │
  ├──────────────────────────────────────────────────┤
  │ secrets          │ 0      │ 0         │ ✓ PASS  │
  │ vulnscan         │ 0 CVEs │ 0         │ ✓ PASS  │
  │ sast             │ 0      │ 0         │ ✓ PASS  │
  └──────────────────────────────────────────────────┘

🔧 COMPLIANCE FINDINGS
  ┌──────────────────────────────────────────────────┐
  │ Tool            │ Score  │ Threshold │ Status   │
  ├──────────────────────────────────────────────────┤
  │ licenses         │ OK     │ OK        │ ✓ PASS  │
  │ design-docs      │ 4/7    │ 5/7       │ ✗ FAIL  │
  └──────────────────────────────────────────────────┘

⚠️  QUICK FIXES
  1. Reduce function complexity in src/main.rs:5 (score: 8, threshold: 5)
  2. Remove TODO/FIXME/HACK markers (3 found)
  3. Remove unused function unused_helper
  4. Replace .unwrap() calls with proper error handling
  5. Split long function (>50 lines)
```

---

## Step 3: Audit Your Own Project (1 min)

Navigate to your project and run Cogent:

```bash
cd /path/to/your/project

# Auto-detect your ecosystem and write .quality.toml
cogent init

# Run all checks
cogent check .
```

If you don't have tests yet, or want faster feedback:

```bash
# Skip slow checks (mutation testing, fuzzing)
cogent check . --only complexity,debt,doccov,deadcode,linelen
```

---

## Step 4: Understand the Output

### Score (0-100, Grade A-F)

| Score | Grade | Meaning |
|-------|-------|---------|
| 90-100 | **A** | Excellent — all critical checks pass |
| 80-89  | **B** | Good — minor quality issues |
| 70-79  | **C** | Fair — several checks failing |
| 60-69  | **D** | Poor — major issues need attention |
| < 60   | **F** | Critical — security or compliance failures |

**Note:** Security checks are weighted 3×, Compliance checks 2×, Quality checks 1×. A single exposed secret or CVE will fail the gate.

### Categories

- 🔴 **Security**: secrets, vulnscan, sast, crypto, taint, errhandle, access-control
- 🟡 **Quality**: crap, debt, doccov, riskmap, dupfind, coupling, complexity, linelen, halstead, deadcode, cohesion, comments, propcov, typecov, fuzz, mutate
- 🔵 **Compliance**: licenses, sbom, supply-chain, outdated
- 🟢 **Operations**: observability, test-quality, design-docs, debuggability

---

## Step 5: Fix Common Issues (1 min)

### High Cyclomatic Complexity

**What it is:** Too many nested if/else statements

**Example:**
```rust
// BAD (complexity: 8)
if priority {
    if expedite {
        if verify {
            return Ok("done");
        }
    }
}

// GOOD (complexity: 2)
if priority {
    return expedite_order(expedite, verify)?;
}
fn expedite_order(expedite: bool, verify: bool) -> Result<String, String> {
    if expedite && verify {
        Ok("done".to_string())
    } else {
        Err("expedite and verify both required".to_string())
    }
}
```

### Technical Debt Markers

**What it is:** TODO/FIXME/HACK comments that never get fixed

**Fix:** Either implement the fix, or create a GitHub issue:

```rust
// BAD
// TODO: Fix this before production

// GOOD
// See https://github.com/your-org/your-repo/issues/42
```

### Contextless Unwraps

**What it is:** `.unwrap()` calls that crash without helpful error messages

**Example:**
```rust
// BAD
let conn = pool.get().unwrap();

// GOOD
let conn = pool.get().context("Failed to get DB connection")?;
```

### Missing Documentation

**What it is:** Public functions without `///` docs

**Fix:**
```rust
/// Process an order for a user.
///
/// # Arguments
/// * `user_id` - The user ID to process the order for
/// * `items` - List of items in the order
///
/// # Returns
/// A success message or error string
pub async fn process_order(user_id: u64, items: Vec<String>) -> Result<String, String> {
    // ...
}
```

---

## Step 6: Generate a Report

```bash
# Generate HTML report with drill-downs
cogent report . --open

# Generate SARIF for GitHub Security tab
cogent check . --format sarif --output cogent.sarif
```

---

## Next Steps

### Add to CI/CD

Add to your `.github/workflows/quality.yml`:

```yaml
name: Quality Gate
on: [push, pull_request]
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo install --git https://github.com/KidIkaros/cogent cogent-cli
      - run: cogent check . --format json --ci
      - run: cogent check . --format sarif --output cogent.sarif
      - uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: cogent.sarif
```

### Add Pre-Commit Hook

```bash
cogent install-hooks
```

Now every commit runs Cogent and blocks low-quality changes.

### Customize Thresholds

Edit `.quality.toml`:

```toml
[cogent]
score_min = 80  # Require at least a B grade

[complexity]
max = 3  # Stricter than default (5)

[doccov]
min = 80  # Looser than default (95)
```

---

## Common Questions

### Q: Cogent takes too long on my large project

**A:** Use `--only` to run only the checks you care about:

```bash
# Fast mode: skip slow checks
cogent check . --only complexity,debt,doccov,deadcode

# Security-only mode
cogent check . --only secrets,sast,crypto,vulnscan
```

### Q: I don't agree with a finding

**A:** Add it to `.cogent-exceptions.yaml`:

```yaml
complexity:
  ignore:
    - "**/legacy/**"  # Skip legacy code
```

### Q: How do I integrate with my CI/CD?

**A:** See [CI/CD Integration](./cicd.md) for GitHub Actions, GitLab CI, and more.

### Q: Can I use this with [SonarQube/Snyk/CodeQL]?

**A:** Cogent replaces all three. If you're migrating, see:

- [SonarQube Migration Guide](./migration/sonarqube.md)
- [Snyk Migration Guide](./migration/snyk.md)
- [CodeQL Migration Guide](./migration/codeql.md)

---

## Get Help

- **Documentation:** https://kidikaros.github.io/cogent/
- **GitHub Issues:** https://github.com/KidIkaros/cogent/issues
- **Discord:** [Join our community](https://discord.gg/cogent)

---

**That's it!** You've audited code, fixed issues, and wired up Cogent in 5 minutes. Happy hacking! 🚀