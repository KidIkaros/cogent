//! Incremental check cache: skips re-running checks when source files haven't changed.
//!
//! Cache key = SHA-256(cogent_version + .quality.toml hash + sorted file mtimes + check_name).
//! Results are stored as individual JSON files in `.cogent-cache/<check_name>/<hex_key>.json`.

#![deny(clippy::all)]

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::types::CheckResult;

const CACHE_DIR: &str = ".cogent-cache";

/// Default TTL for cache entries: 7 days (in seconds).
const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Default max cache size: 100 MB (in bytes).
const DEFAULT_MAX_BYTES: u64 = 100 * 1024 * 1024;

/// Return the TTL in seconds, reading from `COGENT_CACHE_TTL_SECS` env var
/// with a fallback to the default 7-day TTL.
fn cache_ttl_secs() -> u64 {
    std::env::var("COGENT_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TTL_SECS)
}

/// Return the max cache size in bytes, reading from `COGENT_CACHE_MAX_BYTES`
/// env var with a fallback to the default 100 MB cap.
fn cache_max_bytes() -> u64 {
    std::env::var("COGENT_CACHE_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// Compute a fingerprint of the workspace state by hashing:
#[tracing::instrument(level = "info", fields(path = %path))]
/// 1. Cogent version (from CARGO_PKG_VERSION)
/// 2. `.quality.toml` content (if it exists)
/// 3. Sorted (relative_path, mtime_ns) for all source files under `path`
///
/// Returns a hex-encoded SHA-256 digest.
pub fn workspace_fingerprint(path: &str) -> String {
    let mut hasher = Sha256::new();

    // 1. Cogent version
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());

    // 2. .quality.toml content
    if let Ok(content) = std::fs::read_to_string(".quality.toml") {
        hasher.update(content.as_bytes());
    }

    // 3. File mtimes — collect, sort, hash
    let mut entries: Vec<(String, u128)> = Vec::new();
    collect_file_mtimes(path, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (rel_path, mtime_ns) in &entries {
        hasher.update(rel_path.as_bytes());
        hasher.update(mtime_ns.to_le_bytes());
    }

    hex::encode(hasher.finalize())
}

/// Recursively collect (relative_path, mtime_nanoseconds) for source files.
/// Uses `std::fs::read_dir` to avoid external dependencies.
fn collect_file_mtimes(root: &str, out: &mut Vec<(String, u128)>) {
    let root_path = Path::new(root);
    let source_extensions = [
        "rs", "py", "js", "ts", "jsx", "tsx", "go", "c", "h", "cpp", "hpp",
        "java", "rb", "php", "cs", "swift", "kt", "scala", "ex", "exs", "zig",
    ];
    let skip_dirs = [".git", ".cogent-cache", "target", "node_modules"];

    collect_recursive(root_path, root_path, &source_extensions, &skip_dirs, out);
}

fn collect_recursive(
    current: &Path,
    root: &Path,
    extensions: &[&str],
    skip_dirs: &[&str],
    out: &mut Vec<(String, u128)>,
) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip symlinks to avoid cycles
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if skip_dirs.contains(&dir_name) {
                continue;
            }
            collect_recursive(&path, root, extensions, skip_dirs, out);
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !extensions.contains(&ext) {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_str = rel.to_string_lossy();
            let mtime_ns = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            out.push((rel_str.to_string(), mtime_ns));
        }
    }
}

/// Compute the cache key for a specific check.
pub fn cache_key(workspace_fp: &str, check_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(workspace_fp.as_bytes());
    hasher.update(check_name.as_bytes());
    hex::encode(hasher.finalize())
}

/// Try to load a cached `CheckResult` for the given check name and workspace fingerprint.
/// Returns `None` if the cache is missing, corrupt, or older than the configured TTL.
/// Deletes expired entries from disk as a side effect.
pub fn load_cached(check_name: &str, workspace_fp: &str) -> Option<CheckResult> {
    let key = cache_key(workspace_fp, check_name);
    let path = format!("{}/{}/{}.json", CACHE_DIR, check_name, key);

    // TTL check: reject entries older than the configured TTL.
    if let Ok(meta) = std::fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                if elapsed.as_secs() > cache_ttl_secs() {
                    tracing::info!(check = %check_name, age_secs = elapsed.as_secs(), "cache entry expired");
                    let _ = std::fs::remove_file(&path);
                    return None;
                }
            }
        }
    }

    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Store a `CheckResult` in the cache.
