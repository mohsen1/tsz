//! Content-addressed `DefId` generator for LSP mode.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use tsz_common::interner::Atom;

use super::DefId;

/// Content-addressed `DefId` generator for LSP mode.
///
/// Uses a hash of (name, `file_id`, span) to generate stable `DefIds`
/// that survive file edits without changing unrelated definitions.
pub struct ContentAddressedDefIds {
    /// Hash -> `DefId` mapping for deduplication
    hash_to_def: DashMap<u64, DefId>,

    /// Next `DefId` for new hashes
    next_id: AtomicU32,
}

impl Default for ContentAddressedDefIds {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentAddressedDefIds {
    /// Create a new content-addressed `DefId` generator.
    pub fn new() -> Self {
        Self {
            hash_to_def: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
        }
    }

    /// Get or create a `DefId` for the given content hash.
    ///
    /// # Arguments
    /// - `name`: Definition name
    /// - `file_id`: File identifier
    /// - `span_start`: Start offset of definition
    pub fn get_or_create(&self, name: Atom, file_id: u32, span_start: u32) -> DefId {
        use std::hash::{Hash, Hasher};

        let mut hasher = rustc_hash::FxHasher::default();
        name.hash(&mut hasher);
        file_id.hash(&mut hasher);
        span_start.hash(&mut hasher);
        let hash = hasher.finish();

        if let Some(existing) = self.hash_to_def.get(&hash) {
            return *existing;
        }

        let id = DefId(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.hash_to_def.insert(hash, id);
        id
    }

    /// Clear all mappings (for testing).
    pub fn clear(&self) {
        self.hash_to_def.clear();
        self.next_id.store(DefId::FIRST_VALID, Ordering::SeqCst);
    }
}
