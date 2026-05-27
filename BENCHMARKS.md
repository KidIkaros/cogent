# Cogent Performance Benchmarks

This directory contains Criterion benchmarks for measuring Cogent scan performance and detecting regressions.

## Running Benchmarks

### Quick run
```bash
cargo bench -p cogent-cli
```

### Test mode (fast, no measurements)
```bash
cargo bench -p cogent-cli -- --test
```

### Specific benchmark group
```bash
cargo bench -p cogent-cli -- cogent_check
cargo bench -p cogent-cli -- report_generation
cargo bench -p cogent-cli -- individual_tools
```

## Fixture Generation

Generate synthetic code fixtures for stress testing:

```bash
cd crates/cogent-cli/benches/fixtures

# Small: 50 files (~500 functions)
python3 generate.py small --include-issues

# Medium: 500 files (~10k functions)
python3 generate.py medium

# Large: 2000 files (~100k functions)
python3 generate.py large
```

## Benchmark Groups

### `cogent_check` — End-to-end scan performance
Measures full `cogent check . --force` runtime across fixture sizes.

| Fixture | Files | Est. Functions |
|---------|-------|----------------|
| small   | 50    | ~500           |
| medium  | 500   | ~10,000        |
| large   | 2000  | ~100,000       |

### `report_generation` — Output format speed
Compares HTML, Markdown, and SARIF generation overhead on the small fixture.

### `individual_tools` — Per-tool runtime
Benchmarks each standalone tool binary on the small fixture.

## Interpreting Results

Benchmark results are written to `target/criterion/`:
```
target/criterion/
├── cogent_check/
│   ├── e2e/small/
│   └── e2e/medium/
├── report_generation/
│   ├── html/small/
│   ├── markdown/small/
│   └── sarif/small/
└── individual_tools/
    ├── secrets/small_fixture/
    └── ...
```

Each report contains:
- **Mean** average runtime
- **Std Dev** variance across samples
- **Throughput** iterations per second

## CI Regression Detection

The `.github/workflows/quality.yml` runs benchmarks on PRs and compares against `main`.

A PR fails if any benchmark regresses by >10% from baseline.

### Updating baselines
Baseline results are stored as CI artifacts. To update after an intentional optimization:

1. Run benchmarks locally on `main`:
   ```bash
   cargo bench -p cogent-cli
   ```
2. Download the latest baseline artifact from CI
3. Compare with `cargo bench -p cogent-cli -- --baseline <name>`

## Profiling (Optional)

For deeper analysis, use:

```bash
# CPU profiling
cargo flamegraph -p cogent-cli -- check . --force

# Memory profiling
cargo dhat -p cogent-cli -- check . --force
```