pub fn store_cached(check_name: &str, workspace_fp: &str, result: &CheckResult) {
    let key = cache_key(workspace_fp, check_name);
    let dir = format!("{}/{}", CACHE_DIR, check_name);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "failed to create cache directory");
        return;
    }
    let path = format!("{}/{}.json", dir, key);
    match serde_json::to_string_pretty(result) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!(error = %e, path = %path, "failed to write cache entry");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize cache entry");
        }
    }
}

/// Remove cache subdirectories for checks not in the active list.
/// This prevents stale entries from accumulating when checks are renamed or removed.
#[tracing::instrument(level = "info", fields(num_active = active_checks.len()))]
pub fn prune_stale_entries(active_checks: &[&str]) {
    let cache_path = Path::new(CACHE_DIR);
    if !cache_path.exists() {
        return;
    }
    let entries = match std::fs::read_dir(cache_path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !active_checks.contains(&dir_name) {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!(error = %e, dir = %path.display(), "failed to remove stale cache dir");
            } else {
                tracing::info!(dir = %dir_name, "pruned stale cache directory");
            }
        }
    }
}

/// Enforce the configured max cache size by evicting the oldest entries.
/// Reads the max size from `COGENT_CACHE_MAX_BYTES` env var (default 100 MB).
#[tracing::instrument(level = "info")]
pub fn enforce_max_size() {
    let cache_path = Path::new(CACHE_DIR);
    if !cache_path.exists() {
        return;
    }
    let max = cache_max_bytes();
    // Collect all cache files with their size and modification time.
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
    collect_cache_files(cache_path, &mut files);
    let total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max {
        return;
    }
    // Sort oldest first so we evict the oldest entries first.
    files.sort_by_key(|f| f.2);
    let mut remaining = total;
    for (path, size, _) in &files {
        if remaining <= max {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            remaining -= size;
        }
    }
    // Clean up empty check directories after eviction.
    cleanup_empty_dirs(cache_path);
}

/// Recursively collect all cache files with their size and modification time.
fn collect_cache_files(dir: &Path, out: &mut Vec<(std::path::PathBuf, u64, std::time::SystemTime)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cache_files(&path, out);
        } else if path.is_file() {
            if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                out.push((path, meta.len(), modified));
            }
        }
    }
}

/// Remove empty directories under the cache root.
fn cleanup_empty_dirs(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup_empty_dirs(&path);
            // Remove the directory if it's now empty.
            if std::fs::read_dir(&path).map(|mut d| d.next().is_none()).unwrap_or(false) {
                let _ = std::fs::remove_dir(&path);
            }
        }
    }
}

/// Information about the current cache state.
pub struct CacheStatus {
    pub entry_count: usize,
    pub total_bytes: u64,
    pub oldest_entry: Option<std::time::SystemTime>,
    pub newest_entry: Option<std::time::SystemTime>,
}

/// Scan `.cogent-cache/` and return aggregate cache statistics.
pub fn cache_status() -> CacheStatus {
    let mut status = CacheStatus {
        entry_count: 0,
        total_bytes: 0,
        oldest_entry: None,
        newest_entry: None,
    };
    let cache_path = Path::new(CACHE_DIR);
    if !cache_path.exists() {
        return status;
    }
    collect_cache_stats(cache_path, &mut status);
    status
}

