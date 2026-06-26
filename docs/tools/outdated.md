# outdated

Check for dependencies that are significantly behind the latest version.

## What it measures

Scans your dependency lockfiles and identifies:

- **Direct dependencies** >= 1 major version behind latest
- **Transitive dependencies** with known CVEs (reported by vulnscan)
- **Stale dependencies** not updated in the last 12 months

## Why it matters

Stale dependencies miss:
- Security patches (CVEs)
- Performance improvements
- Bug fixes
- New features that could simplify your code

A dependency 2+ major versions behind is a technical debt time bomb.

## Output

```
outdated status: 3 direct deps behind

Direct dependencies behind latest:
  ┌─────────────────┬─────────┬─────────┬──────────┬─────────────────┐
  │ Package         │ Current │ Latest  │ Behind   │ Reason          │
  ├─────────────────┼─────────┼─────────┼──────────┼─────────────────┤
  │ serde           │ 1.0.136 │ 1.0.152 │ -1 minor │ bugfixes        │
  │ tokio           │ 1.24.0  │ 1.38.0  │ -2 major │ perf + features │
  │ reqwest         │ 0.11.10 │ 0.12.5  │ -2 major │ bugfixes        │
  └─────────────────┴─────────┴─────────┴──────────┴─────────────────┘

Recommendation: Update tokio and reqwest (major version bumps)

Stale dependencies (>12 months since last update):
  - chrono: 14 months since last update (consider alternative)
```

## Threshold

`.quality.toml`:
```toml
[outdated]
max_behind_major = 0  # Fail if any direct dep is >1 major version behind
max_stale_months = 12
```

## Common fixes

1. **Update to latest compatible version** (Rust):
   ```bash
   # Check what needs updating
   cargo outdated

   # Update specific crate
   cargo update -p tokio

   # Update all dependencies
   cargo update

   # Re-run tests
   cargo test
   ```

2. **Handle major version bumps**:
   ```bash
   # Check breaking changes
   cargo install cargo-outdated
   cargo outdated --format json | jq '.[] | select(.latest != .compat)'

   # Read migration guide
   # e.g., https://tokio.rs/blog/2024-04-tokio-1.38
   ```

3. **Replace unmaintained crates**:
   ```toml
   # Before (unmaintained)
   [dependencies]
   chrono = "0.4"

   # After (actively maintained)
   [dependencies]
   time = "0.3"  # Features overlap, active development
   ```

## Language-specific commands

| Language | Command | Lockfile |
|----------|---------|----------|
| Rust | `cargo outdated` | `Cargo.lock` |
| Python | `pip list --outdated` | `requirements.lock` |
| JS/TS | `npm outdated` / `yarn outdated` | `package-lock.json` / `yarn.lock` |
| Go | `go list -u -m all` | `go.sum` |

## Integration with vulnscan

The `outdated` check is complementary to `vulnscan`:

- `vulnscan`: Checks for known CVEs in current versions
- `outdated`: Proactively identifies versions likely to have issues

Run together in CI:
```bash
cogent vulnscan . && cogent outdated .
```

## False positives

- **Pinned versions**: Sometimes you intentionally stay on an old version for compatibility
- **Internal crates**: Your own packages don't need updates

Add to `.cogent-exceptions.yaml`:
```yaml
outdated:
  ignore:
    - "my-org/internal-crate"
    - "legacy-pkg"  # Intentionally pinned to old version
```