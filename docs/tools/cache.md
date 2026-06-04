# Incremental Cache

## Purpose

Speeds up repeated `cogent check` runs by caching results. When source files and configuration haven't changed, cached results are returned instantly instead of re-running checks.

## How it works

1. A **workspace fingerprint** is computed from: Cogent version + `.quality.toml` content + sorted file modification times
2. For each check, the fingerprint + check name produces a cache key (SHA-256)
3. If a matching cached result exists and hasn't expired, it's returned without re-running the check
4. Fresh results are stored for future reuse

## CLI commands

### `cogent cache clear`

Delete all cached check results:

```bash
cogent cache clear
```

### `cogent cache status`

Show cache size, entry count, and age:

```bash
cogent cache status
```

Example output:

```
Cached checks: 18
Total size:    245.3 KB
Oldest entry:  2d ago
Newest entry:  5m ago
```

### Cache flags on `cogent check`

| Flag | Description |
|------|-------------|
| `--no-cache` | Disable caching for this run; re-run all checks from scratch |
| `--clear-cache` | Clear the cache before running checks |

```bash
# Run checks without using cache
cogent check . --no-cache

# Clear cache then run fresh
cogent check . --clear-cache
```

## Configuration

Cache behavior is controlled via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `COGENT_CACHE_TTL_SECS` | `604800` (7 days) | Maximum age of cache entries in seconds. Expired entries are deleted on access. |
| `COGENT_CACHE_MAX_BYTES` | `104857600` (100 MB) | Maximum total cache size in bytes. Oldest entries are evicted when the limit is exceeded. |

```bash
# Use a 1-day TTL
export COGENT_CACHE_TTL_SECS=86400

# Limit cache to 50 MB
export COGENT_CACHE_MAX_BYTES=52428800

# Disable caching via env var (equivalent to --no-cache)
# Use --no-cache flag instead — there is no env var for this
```

## Cache storage

Cache entries are stored in `.cogent-cache/<check_name>/<hash>.json`. This directory is:

- Excluded from source file fingerprinting
- Added to `.gitignore` by default
- Automatically managed (stale entries pruned, expired entries deleted, size capped)

## Stale entry cleanup

When `cogent check` runs, cache subdirectories for checks that are no longer in the active run are automatically removed. This prevents stale entries from accumulating when checks are renamed or removed.

## Text output

In text format, cached checks are marked with a `(cached)` tag:

```
  ▶ Running 22 checks in parallel...
  ✓ debt             0 markers found (cached)
  ✓ doc_coverage     95.2% coverage (cached)
  ✓ secrets          0 findings
```

## Exit codes

Same as `cogent check` — caching doesn't affect exit codes.

## Examples

```bash
# First run — all checks execute, results cached
cogent check .
# 22 checks in 8.5s

# Second run — most checks served from cache
cogent check .
# 22 checks in 0.3s (20 cached)

# After editing a file — only affected checks re-run
echo "// new code" >> src/main.rs
cogent check .
# 22 checks in 3.1s (18 cached)

# Force fresh run
cogent check . --no-cache
# 22 checks in 8.5s

# Check cache status
cogent cache status
# Cached checks: 22
# Total size:    128.5 KB
# Oldest entry:  1d ago
# Newest entry:  10s ago

# Clear cache
cogent cache clear
# ✓ Cache cleared.

# CI: always run fresh
cogent check . --ci --no-cache
```

## Performance

| Scenario | Without cache | With cache |
|----------|--------------|------------|
| No changes | 6–12s | 0.2–0.5s |
| Single file changed | 6–12s | 2–4s |
| Config changed | 6–12s | 6–12s |

Cache hit rate depends on project size and change frequency. Typical developer workflows (edit → check → edit) benefit most from caching.

## See also

- `cogent check` — run all quality checks
- `cogent watch .` — continuous monitoring mode
- `cogent audit` — full security/quality/compliance audit
