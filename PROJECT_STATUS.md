# Project Status — Cogent

## Current Phase
**Stable v1.2.0** — Production-ready, GitHub public at `github.com/KidIkaros/cogent`

## 31 Checks Across 5 Categories

**Quality (16):** crap · debt · doccov · riskmap · dupfind · coupling · complexity · linelen · halstead · deadcode · cohesion · comments · propcov · typecov · fuzz · mutate

**Security (7):** secrets · taint · errhandle · vulnscan · sast · crypto · access-control

**Compliance (2):** licenses · sbom

**Supply Chain (2):** supply-chain · outdated

**Operations (4):** observability · test-quality · design-docs · debuggability

| Check | Binary | Status |
|-------|--------|--------|
| Access Control | `access-control` | ✓ |
| Code Debt | `debt` | ✓ |
| Cohesion | `cohesion` | ✓ |
| Comment Ratio | `comments` | ✓ |
| Complexity | — (via `cogent check`) | ✓ |
| Coupling | `coupling` | ✓ |
| CRAP Metric | `crap` | ✓ |
| Crypto Check | `cryptocheck` | ✓ |
| Dead Code | `deadcode` | ✓ |
| Doc Coverage | `doccov` | ✓ |
| Duplication | `dupfind` | ✓ |
| Error Handling | `errhandle` | ✓ |
| Fuzz Surface | `fuzz` | ✓ |
| Halstead | `halstead` | ✓ |
| Licenses | `licenses` | ✓ |
| Line Length | `linelen` | ✓ |
| Mutation Test | `mutate` | ✓ |
| Property Coverage | `propcov` | ✓ |
| Risk Map | `riskmap` | ✓ |
| SAST | `sast` | ✓ |
| SBOM | `sbom` | ✓ |
| Secrets | `secrets` | ✓ |
| Supply Chain | `supply-chain` | ✓ |
| Taint Scan | `taint` | ✓ |
| Type Coverage | `typecov` | ✓ |
| Vuln Scan | `vulnscan` | ✓ |
| Outdated | `outdated` | ✓ |
| Observability | `observability` | ✓ |
| Test Quality | `test-quality` | ✓ |
| Design Docs | `design-docs` | ✓ |
| Debuggability | `debuggability` | ✓ |

## Recent Work
- Rebrand from `quality-tools` → `cogent` (May 2026)
- Unified CLI under single `cogent` binary entry point
- Exported Hermes Agent skills into repo for AI integration
- Added `cogent init/check/watch/install-hooks/report/diff` high-level commands
- MCP server (`cogent-server`) for GUI client compatibility (Claude Desktop, Cursor, Windsurf)

## Production Readiness (Completed)

### Phase 1: Testing & Schema Coverage
- **JSON Schemas**: All 31 tools have validated JSON schemas in `schemas/`
- **Integration Tests**: 29 tests validating all tool binaries run and produce valid output
- **Schema Validation**: CI validates tool output against schemas

### Phase 2: Distribution & Packaging
- **GitHub Releases**: Multi-platform workflow (Linux x86_64, macOS x86_64/ARM64, Windows x86_64)
- **Docker**: Multi-stage `Dockerfile` with all 27 binaries
- **Homebrew**: Formula at `Formula/cogent.rb`
- **Shell Completions**: `cogent completions <shell>` generates bash/zsh/fish/powershell/elvish scripts

### Phase 3: Documentation
- **Tool Docs**: All 31 tools documented in `docs/tools/<tool>.md`
- **README**: Installation, quick start, CI/CD integration sections added
- **Reporting Guide**: `docs/reporting.md` covers all output formats

### Phase 4: Security Hardening
- **Dependency Audit**: `cargo audit` — 0 vulnerabilities in 267 dependencies
- **License Audit**: `cargo deny` configured with `deny.toml` — all licenses approved
- **Unsafe Code Audit**: 2 justified `unsafe` blocks (libc::isatty, libc::kill for timeouts)
- Expanded from 10 to 26 checks: added access-control, supply-chain, sast, crypto, secrets, licenses, sbom, deadcode, linelen, complexity, typecov, comments, cohesion, errhandle, vulnscan, halstead
- HTML audit report with sidebar navigation, health score (A–F), SVG gauge, inline offenders
- Watch mode `--full` flag + cycle diff; `--verbose` flag on check; health score in summary box
- Self-audit clean: 31/31 checks pass (`cogent check .` scores 100/100)

## Known Limitations
| Tool | Limitation |
|------|------------|
| `mutate` | Requires tests to pass — ignores ignored tests by default |
| `outdated` | Requires `cargo-outdated` to be installed (skipped otherwise) |

## Roadmap
- [x] Replace `crap` icon table truncation with proper unicode width handling
- [x] Fix taint-secret detection (log-leak and Secret:: RHS now detected)
- [x] Add JSON schema validation for all tool outputs
- [x] Fix `load_config_thresholds` — all `.quality.toml` keys now parsed
- [x] Add access-control and supply-chain analyzers
- [x] Self-audit passes at 100/100
- [x] Expand JSON schema coverage to all 31 tools (29 of 29 done)
- [x] Add integration tests for all tool binaries (29 of 29 done)
- [ ] Publish crates to crates.io — guide at `docs/PUBLISHING.md` (pending API token)

## Getting Started
```bash
cargo build --release
cogent init                        # detect ecosystem, write .quality.toml
cogent check .                     # self-audit (31 checks, weighted scoring)
bash scripts/test.sh               # full test suite
```

## Repo Structure
```
crates/          34 crates: 28 tool engines + engine + config + report + fix + protocol + common + CLI + server + fixtures + ast-parse-ts
hermes/          Hermes Agent skills (AI integration)
docs/            Guides & integration notes
schemas/         JSON schemas for output validation (29 of 29 complete)
scripts/         CI/build helpers
```

---

*Last updated: 2026-06-05 | Branch: master*
