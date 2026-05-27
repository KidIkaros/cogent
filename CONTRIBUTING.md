# Contributing to Cogent

Thank you for your interest in contributing to Cogent! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites
- Rust (edition 2021, MSRV 1.75)
- Cargo
- Git

### Building
```bash
# Standard build
cargo build

# Build with release optimizations
cargo build --release

# If build path doesn't support exec permissions (e.g., FAT32)
CARGO_TARGET_DIR=/tmp/Cogent-build cargo build
```

### Testing
```bash
# Run all tests (recommended for most cases)
./scripts/test.sh

# Ultra-safe mode - one crate at a time (for low-memory systems)
./scripts/test.sh --safe

# Test specific crate
./scripts/test.sh -p crap-metric

# Standard cargo test (for single crate development)
cargo test -p cogent-common
```

### Linting and Formatting
```bash
# Check formatting
cargo fmt --check

# Apply formatting
cargo fmt

# Run clippy lints (we deny clippy::all)
cargo clippy -- -D warnings
```

## How to Contribute

### Reporting Bugs
- Use the [GitHub Issues](https://github.com/KidIkaros/cogent/issues) tracker
- Include steps to reproduce, expected behavior, and actual behavior
- Mention your OS, Rust version, and Cogent version

### Suggesting Enhancements
- Open an issue with the "enhancement" label
- Describe the feature and its use case
- Consider starting a discussion for major changes

### Pull Requests
1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and linting locally
5. Commit with clear, descriptive messages
6. Push to your fork (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### PR Guidelines
- Keep PRs focused on a single concern
- Update documentation if needed
- Add tests for new functionality
- Ensure all CI checks pass
- Link to relevant issues

## Code Style
- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Use meaningful variable and function names
- Add comments for non-obvious code
- Keep functions small and focused

## Adding a New Tool
1. Create a new crate in `crates/`
2. Use `ast-parse-ts` for multi-language support
3. Follow the existing CLI pattern (clap derive API)
4. Support JSON output via `serde_json::to_string_pretty`
5. Add integration tests in `tests/`
6. Update the README with usage examples

## Commit Messages
Follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:
```
feat: add new complexity threshold option
fix: correct false positive in coupling analysis
docs: update README with new examples
test: add integration tests for taint-scan
refactor: simplify error handling in cogent-cli
```

## License
By contributing, you agree that your contributions will be licensed under the project's dual license (Apache-2.0 OR OPL-1.1).

## Questions?
Feel free to open an issue with your question or reach out to the maintainers.