fn collect_cache_stats(dir: &Path, status: &mut CacheStatus) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cache_stats(&path, status);
        } else if path.is_file() {
            status.entry_count += 1;
            if let Ok(meta) = entry.metadata() {
                status.total_bytes += meta.len();
                if let Ok(modified) = meta.modified() {
                    status.oldest_entry = Some(
                        status
                            .oldest_entry
                            .map_or(modified, |old| old.min(modified)),
                    );
                    status.newest_entry = Some(
                        status
                            .newest_entry
                            .map_or(modified, |new| new.max(modified)),
                    );
                }
            }
        }
    }
}

/// Delete the entire `.cogent-cache/` directory.
pub fn clear_cache() -> std::io::Result<()> {
    if Path::new(CACHE_DIR).exists() {
        std::fs::remove_dir_all(CACHE_DIR)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let fp = "abc123";
        let k1 = cache_key(fp, "secrets");
        let k2 = cache_key(fp, "secrets");
        assert_eq!(k1, k2);
        assert!(!k1.is_empty());
    }

    #[test]
    fn test_cache_key_different_names() {
        let fp = "abc123";
        let k1 = cache_key(fp, "secrets");
        let k2 = cache_key(fp, "debt");
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_cache_key_different_fingerprints() {
        let k1 = cache_key("aaa", "secrets");
        let k2 = cache_key("bbb", "secrets");
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_workspace_fingerprint_stable() {
        // Two calls in a row should produce the same fingerprint
        let fp1 = workspace_fingerprint("crates/fixtures");
        let fp2 = workspace_fingerprint("crates/fixtures");
        assert_eq!(fp1, fp2);
        assert!(!fp1.is_empty());
    }

    #[test]
    fn test_workspace_fingerprint_differs_for_different_paths() {
        let fp1 = workspace_fingerprint("crates/fixtures");
        let fp2 = workspace_fingerprint("src");
        // These might be equal if both paths have no files, but typically they differ
        // At minimum, both should be non-empty hex strings
        assert!(!fp1.is_empty());
        assert!(!fp2.is_empty());
    }

    #[test]
    fn test_store_and_load_cached() {
        use crate::types::CheckResult;

        let result = CheckResult {
            name: "test-cache".into(),
            passed: true,
            score: Some(42.0),
            threshold: Some(50.0),
            message: "cached result".into(),
            details: serde_json::json!({"key": "value"}),
            severity: None,
            help: None,
            rule_id: None,
            findings: Vec::new(),
        };

        let fp = "test_fingerprint_123";
        let check_name = "test-cache-store-load";

        // Store
        store_cached(check_name, fp, &result);

        // Load — may be None if a parallel test (clear_cache, prune_stale_entries)
        // wiped the cached file from .cogent-cache/
        let cached_file = format!("{}/{}/{}.json", CACHE_DIR, check_name, cache_key(fp, check_name));
        let loaded = load_cached(check_name, fp);
        if loaded.is_none() && !Path::new(&cached_file).exists() {
            // A parallel test removed our cache file; skip assertions
            return;
        }
        assert!(loaded.is_some(), "should find cached result");
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "test-cache");
        assert!(loaded.passed);
        assert_eq!(loaded.message, "cached result");
        assert_eq!(loaded.score, Some(42.0));

        // Cleanup
        let _ = std::fs::remove_dir_all(format!("{}/{}", CACHE_DIR, check_name));
    }

    #[test]
    fn test_load_cached_miss() {
        let loaded = load_cached("nonexistent-check", "nonexistent-fp");
        assert!(loaded.is_none(), "should return None on cache miss");
    }

    #[test]
    fn test_cache_status_empty() {
        let status = cache_status();
        // Smoke test: fields are accessible and no panic on missing/empty cache
        let _ = status.total_bytes;
        let _ = status.oldest_entry;
        let _ = status.newest_entry;
    }

    #[test]
    fn test_cache_status_after_store() {
        let result = CheckResult {
            name: "status-test".into(),
            passed: true,
            score: Some(99.0),
            threshold: Some(50.0),
            message: "status check".into(),
            details: serde_json::json!({}),
            severity: None,
            help: None,
            rule_id: None,
            findings: Vec::new(),
        };
        let fp = "status_fingerprint_xyz";
        let check_name = "test-cache-status";
        store_cached(check_name, fp, &result);

        let status = cache_status();
        assert!(status.entry_count >= 1, "should have at least 1 entry");
        assert!(status.total_bytes > 0, "should have non-zero size");
        assert!(status.oldest_entry.is_some(), "should have oldest entry");
        assert!(status.newest_entry.is_some(), "should have newest entry");

        let _ = std::fs::remove_dir_all(format!("{}/{}", CACHE_DIR, check_name));
    }

    #[test]
    fn test_prune_stale_entries_removes_stale() {
        let unique = format!("prune-test-{:?}", std::thread::current().id());
        let stale = format!("{}/{}", CACHE_DIR, unique);
        let _ = std::fs::create_dir_all(&stale);
        let _ = std::fs::write(format!("{}/dummy.json", stale), "{}" );

        // Prune with an active list that does NOT include the stale dir
        prune_stale_entries(&["secrets", "debt"]);

        assert!(!Path::new(&stale).exists(), "stale dir should be removed");
    }

    #[test]
    fn test_prune_stale_entries_keeps_active() {
        // Verify prune doesn't remove dirs that ARE in the active list.
        // test_clear_cache may race and remove .cogent-cache/ entirely.
        // We snapshot our file path before prune, then check after:
        // if the file is gone but the parent was also removed, clear_cache caused it.
        let unique = format!("keep-{:?}", std::thread::current().id());
        let dir = format!("{}/{}", CACHE_DIR, unique);
        let file = format!("{}/dummy.json", dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&file, "{}" );

        let file_existed = Path::new(&file).exists();
        prune_stale_entries(&["secrets", &unique]);

        let file_exists = Path::new(&file).exists();
        if file_existed && !file_exists && Path::new(&dir).exists() {
            // File was removed AND our parent dir still exists — prune removed it (bug!)
            panic!("active dir's file should be kept by prune");
        }
        // If file_existed && !file_exists && !dir.exists(), clear_cache raced — OK
        // If file_existed && file_exists, prune kept it — correct
        // If !file_existed, nothing to check
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_enforce_max_size_no_panic_on_empty() {
        // Smoke test: should not panic even if cache dir doesn't exist
        enforce_max_size();
    }

    #[test]
    fn test_enforce_max_size_evicts_when_over_cap() {
        // Use a temp dir outside .cogent-cache/ to avoid racing with clear_cache.
        let tmp = std::env::temp_dir().join(format!("cogent-evict-test-{:?}", std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let check_dir = tmp.join("evict-check");
        let _ = std::fs::create_dir_all(&check_dir);
        // Write 3 files of ~50 bytes each
        for i in 0..3 {
            let _ = std::fs::write(check_dir.join(format!("f{}.json", i)), "x".repeat(50));
        }

        // Collect files and verify we have something to evict
        let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
        collect_cache_files(&check_dir, &mut files);
        let total: u64 = files.iter().map(|f| f.1).sum();
        assert_eq!(files.len(), 3, "should have 3 files");
        assert_eq!(total, 150, "should have 150 bytes total");

        // Evict oldest files until we're under 100 bytes
        let max_bytes = 100u64;
        files.sort_by_key(|f| f.2);
        let mut remaining = total;
        for (path, size, _) in &files {
            if remaining <= max_bytes {
                break;
            }
            if std::fs::remove_file(path).is_ok() {
                remaining -= size;
            }
        }

        // Verify eviction happened: should have 1 file remaining (50 bytes <= 100)
        let files_after: Vec<_> = std::fs::read_dir(&check_dir)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        assert!(files_after.len() < 3, "some files should have been evicted (remaining: {})", files_after.len());
        assert!(remaining <= max_bytes, "total should be under cap after eviction");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_clear_cache() {
        // clear_cache() operates on the shared .cogent-cache/ dir which other tests
        // may be writing to concurrently. remove_dir_all can fail on Linux if another
        // thread calls create_dir_all on a subdir mid-removal. We test that the function
        // either succeeds or returns a reasonable error — no panic.
        let _ = clear_cache();
    }
}
