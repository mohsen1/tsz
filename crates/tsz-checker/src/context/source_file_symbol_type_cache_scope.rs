use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_source_file_symbol_type_cache_scope() -> u64 {
    NEXT_SCOPE.fetch_add(1, Ordering::Relaxed).max(1)
}
