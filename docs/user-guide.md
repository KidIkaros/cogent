# User Guide — Using Cogent to Improve Your Project

This guide shows how to use Cogent to audit and improve your project's code quality.

## Quick Start (5 Minutes)

1. **Install Cogent**:
   ```bash
   git clone https://github.com/KidIkaros/cogent.git
   cd cogent && cargo build --release
   export PATH="$PWD/target/release:$PATH"
   ```

2. **Bootstrap your project** (auto-detects language, writes `.quality.toml`):
   ```bash
   cogent init
   ```

3. **Run all 21 checks**:
   ```bash
   cogent check .
   ```

4. **Generate an HTML audit report**:
   ```bash
   cogent report . --open
   ```

5. **Interpret results**:
   - Look for `✗` marks (failing checks) in terminal output
   - Report shows Health Score (A–F), category breakdown, and per-finding drill-downs
   - Focus on Critical and High severity items first

## Using Individual Tools

### CRAP Metric (Maintenance Risk)
```bash
cogent crap ./src --recursive
```
- **Target**: CRAP < 15 per function
- **Fix**: Reduce complexity (split functions) + increase test coverage

### Technical Debt Scan
```bash
cogent debt ./src --recursive
```
- **Target**: 0 TODO/FIXME/HACK markers
- **Fix**: Address each marker or convert to tracked issues

### Documentation Coverage
```bash
cogent doccov ./src --recursive
```
- **Target**: >95% public API documentation
- **Fix**: Add doc comments to all public functions/types

### Code Duplication
```bash
cogent run . --format json | jq '.checks[] | select(.name=="dup")'
```
- **Target**: 0 duplication blocks >3 lines
- **Fix**: Extract duplicated code into shared functions

### Security (SAST / Secrets / Crypto)
```bash
cogent check . --only sast,secrets,crypto,taint
```
- **Target**: 0 findings
- **Fix**: Address each finding; use `--verbose` for file:line context

### Fuzz Surface Analysis
```bash
cogent fuzz ./src --recursive
```
- **Target**: Identify high-value fuzz targets
- **Fix**: Add fuzz harnesses for flagged functions

## Batch Mode (Recommended)

```bash
# 1. Auto-detect ecosystem and write .quality.toml
cogent init

# 2. Edit thresholds if needed
vim .quality.toml

# 3. Run all 21 checks
cogent check .

# 4. Generate visual report
cogent report . --open
```

## CI Integration

```bash
# Wire GitHub Actions + pre-commit hook automatically
cogent init --ci
```

Or add manually to your GitHub Actions workflow:
```yaml
- name: Quality Gate
  run: cogent check . --ci   # JSON output, no TTY colors, exits 1 on failure
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

## Watch Mode (live dev loop)

```bash
cogent watch .            # runs debt + doc + crap on every file change
cogent watch . --full     # runs all 21 checks every cycle
cogent watch . --no-tests # skip tests, metrics-only
```

## Understanding Reports

### CRAP Score Explanation
- **0-5**: Excellent (low risk)
- **5-15**: Good (acceptable)
- **15-30**: Poor (needs improvement)
- **>30**: Critical (must fix)

Formula: `CRAP = complexity² × (1 - coverage/100)³ + complexity`

### Severity Levels
- **Critical**: Immediate fix required (blocks merge)
- **High**: Fix before next release
- **Medium**: Fix when convenient
- **Low**: Optional improvement

## Getting Help

- See [Metrics Explained](./metrics-explained.md) for detailed metric definitions
- See [Quality Standards](./quality-standards.md) for target thresholds
- Open an issue for bugs or feature requests
