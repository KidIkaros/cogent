# Common development tasks for the Cogent workspace.
# Install `just` via `cargo install just`.

# Default recipe — show available commands
[private]
default:
    @just --list

# Build the entire workspace
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run linting (fmt + clippy)
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings

# Auto-fix formatting and clippy where possible
fix:
    cargo fmt --all
    cargo clippy --workspace --fix --allow-dirty --allow-staged

# Run all unit tests (uses batched script to avoid OOM)
test:
    ./scripts/test.sh

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}} --lib

# Generate code coverage report with tarpaulin
coverage:
    cargo tarpaulin --lib --workspace --out Html --output-dir target/coverage
    @echo "Coverage report: target/coverage/tarpaulin-report.html"

# Run coverage with threshold enforcement (CI behavior)
coverage-ci:
    cargo tarpaulin --lib --workspace --out Xml --fail-under 70

# Run security audit
audit:
    cargo audit

# Run the full quality pipeline (lint + build + test)
check:
    just lint
    just build
    just test

# Run cogent on the workspace itself (dogfood)
dogfood:
    cargo run --bin cogent -- run . --format text

# Clean build artifacts
clean:
    cargo clean
    rm -rf target/coverage

# Install development dependencies (cargo-audit, cargo-tarpaulin)
install-tools:
    cargo install --locked cargo-audit
    cargo install --locked cargo-tarpaulin
