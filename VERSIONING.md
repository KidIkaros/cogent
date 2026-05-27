# Versioning & Stability Policy

Cogent follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) (SemVer) for public API surface and CLI contracts.

---

## What Gets Versioned

| Component | Version Scope | Breaking Change Criteria |
|-----------|---------------|--------------------------|
| `cogent` CLI binary (command syntax, flags, exit codes) | **MAJOR** | Removing/renaming subcommands, changing exit code semantics, removing flag |
| JSON/NDJSON/SARIF output schemas (files in `schemas/`) | **MAJOR** | Removing/renaming fields, changing types, altering nested structure |
| Tool algorithm results (score computations) | **MINOR** | Score formula changes, threshold defaults adjustments |
| Language detection heuristics | **MINOR** | Support for new language parsers |
| Hermes Agent skill interface | **MINOR** | Skill ID/name changes, output format changes |
| Internal crate structure (private modules) | **PATCH** | Private refactors, optimizations, bug fixes not affecting outputs |

**Bottom line:** If a downstream script or agent depends on Cogent output, breaking changes require a MAJOR version bump.

---

## Release Cadence

- **Patch releases** (x.y.Z) — bug fixes, test improvements, doc updates, CI tweaks
- **Minor releases** (x.Y.0) — new tools, new output formats, new language support, threshold tuning
- **Major releases** (X.0.0) — CLI restructuring, output schema changes, breaking compatibility

We aim for **at least one minor release per quarter** if features accumulate; patches as needed.

---

## Deprecation Policy

When a feature must be removed or changed in a breaking way:

1. **Announce** in CHANGELOG under "Deprecated" with clear migration path
2. **Emit runtime warning** when deprecated feature is used (one full minor version before removal)
3. **Remove** in the next MAJOR release

Example:
```text
[1.2.0] — Deprecated `--legacy-format` flag (use `--format json` instead). Warning printed when used.
[2.0.0] — Removed `--legacy-format` flag.
```

---

## Stability Tiers

| Tier | Guarantee | Examples |
|------|-----------|----------|
| **Stable** | Backward-compatible within major version | CLI syntax, exit codes, output schemas (published in `schemas/`) |
| **Beta** | May change before first major version | Hermes skill interfaces (currently stable as of v1.0) |
| **Experimental** | Subject to change without notice | `--dev` flags, internal crate APIs not exported in public docs |

Everything shipping in v1.0.0 is **Stable**.

---

## Schema Evolution

JSON/NDJSON/SARIF schemas are versioned inline via `$schema` and `$id` fields. Each tool's output includes a `version` field (SemVer) so consumers can handle multiple versions programmatically.

When we need to evolve a schema:
- **Additive changes** (new optional fields) → MINOR bump
- **Required field additions or type changes** → MAJOR bump

See `schemas/README.md` for schema usage and validation instructions.

---

## Breaking Changes Log

| Version | Breaking Change | Migration Guidance |
|---------|-----------------|--------------------|
| 1.0.0 | `quality` → `cogent` CLI rename; `.quality-baseline.sarif` → `.cogent-baseline.sarif` | See `UPGRADE.md` |
| Future (2.0.0) | TBD — will be announced at least one minor version in advance | Watch CHANGELOG and releases |

---

## Questions?

Open an issue on GitHub or consult `PROJECT_STATUS.md` for known limitations and roadmap.
