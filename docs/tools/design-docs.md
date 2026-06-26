# design-docs

Check whether your project has required design documentation pillars.

## What it measures

Scans your repo for the presence of key documentation artifacts:

- **README**: Project overview, installation, usage
- **CHANGELOG**: Version history and release notes
- **Architecture/DESIGN**: System design decisions and tradeoffs
- **CONTRIBUTING**: How to contribute to the project
- **CODE_OF_CONDUCT**: Community guidelines (for open source projects)
- **SECURITY**: Security policy and vulnerability reporting
- **LICENSE**: License declaration

## Why it matters

Projects without design documentation become unmaintainable. New contributors can't understand the "why" behind decisions, and architectural drift sets in. Good docs are the difference between a maintainable codebase and a rewrite waiting to happen.

## Output

```
design-docs score: 57% (4/7 pillars present)

Present:
  ✓ README.md
  ✓ CHANGELOG.md
  ✓ CONTRIBUTING.md
  ✓ LICENSE

Missing:
  ✗ docs/DESIGN.md or docs/architecture.md
  ✗ CODE_OF_CONDUCT.md
  ✗ SECURITY.md

Recommendation: Create docs/DESIGN.md to document your system architecture.
```

## Threshold

`.quality.toml`:
```toml
[design-docs]
min_pillars = 5  # Require at least 5 of 7 pillars
```

## Common fixes

1. **Create an architecture doc** (`docs/DESIGN.md`):
   ```markdown
   # System Architecture

   ## High-Level Overview
   Cogent is a Rust-based static analysis toolkit composed of 34 crates.

   ## Key Components
   - `cogent-cli`: Unified entry point
   - `cogent-engine`: Orchestration layer
   - Individual tool crates: Each is an independent analysis engine

   ## Data Flow
   CLI invocation → config load → tool dispatch → JSON output → score calculation

   ## Design Decisions
   - **Rust**: Zero runtime deps, fast startup, safety guarantees
   - **Workspace pattern**: Each tool is its own crate, easy to extend
   - **JSON-first output**: Agent-friendly, CI/CD integratable
   ```

2. **Add a security policy** (`SECURITY.md`):
   ```markdown
   # Security Policy

   ## Reporting Vulnerabilities
   Please report security issues privately to security@kidikaros.com

   ## Supported Versions
   - v1.2.x: Security patches
   - v1.1.x: Critical security patches only
   - v1.0.x: End of life
   ```

3. **Add a code of conduct** (`CODE_OF_CONDUCT.md`):
   ```markdown
   # Contributor Covenant Code of Conduct

   ## Our Pledge
   We pledge to make participation in our community a harassment-free experience.
   ```

## File location flexibility

Cogent searches in multiple locations:

| Pillar | Valid paths |
|--------|-------------|
| Architecture | `DESIGN.md`, `ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/architecture.md` |
| Changelog | `CHANGELOG.md`, `CHANGES.md`, `HISTORY.md` |
| Contributing | `CONTRIBUTING.md`, `CONTRIBUTING.rst`, `docs/CONTRIBUTING.md` |
| Security | `SECURITY.md`, `SECURITY.md`, `docs/SECURITY.md` |

## False positives

- **Internal/proprietary projects**: May not need CODE_OF_CONDUCT or public security policy

Add to `.cogent-exceptions.yaml`:
```yaml
design-docs:
  optional_pillars:
    - "CODE_OF_CONDUCT.md"
    - "SECURITY.md"
```