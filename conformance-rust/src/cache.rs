//! TSC cache module
//!
//! Handles loading, hashing, and fast lookup of TSC results from cache file.

use crate::tsc_results::TscResult;
use blake3::Hasher;
use std::collections::HashMap;
use std::path::Path;

/// TSC cache type: hash -> TSC result
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

/// Calculate deterministic hash for a test file
///
/// Hash includes: file content + all options (sorted deterministically)
pub fn calculate_test_hash(content: &str, options: &HashMap<String, String>) -> String {
    let mut hasher = Hasher::new();

    // Hash content
    hasher.update(content.as_bytes());

    // Hash options in sorted order for determinism
    let mut sorted_options: Vec<_> = options.iter().collect();
    sorted_options.sort_by_key(|(k, _)| *k);
    for (key, value) in sorted_options {
        hasher.update(key.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
    }

    // Return hex string
    hasher.finalize().to_hex().to_string()
}

/// Check if file is cached based on metadata (fast path)
///
/// Returns Some(result) if cached, None if not found or metadata mismatch
pub fn check_cache_metadata<'a>(
    cache: &'a TscCache,
    hash: &str,
    mtime_ms: u64,
    size: u64,
) -> Option<&'a TscResult> {
    cache.get(hash).and_then(|entry| {
        if entry.metadata.mtime_ms == mtime_ms && entry.metadata.size == size {
            Some(entry)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_test_hash() {
        let content = "function foo() {}";
        let mut options = HashMap::new();
        options.insert("strict".to_string(), "true".to_string());

        let hash1 = calculate_test_hash(content, &options);
        let hash2 = calculate_test_hash(content, &options);

        // Same input should produce same hash
        assert_eq!(hash1, hash2);

        // Different order of options should produce same hash
        let mut options2 = HashMap::new();
        options2.insert("strict".to_string(), "true".to_string());
        let hash3 = calculate_test_hash(content, &options2);
        assert_eq!(hash1, hash3);
    }
}
