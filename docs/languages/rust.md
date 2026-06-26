# Cogent for Rust Projects

Complete guide to using Cogent with Rust projects.

---

## Rust-Specific Tools

Cogent includes 10 tools specialized for Rust:

| Tool | What it checks | Command |
|------|----------------|---------|
| **complexity** | Cyclomatic complexity | `cogent complexity .` |
| **debt** | TODO/FIXME/HACK markers | `cogent debt .` |
| **doccov** | Documentation coverage for public APIs | `cogent doccov .` |
| **deadcode** | Unused functions and dead code | `cogent deadcode .` |
| **errhandle** | Unsafe unwrap/expect/panic patterns | `cogent errhandle .` |
| **mutation** | Mutation testing (requires tests) | `cogent mutate . -p my-crate` |
| **typecov** | Type annotation coverage | `cogent typecov .` |
| **crap** | CRAP score (maintenance risk) | `cogent crap .` |
| **riskmap** | Risk map (churn × complexity) | `cogent riskmap .` |
| **observability** | Structured logging/tracing | `cogent observability .` |

---

## Quick Start

### 1. Initialize Cogent

```bash
cd your-rust-project
cogent init
```

Output:
```
… detecting project ecosystem
  ✓ detected: Rust  (Cargo.toml found)  test: cargo test
  ✓ wrote .quality.toml  (0.0s)

  ▶ Key thresholds chosen:
    · max_crap    = 15
    · min_doc     = 95%
    · max_debt    = 0
    · max_complexity_violations = 0
```

### 2. Run Full Audit

```bash
cogent check .
```

Output:
```
  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  PASSED ✓                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  31/31 checks passed  ·  5.1s total                  ║
  ║  Score: 100/100  A                                   ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝
```

### 3. Generate Report

```bash
cogent report . --open
```

Opens HTML report in browser.

---

## Example: Finding Rust Issues

### 1. High Complexity

**Code:**
```rust
pub fn process_order(user_id: u64, items: Vec<String>, priority: bool, expedited: bool, discount: Option<f64>, tax_rate: f64, shipping: f64) -> Result<Order, Error> {
    // 200 lines of nested conditions...
}
```

**Cogent output:**
```
✗ complexity: src/main.rs:15
  Function 'process_order' has complexity 23 (threshold: 5)
  Help: Break into smaller functions, use early returns
```

**Fix:**
```rust
pub fn process_order(...) -> Result<Order, Error> {
    validate_user(user_id)?;
    validate_items(&items)?;
    calculate_pricing(priority, expedited, discount, tax_rate, shipping)
}

fn validate_user(user_id: u64) -> Result<(), Error> {
    // ...
}
```

---

### 2. Unsafe Unwrap

**Code:**
```rust
let config = config_path.read_to_string().unwrap();
let data: Config = serde_json::from_str(&config).unwrap();
```

**Cogent output:**
```
✗ errhandle: src/config.rs:10
  Contextless unwrap at line 10
  Help: Use ? operator or match for error handling
```

**Fix:**
```rust
let config = config_path.read_to_string()?;
let data: Config = serde_json::from_str(&config)?;
```

---

### 3. Missing Documentation

**Code:**
```rust
pub fn calculate_tax(amount: f64, rate: f64) -> f64 {
    amount * rate
}
```

**Cogent output:**
```
✗ doccov: src/tax.rs:5
  Public function 'calculate_tax' missing documentation
  Help: Add /// doc comment with parameters and return type
```

**Fix:**
```rust
/// Calculates tax amount.
///
/// # Arguments
///
/// * `amount` - Base amount before tax
/// * `rate` - Tax rate as decimal (e.g., 0.08 for 8%)
///
/// # Returns
///
/// Tax amount
pub fn calculate_tax(amount: f64, rate: f64) -> f64 {
    amount * rate
}
```

---

### 4. Technical Debt Markers

**Code:**
```rust
// TODO: Handle error properly
// FIXME: This is slow, optimize later
// HACK: Quick workaround for production
```

**Cogent output:**
```
✗ debt: src/main.rs:20
  TODO marker found at line 20
  Help: Address technical debt or remove marker
```

**Fix:**
```rust
// Addressed: Error handling now uses proper Result types
// Optimized: Replaced O(n^2) with O(n log n) algorithm
// Removed: Replaced with proper implementation
```

---

## Cargo.toml Recommendations

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
criterion = "0.5"  # For performance benchmarks
proptest = "1.0"   # For property-based testing
```

These enable:
- **Riskmap** (needs git churn data) — `criterion` for benchmarking
- **Propcov** (property test coverage) — `proptest` for property tests

---

## Configuration Example

**.quality.toml:**
```toml
# Rust-specific thresholds
[complexity]
threshold = 5

[doccov]
min_coverage = 0.95  # 95% of public APIs documented

[debt]
max_todo = 0
max_fixme = 0
max_hack = 0

[mutation]
enabled = true
max_mutants = 100

[errhandle]
max_unwrap = 0
max_expect = 0

[crap]
threshold = 15
```

---

## Integration with Cargo

### Pre-commit Hook

```bash
cogent install-hooks
```

This adds `.git/hooks/pre-commit`:
```bash
#!/bin/bash
cogent check . --format text
exit $?
```

Now `git commit` will fail if quality checks fail.

---

### GitHub Actions

**.github/workflows/cogent.yml:**
```yaml
name: Cogent Audit
on: [push, pull_request]

jobs:
  audit:
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

---

## Workflows

### Local Development Loop

```bash
# Terminal 1: Watch mode
cogent watch .

# Terminal 2: Edit code
vim src/main.rs

# Cogent auto-rechecks on file save
```

### CI/CD Gate

```yaml
- name: Quality Gate
  run: cogent check . --ci
```

Fails if score < 80 or any check fails.

---

## Common Pitfalls

### 1. Mutation Testing Requires Passing Tests

Before running `cogent mutate`, ensure tests pass:

```bash
cargo test
cogent mutate . -p my-crate
```

### 2. Riskmap Needs Git History

Riskmap requires git churn data. Initialize git if not already:

```bash
git init
git add .
git commit -m "Initial commit"
cogent riskmap .
```

### 3. Doccov Requires Cargo Metadata

Doccov reads `cargo doc` output. Ensure your project builds:

```bash
cargo build
cogent doccov .
```

---

## Pro Tips

1. **Run `cogent init --ci`** — Auto-wires GitHub Actions + pre-commit hook + baseline
2. **Use `cogent watch .`** — Live re-checking on file save
3. **Configure thresholds** — Edit `.quality.toml` to match your project's standards
4. **Generate HTML reports** — `cogent report . --open` for shareable reports
5. **Use `cogent explain <tool>`** — Learn what each tool measures

---

## Example Rust Projects

Try Cogent on these Rust projects:

```bash
# Clone Cogent example repo
git clone https://github.com/KidIkaros/cogent-example.git
cd cogent-example
cogent init
cogent check .
```

Or audit your own project!

---

## Next Steps

- **Learn more:** See [developer-guide.md](../developer-guide.md)
- **Explore other tools:** See [tools/](../tools/)
- **Migration guides:** See [migration/](migration/)

---

**Happy Rust auditing! 🦀**