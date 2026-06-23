# Cogent Roadmap

This roadmap describes where Cogent is headed. It is intentionally high-level and
subject to change — dates are targets, not commitments. For shipped changes see
[`CHANGELOG.md`](CHANGELOG.md); for the current health snapshot see
[`PROJECT_STATUS.md`](PROJECT_STATUS.md).

**Current release:** `v1.2.0` — 31 audit tools across 5 categories, self-hosting,
SARIF/JSON/NDJSON/HTML output, MCP server, Hermes agent skills.

Have an idea or a vote? Open a
[discussion](https://github.com/KidIkaros/cogent/discussions) or an
[issue](https://github.com/KidIkaros/cogent/issues).

---

## Guiding Principles

These don't change release to release — they are the lens we use to accept or
reject roadmap items:

1. **Zero-config first.** Every feature must work with sensible defaults before it
   gets a knob.
2. **Single binary, no runtime deps.** No JVM, no cloud token, no daemon required.
3. **Machine-first output.** Anything a human can see, an agent or CI job can parse
   (JSON / NDJSON / SARIF).
4. **Deterministic gates.** Same input, same exit code — always.
5. **Self-hosting.** Cogent must pass its own gate on every commit.

---

## Now — `v1.3` (next release)

Stabilization and polish of the existing surface.

- [ ] **Green CI on all three platforms.** Resolve the macOS broken-pipe e2e flake
      (`test_e2e_run_json_piped_to_head_exits_cleanly`) so Linux/macOS/Windows are
      all green without ignored tests.
- [ ] **Publish to crates.io.** Ship `cogent-cli` + engines so `cargo install cogent`
      works (guide already drafted in [`docs/PUBLISHING.md`](docs/PUBLISHING.md);
      pending the API token).
- [ ] **Project website.** A GitHub Pages landing site with install, tool catalog,
      and links into the docs.
- [ ] **Threshold calibration UX.** `cogent init` should explain *why* each
      threshold was chosen and make recalibration a single command rather than a
      manual `.quality.toml` edit.
- [ ] **Richer `cogent explain <tool>`.** Before/after examples inline in the
      terminal for every tool, not just the common ones.

## Next — `v1.4`

Broaden language coverage and sharpen the security tools.

- [ ] **First-class language packs.** Promote Go, Java, C/C++, PHP, Ruby, and C#
      from "supported" to "tuned" — language-specific default thresholds and rules.
- [ ] **Incremental analysis.** Analyze only changed files in a diff (`cogent check
      --since <ref>`) for sub-second PR gates on large repos.
- [ ] **SAST rule packs.** Versioned, opt-in rule bundles (OWASP Top 10, CWE Top 25)
      with provenance so teams can pin a ruleset.
- [ ] **Baseline & suppression workflow.** First-class "accept this finding"
      tracking with expiry, replacing ad-hoc `secrets_exclude`-style lists.
- [ ] **Config presets.** `cogent init --preset strict|balanced|lenient` for
      one-line policy selection.

## Later — `v2.0`

Bigger bets that may involve breaking changes.

- [ ] **Stable plugin API.** A documented contract so third parties can ship their
      own engines that slot into `cogent check` and the scoring model.
- [ ] **Historical trend dashboard.** Self-hosted, static HTML dashboard built from
      `.cogent-history/` showing score/finding trends over time.
- [ ] **Org-level policy server (optional).** Centralized policy distribution and
      aggregated reporting for teams — fully optional, never required for the CLI.
- [ ] **Autofix expansion.** Grow `cogent remediate` from formatting/import fixes
      to safe, reviewable security and quality fixes with diff previews.
- [ ] **Editor integrations.** Maturing the VS Code experience
      ([`docs/vscode.md`](docs/vscode.md)) and adding LSP-style inline diagnostics.

---

## Known Limitations (tracked, not yet scheduled)

| Area | Limitation | Notes |
|------|------------|-------|
| `mutate` | Requires the test suite to pass first; ignored tests are skipped | By design — mutation score is meaningless on a red suite |
| `outdated` | Needs `cargo-outdated` installed (otherwise skipped) | External dependency; surfaced as "skipped" not "failed" |
| macOS e2e | One broken-pipe e2e test is flaky | Tracked for `v1.3` |

See [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md) for the full list.

---

## Recently Shipped

A few highlights — full history in [`CHANGELOG.md`](CHANGELOG.md).

- [x] Cache lifecycle: TTL pruning + size cap + `--clear-cache` (`v1.2.0`)
- [x] OpenTelemetry tracing with OTLP export (`v1.2.0`)
- [x] End-to-end test suite across all fixture languages (`v1.2.0`)
- [x] Eliminated all `unsafe` blocks (`v1.2.0`)
- [x] Expanded from 10 → 31 tools across 5 categories (`v1.1.0`)
- [x] HTML audit report with A–F health grade and drill-downs (`v1.1.0`)
- [x] MCP server (`cogent-server`) for Claude Desktop / Cursor / Windsurf (`v1.1.0`)
- [x] JSON schemas + integration tests for all tools (`v1.0.0`)

---

*Roadmap last updated: 2026-06-23.*
