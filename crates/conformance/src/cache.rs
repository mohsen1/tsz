//! TSC cache module
//!
//! Handles loading and fast lookup of TSC results from cache file.
//! Cache keys are relative file paths (e.g., "compiler/foo.ts") since each
//! conformance test is a single file (multi-file tests use @filename directives).

use crate::tsc_results::TscResult;
use std::collections::HashMap;
use std::path::Path;

/// TSC cache type: relative file path -> TSC result
pub type TscCache = HashMap<String, TscResult>;

/// Load TSC cache from JSON file
///
/// Uses streaming deserialization to avoid loading entire file into memory twice.
pub fn load_cache(cache_path: &Path) -> anyhow::Result<TscCache> {
    use std::io::BufReader;

    let file = std::fs::File::open(cache_path)?;
    let reader = BufReader::new(file);

    let cache: TscCache = serde_json::from_reader(reader)
        .map_err(|e| anyhow::anyhow!("Failed to parse cache JSON: {}", e))?;

    Ok(cache)
}

/// Compute cache key for a test file: its path relative to the test directory.
///
/// Both paths are canonicalized before computing the relative path to handle
/// symlinks, `./` prefixes, and other path normalization differences.
///
/// Returns `None` if the path cannot be made relative to `test_dir`.
pub fn cache_key(path: &Path, test_dir: &Path) -> Option<String> {
    // Try direct strip first (fast path)
    if let Ok(rel) = path.strip_prefix(test_dir) {
        return Some(rel.to_string_lossy().to_string());
    }
    // Canonicalize both to handle symlinks and relative paths
    let canon_path = path.canonicalize().ok()?;
    let canon_dir = test_dir.canonicalize().ok()?;
    canon_path
        .strip_prefix(&canon_dir)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// Look up a test in the cache by its relative path key.
pub fn lookup<'a>(cache: &'a TscCache, key: &str) -> Option<&'a TscResult> {
    cache.get(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_result() -> TscResult {
        TscResult {
            metadata: crate::tsc_results::FileMetadata {
                mtime_ms: 123,
                size: 456,
                typescript_version: Some("5.4.0".to_string()),
            },
            error_codes: vec![2307, 2322],
            diagnostic_fingerprints: Vec::new(),
        }
    }

    #[test]
    fn load_cache_reads_entries_and_lookup_finds_them() {
        let mut file = NamedTempFile::new().expect("temp cache file");
        let cache_json = serde_json::json!({
            "compiler/foo.ts": sample_result(),
        });
        serde_json::to_writer(&mut file, &cache_json).expect("write cache json");
        file.flush().expect("flush cache json");

        let cache = load_cache(file.path()).expect("load cache");
        let result = lookup(&cache, "compiler/foo.ts").expect("cache entry");

        assert_eq!(result.error_codes, vec![2307, 2322]);
        assert_eq!(result.metadata.size, 456);
        assert!(lookup(&cache, "missing.ts").is_none());
    }

    #[test]
    fn load_cache_reports_json_parse_errors() {
        let file = NamedTempFile::new().expect("temp cache file");
        fs::write(file.path(), "{ not valid json").expect("write invalid cache json");

        let err = load_cache(file.path()).expect_err("cache load should fail");
        let message = format!("{err:#}");
        assert!(message.contains("Failed to parse cache JSON"));
    }

    #[test]
    fn cache_key_uses_canonicalized_paths_when_direct_strip_fails() {
        let tempdir = tempfile::tempdir().expect("temp directory");
        let test_dir = tempdir.path().join("cases");
        let test_dir_with_dot = test_dir.join(".");
        let compiler_dir = test_dir.join("compiler");
        fs::create_dir_all(&compiler_dir).expect("create cache test directories");

        let path = compiler_dir.join("foo.ts");

        assert_eq!(
            cache_key(&path, &test_dir_with_dot),
            Some("compiler/foo.ts".to_string())
        );
    }
}
