# Cogent Onboarding — 3-Minute Quickstart

Welcome to Cogent! This guide gets you from zero to a passing quality gate in three steps.

---

## Step 1: Initialize your project

```bash
cogent init
```

**Expected output:**
```
  ✓ detected: Rust  (Cargo.toml found)  test: cargo test
  ✓ wrote .quality.toml  (1.2ms)

  ▶ Key thresholds chosen:
    · max_crap    = 15.0
    · min_doc     = 95%
    · max_debt    = 0
    · max_complexity_violations = 0

  ▶ cogent check . runs 31 checks and produces a weighted 0-100 score + letter grade.

  ▶ Next steps:
    1. $ cogent check .          — run all checks now
    2. $ cogent report .         — generate HTML audit report
    3. $ cogent init --ci        — wire GitHub Actions + pre-commit hook
    4. $ cogent watch .          — live re-check on file save
```

> **Tip:** If `cogent` is not in your PATH, use `cargo run -p cogent-cli -- init`.

---

## Step 2: Run your first check

```bash
cogent check . --format text
```

**What happens:**
- 31 checks run in parallel with a live progress bar
- Each check prints ✓ or ✗ with elapsed time
- A summary box shows **Score: X/100** and a letter grade (A–F)
- If anything fails, you see:
  - **Failed by category** — Security 🔴, Quality 🟡, Compliance 🔵
  - **Quick Fixes** box — one-line remediation per failed check

**Example passing run:**
```
  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  PASSED ✓                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  31/31 checks passed  ·  5.1s total                  ║
  ║  Score: 100/100  A                                   ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝
```

---

## Step 3: Fix failures with `cogent explain`

When a check fails, ask Cogent what it means and how to fix it:

```bash
cogent explain crap
cogent explain debt
cogent explain doccov
cogent explain complexity
```

Each prints:
- What the tool measures
- What the threshold means
- How to read the output
- 3 concrete fixes for common findings

> **Pro tip:** `cogent explain <tool>` works for every check name you see in the output.

---

## What each check means

| Check | What it finds | How to fix | Learn more |
|-------|---------------|------------|------------|
| **crap** | Functions that are complex AND untested | Add tests or simplify code | `cogent explain crap` |
| **debt** | TODO / FIXME / HACK / XXX markers | Convert to issues, remove markers | `cogent explain debt` |
| **doccov** | Public functions missing doc comments | Add `///` comments | `cogent explain doccov` |
| **complexity** | Functions with too many branches | Extract helpers, reduce nesting | `cogent explain complexity` |
| **taint** | Untrusted data reaching dangerous calls | Sanitize input, use safe APIs | `cogent explain taint` |
| **dup** | Copy-pasted code blocks | Extract shared functions | `cogent explain dup` |
| **secrets** | Hardcoded keys, tokens, passwords | Use env vars / secret manager | `cogent explain secrets` |
| **vulnscan** | Known CVEs in dependencies | `cargo update` flagged crates | `cogent explain vulnscan` |
| **sast** | SQL injection, XSS, path traversal | Use safe APIs, validate input | `cogent explain sast` |
| **licenses** | OSS license conflicts | Review or replace dependencies | `cogent explain licenses` |

See [docs/tools/](docs/tools/) for deeper per-tool guides.

---

## Common first-run failures (copy-paste fixes)

### "No .quality.toml found"
```bash
cogent init        # auto-detect and create config
cogent check .     # now works
```

### "CRAP score too high"
```bash
cogent crap ./src --format json   # see top offenders
# Fix: add unit tests for the listed functions
cargo test
```

### "Technical debt markers found"
```bash
cogent debt ./src --format json   # see every marker
# Fix: create GitHub issues, then remove markers from code
```

### "Documentation coverage below 95%"
```bash
cogent doccov ./src --format json   # see missing docs
# Fix: add /// to every public function/struct
# Enable: #![warn(missing_docs)] in lib.rs
```

### "Complexity violations"
```bash
cogent complexity ./src --format json   # see offending functions
# Fix: extract nested conditionals into named helpers
```

---

## Understanding your score

Cogent computes a **weighted health score (0–100)** and a letter grade:

| Score | Grade | Meaning |
|-------|-------|---------|
| 90–100 | **A** | Excellent — all critical checks pass |
| 80–89  | **B** | Good — minor quality issues |
| 70–79  | **C** | Fair — several checks failing |
| 60–69  | **D** | Poor — major issues need attention |
| < 60   | **F** | Critical — security or compliance failures |

**Security checks are weighted 3×** (secrets, vulnscan, sast, crypto, taint, errhandle)  
**Compliance checks are weighted 2×** (licenses, sbom)  
**Quality checks are weighted 1×** (everything else)

---

## Next steps

1. **Tune thresholds** — edit `.quality.toml` to match your team's standards
2. **Set up CI** — run `cogent init --ci` to generate a GitHub Actions workflow
3. **Live feedback** — run `cogent watch .` to re-check on every save
4. **Deep docs** — read [docs/quality-standards.md](docs/quality-standards.md) and [docs/metrics-explained.md](docs/metrics-explained.md)
5. **Per-tool guides** — visit [docs/tools/](docs/tools/) for detailed explanations

## Need help?

- Run `cogent explain <tool>` for instant documentation
- Open an issue: https://github.com/KidIkaros/cogent/issues
