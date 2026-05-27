# Quality Standards — "Exceeding Standards" Definition

These are the strict quality targets that all projects using Cogent should meet. The tools themselves are held to these same standards.

## Metric Targets

| Metric | Target | Rationale |
|--------|--------|-----------|
| **CRAP Score** | < 15 per function | Functions with CRAP >15 are risky to maintain |
| **Cyclomatic Complexity** | < 5 per function | Lower complexity = easier to test and understand |
| **Test Coverage** | > 90% | Ensures most code paths are verified |
| **Documentation Coverage** | > 95% for public APIs | Well-documented code is easier to adopt |
| **Technical Debt Markers** | 0 (TODO/FIXME/HACK/XXX) | Zero tolerance for known issues |
| **Code Duplication** | 0 blocks > 3 lines | Duplication increases maintenance burden |
| **Clippy Warnings** | 0 | Clean code follows Rust best practices |
| **Test Coverage (Tools)** | > 90% for all crates | The tools themselves must be well-tested |

## Quality Gates (CI Enforcement)

PRs will be blocked if any of these conditions are met:
1. New technical debt markers introduced (TODO/FIXME/HACK/XXX)
2. Any function has CRAP score > 15
3. Test coverage drops by > 5%
4. Documentation coverage for public APIs < 95%
5. Any code duplication > 3 lines detected
6. Clippy warnings are present

## Severity Levels

| Level | Description | Action |
|-------|-------------|--------|
| **Critical** | CRAP > 30, Complexity > 15, Coverage < 50% | Block merge immediately |
| **High** | CRAP 15-30, Complexity 10-15, Coverage 50-80% | Require fixes before merge |
| **Medium** | CRAP 5-15, Complexity 5-10, Coverage 80-90% | Prompt improvement in PR |
| **Low** | CRAP <5, Complexity <5, Coverage >90% | Exceeds standards |

## Self-Audit Requirement

Every crate in this project:
1. Must have zero technical debt markers
2. Must be tested with >90% coverage
3. Must use its own tools to verify compliance
4. Must document all public APIs with >95% coverage

Run `cogent run . --format sarif` to self-audit.
