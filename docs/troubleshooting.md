# Troubleshooting

Common issues and how to resolve them.

---

## Tool "unavailable" or "skipped"

**Symptom:** You see messages like:
```
Skipped: Tool unavailable: access-control
Skipped: Tool unavailable: supply-chain
```

**Cause:** Individual tool binaries are not installed in your PATH.

**Fix:** Use the full workspace build instead of installing individual tools:

```bash
# Build the entire workspace
git clone https://github.com/KidIkaros/cogent.git
cd cogent
cargo build --release --workspace

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Now all tools are available
cogent check .
```

**Explanation:** Cogent is a workspace of 34 crates. When you install via Homebrew or binary download, you get the unified `cogent` CLI that orchestrates all tools. Individual tools (like `access-control`, `supply-chain`) are run via the CLI dispatcher and don't need to be in PATH.

---

## No .quality.toml found

**Symptom:**
```
! No .quality.toml found.
    → Run cogent init to auto-detect your project and generate one.
```

**Fix:** Run `cogent init`:

```bash
cogent init
```

This detects your project type (Rust, Python, JS/TS, Go) and writes a `.quality.toml` file with language-tuned thresholds.

---

## Slow Execution

**Symptom:** `cogent check .` takes more than 30 seconds.

**Cause:** By default, Cogent runs all 31 tools including slow ones like `mutate` (mutation testing).

**Fix:** Run only fast checks:

```bash
# Run only fast checks (skip mutate, fuzz, supply-chain)
cogent check . --only complexity,debt,doccov,deadcode,linelen,secrets,sast,vulnscan

# Or skip specific slow tools
cogent check . --skip mutate,fuzz,supply-chain
```

---

## Mutation Testing Timeout

**Symptom:** `mutate` check takes 2-10 minutes or hangs.

**Cause:** Mutation testing recompiles and runs your test suite for each mutation.

**Fix:** Limit the number of mutants:

```bash
cogent mutate . --max-mutants 10  # Only test 10 mutations (fast)
```

Or skip mutation testing entirely for quick feedback:

```bash
cogent check . --skip mutate
```

---

## Wrong Ecosystem Detected

**Symptom:** Cogent detects the wrong language (e.g., detects JavaScript when you have a Python project).

**Cause:** Cogent looks for specific files (`package.json`, `pyproject.toml`, `Cargo.toml`) to detect the ecosystem.

**Fix:** Specify the ecosystem manually in `.quality.toml`:

```toml
[cogent]
ecosystem = "python"  # or "rust", "javascript", "go"

[python]
# Python-specific thresholds
max_crap = 20.0
min_doc = 80.0
```

---

## Coverage Not Found

**Symptom:** `doccov` or `mutate` fail with "no coverage file found."

**Cause:** Cogent can't find the coverage output file.

**Fix:** Ensure you've generated coverage first:

```bash
# Rust
cargo llvm-cov --lcov --output-path lcov.info

# Python
pytest --cov --cov-report=lcov:lcov.info

# JavaScript (vitest)
npx vitest run --coverage
```

Then run Cogent:

```bash
cogent check .
```

---

## Permission Denied

**Symptom:** `bash: /usr/local/bin/cogent: Permission denied`

**Fix:** Make the binary executable:

```bash
sudo chmod +x /usr/local/bin/cogent
```

---

## Command Not Found

**Symptom:** `cogent: command not found`

**Cause:** `/usr/local/bin` is not in your PATH.

**Fix:** Add it to your shell config:

```bash
# For bash
echo 'export PATH="/usr/local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# For zsh
echo 'export PATH="/usr/local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

---

## Outdated Version

**Symptom:** You see old behavior or missing features.

**Fix:** Check and upgrade:

```bash
# Check version
cogent --version

# Upgrade (Homebrew)
brew upgrade cogent

# Upgrade (binary download)
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
```

---

## Cache Issues

**Symptom:** Old results persist or changes aren't reflected.

**Fix:** Clear the cache:

```bash
cogent cache clear
cogent check .  # Fresh run
```

---

## CI Failures

**Symptom:** `cogent check . --ci` fails but local passes.

**Cause:** CI environment differences (no coverage, different tool versions).

**Fix:** Use the same config in CI as local:

```yaml
- name: Generate coverage
  run: cargo llvm-cov --lcov --output-path lcov.info
- name: Run Cogent
  run: cogent check . --format json --ci
```

---

## Get More Help

- **Documentation:** https://kidikaros.github.io/cogent/
- **GitHub Issues:** https://github.com/KidIkaros/cogent/issues
- **Discord:** [Join our community](https://discord.gg/cogent)

---

**Still stuck?** Open a GitHub issue with:
1. The command you ran
2. The error message
3. Your OS and Cogent version (`cogent --version`)