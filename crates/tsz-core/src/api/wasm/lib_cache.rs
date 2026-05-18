use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::binder::BinderState;
use crate::lib_loader::LibFile;
use crate::parser::ParserState;

type LibFileCache = FxHashMap<(String, u64), Arc<LibFile>>;

// Global cache for parsed lib files to avoid re-parsing lib.d.ts per test.
// Key: (file_name, content_hash), Value: Arc<LibFile>.
static LIB_FILE_CACHE: Lazy<Mutex<LibFileCache>> = Lazy::new(|| Mutex::new(FxHashMap::default()));
static LIB_FILE_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static LIB_FILE_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LibFileCacheStatistics {
    pub(crate) entries: usize,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
}

impl LibFileCacheStatistics {
    pub(crate) const fn estimated_size_bytes(&self) -> usize {
        const BUCKET_OVERHEAD: usize = 8;
        self.entries
            * (BUCKET_OVERHEAD + size_of::<String>() + size_of::<u64>() + size_of::<Arc<LibFile>>())
    }
}

/// Simple hash function for lib file content.
fn hash_lib_content(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Get or create a cached lib file. This avoids re-parsing lib.d.ts for every test.
pub(crate) fn get_or_create_lib_file(file_name: String, source_text: String) -> Arc<LibFile> {
    let content_hash = hash_lib_content(&source_text);
    let cache_key = (file_name.clone(), content_hash);

    // Try to get from cache.
    {
        let cache = LIB_FILE_CACHE
            .lock()
            .expect("LIB_FILE_CACHE mutex poisoned");
        if let Some(cached) = cache.get(&cache_key) {
            LIB_FILE_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            return Arc::clone(cached);
        }
    }

    // Not in cache - parse and bind.
    LIB_FILE_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    let mut lib_parser = ParserState::new(file_name.clone(), source_text);
    let source_file_idx = lib_parser.parse_source_file();

    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);

    let lib_file = Arc::new(LibFile::new(file_name, arena, binder, source_file_idx));

    // Store in cache.
    {
        let mut cache = LIB_FILE_CACHE
            .lock()
            .expect("LIB_FILE_CACHE mutex poisoned");
        if let Some(cached) = cache.get(&cache_key) {
            return Arc::clone(cached);
        }
        cache.insert(cache_key, Arc::clone(&lib_file));
    }

    lib_file
}

pub(crate) fn lib_file_cache_statistics() -> LibFileCacheStatistics {
    let cache = LIB_FILE_CACHE
        .lock()
        .expect("LIB_FILE_CACHE mutex poisoned");
    LibFileCacheStatistics {
        entries: cache.len(),
        hits: LIB_FILE_CACHE_HITS.load(Ordering::Relaxed),
        misses: LIB_FILE_CACHE_MISSES.load(Ordering::Relaxed),
    }
}

pub(crate) fn lib_file_cache_statistics_json() -> String {
    let stats = lib_file_cache_statistics();
    serde_json::json!({
        "entries": stats.entries,
        "hits": stats.hits,
        "misses": stats.misses,
        "estimatedSizeBytes": stats.estimated_size_bytes(),
    })
    .to_string()
}

#[cfg(test)]
fn clear_lib_file_cache_for_test() {
    LIB_FILE_CACHE
        .lock()
        .expect("LIB_FILE_CACHE mutex poisoned")
        .clear();
    LIB_FILE_CACHE_HITS.store(0, Ordering::Relaxed);
    LIB_FILE_CACHE_MISSES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_file_cache_statistics_track_entries_hits_and_misses() {
        clear_lib_file_cache_for_test();

        let first = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { length: number; }\n".to_string(),
        );
        let after_first = lib_file_cache_statistics();
        assert_eq!(after_first.entries, 1);
        assert_eq!(after_first.hits, 0);
        assert_eq!(after_first.misses, 1);
        assert!(after_first.estimated_size_bytes() > 0);

        let second = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { length: number; }\n".to_string(),
        );
        let after_second = lib_file_cache_statistics();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(after_second.entries, 1);
        assert_eq!(after_second.hits, 1);
        assert_eq!(after_second.misses, 1);
        let json: serde_json::Value =
            serde_json::from_str(&lib_file_cache_statistics_json()).unwrap();
        assert_eq!(json["entries"], 1);
        assert_eq!(json["hits"], 1);
        assert_eq!(json["misses"], 1);
        assert!(json["estimatedSizeBytes"].as_u64().unwrap() > 0);

        let third = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { readonly length: number; }\n".to_string(),
        );
        let after_third = lib_file_cache_statistics();
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(after_third.entries, 2);
        assert_eq!(after_third.hits, 1);
        assert_eq!(after_third.misses, 2);
    }
}
