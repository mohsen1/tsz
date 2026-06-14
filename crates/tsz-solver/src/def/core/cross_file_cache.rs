//! Cross-file checker query memoization for the shared [`DefinitionStore`].
//!
//! Bundles the thread-safe cross-file query result cache with the per-file
//! delegation locks and the program-local scope stamp that keys it. Keeping
//! these together makes the cache's invalidation surface (scope stamp bumped
//! per virtual program; first-writer-wins entries) explicit instead of fused
//! into the definition god-object.
//!
//! [`DefinitionStore`]: super::DefinitionStore

use super::{CrossFileQueryCacheKey, CrossFileQueryCacheValue, DASHMAP_ENTRY_OVERHEAD, DefDashMap};
use crate::types::{TypeId, TypeParamInfo};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Cross-file query cache plus its delegation locks and scope stamp.
#[derive(Debug)]
pub(crate) struct CrossFileQueryCache {
    /// Thread-safe cache for cross-file checker queries (interface lowering,
    /// class instance type, interface member simple types, symbol type), keyed
    /// by `(kind, file_idx, primary, secondary, args_hash)`.
    resolved_cross_file_queries: DefDashMap<CrossFileQueryCacheKey, CrossFileQueryCacheValue>,

    /// Program-local scope mixed into source-file symbol-type query keys. Batch
    /// drivers stamp this from `ProgramContext` so reused shared stores cannot
    /// read stale entries from an earlier virtual program.
    source_file_symbol_type_cache_scope: AtomicU64,

    /// Per-file mutual exclusion locks for cross-file type delegation. Prevents
    /// concurrent delegation to the same target file.
    file_delegation_locks: DefDashMap<usize, Arc<Mutex<()>>>,
}

impl Default for CrossFileQueryCache {
    fn default() -> Self {
        Self {
            resolved_cross_file_queries: DefDashMap::default(),
            // Scope is 1-based; the getter/setter clamp to >= 1.
            source_file_symbol_type_cache_scope: AtomicU64::new(1),
            file_delegation_locks: DefDashMap::default(),
        }
    }
}

impl CrossFileQueryCache {
    /// Look up a previously resolved cross-file query result. Returns the shared
    /// `Arc` over the cached type-params so per-hit reads avoid a deep clone.
    #[inline]
    pub(crate) fn get(
        &self,
        kind: u8,
        file_idx: u32,
        primary: u32,
        secondary: u32,
        args_hash: u64,
    ) -> Option<(TypeId, Arc<Vec<TypeParamInfo>>)> {
        self.resolved_cross_file_queries
            .get(&(kind, file_idx, primary, secondary, args_hash))
            .map(|entry| {
                let (type_id, params) = entry.value();
                (*type_id, Arc::clone(params))
            })
    }

    /// Cache a cross-file query result. First writer wins to keep parallel
    /// checking deterministic when equivalent queries race.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert(
        &self,
        kind: u8,
        file_idx: u32,
        primary: u32,
        secondary: u32,
        args_hash: u64,
        type_id: TypeId,
        type_params: Vec<TypeParamInfo>,
    ) {
        self.resolved_cross_file_queries
            .entry((kind, file_idx, primary, secondary, args_hash))
            .or_insert_with(|| (type_id, Arc::new(type_params)));
    }

    /// Current program-local scope stamp (clamped to >= 1).
    #[inline]
    pub(crate) fn scope(&self) -> u64 {
        self.source_file_symbol_type_cache_scope
            .load(Ordering::Relaxed)
            .max(1)
    }

    /// Set the program-local scope stamp (clamped to >= 1).
    pub(crate) fn set_scope(&self, scope: u64) {
        self.source_file_symbol_type_cache_scope
            .store(scope.max(1), Ordering::Relaxed);
    }

    /// Initialize per-file delegation locks for parallel checking.
    pub(crate) fn init_file_locks(&self, file_count: usize) {
        for i in 0..file_count {
            self.file_delegation_locks
                .entry(i)
                .or_insert_with(|| Arc::new(Mutex::new(())));
        }
    }

    /// Get the delegation lock for a target file.
    pub(crate) fn file_delegation_lock(&self, file_idx: usize) -> Option<Arc<Mutex<()>>> {
        self.file_delegation_locks
            .get(&file_idx)
            .map(|r| Arc::clone(r.value()))
    }

    /// Estimated heap footprint of the query cache, in bytes (rough lower bound).
    pub(crate) fn estimated_size_bytes(&self) -> usize {
        let mut size = 0;
        for entry in &self.resolved_cross_file_queries {
            size += std::mem::size_of::<CrossFileQueryCacheKey>()
                + std::mem::size_of::<TypeId>()
                + DASHMAP_ENTRY_OVERHEAD;
            size += entry.value().1.capacity() * std::mem::size_of::<TypeParamInfo>();
        }
        size
    }
}
