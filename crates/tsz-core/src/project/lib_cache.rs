use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

use crate::binder::BinderState;
use crate::lib_loader::LibFile;
use crate::parser::ParserState;

type LibFileCache = FxHashMap<(String, u64), Arc<LibFile>>;

// Global cache for parsed lib files to avoid re-parsing lib.d.ts per test.
// Key: (file_name, content_hash), Value: Arc<LibFile>.
static LIB_FILE_CACHE: Lazy<Mutex<LibFileCache>> = Lazy::new(|| Mutex::new(FxHashMap::default()));

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
            return Arc::clone(cached);
        }
    }

    // Not in cache - parse and bind.
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
        cache.insert(cache_key, Arc::clone(&lib_file));
    }

    lib_file
}
