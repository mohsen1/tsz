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
    use std::path::PathBuf;

    #[test]
    fn test_cache_key() {
        let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
        let path = PathBuf::from("/repo/TypeScript/tests/cases/compiler/foo.ts");
        assert_eq!(
            cache_key(&path, &test_dir),
            Some("compiler/foo.ts".to_string())
        );
    }

    #[test]
    fn test_cache_key_nested() {
        let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
        let path =
            PathBuf::from("/repo/TypeScript/tests/cases/conformance/types/intersection/bar.ts");
        assert_eq!(
            cache_key(&path, &test_dir),
            Some("conformance/types/intersection/bar.ts".to_string())
        );
    }

    #[test]
    fn test_cache_key_outside_test_dir() {
        let test_dir = PathBuf::from("/repo/TypeScript/tests/cases");
        let path = PathBuf::from("/somewhere/else/foo.ts");
        assert_eq!(cache_key(&path, &test_dir), None);
    }
}
