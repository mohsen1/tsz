//! Registration-window eviction for the reused checker-pool [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the 2000-line
//! file-size cap. This is a child module of `query_cache`, so it keeps access
//! to the cache's private fields (`eval_cache`, `eval_dependency_index`, and
//! `registration_window_eval_keys`).
//!
//! The bounded checker pool reuses one `QueryCache` across the files of a
//! partition, and each file is a distinct def-registration window. A top-level
//! `eval_cache` entry that the depth-agnostic gate admitted but
//! `is_stable_for_run_wide_cache` refused (an `UnresolvedDef` registration-window
//! artifact) is reusable only *within* the window that produced it. The driver
//! evicts those entries at every file boundary so the invariant
//! `is_stable_for_run_wide_cache`'s documentation relies on — "the window ends
//! before the next registration can occur" — actually holds for the reused
//! cache (#16553).

use super::*;

impl QueryCache<'_> {
    /// Evict `eval_cache` entries published under a registration-window taint
    /// (see the `registration_window_eval_keys` field), retaining every clean
    /// entry so the pool keeps amortizing evaluation cost across files.
    pub fn evict_registration_window_eval_entries(&self) {
        let mut keys = self.registration_window_eval_keys.borrow_mut();
        if keys.is_empty() {
            return;
        }
        let mut cache = self.eval_cache.borrow_mut();
        for key in keys.drain() {
            if cache.remove(&key).is_some() {
                // Keep the reverse dependency index consistent so a later
                // `invalidate_for_def` never chases an already-evicted key.
                eval_dependency_index::forget_key(&self.eval_dependency_index, &key);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn insert_eval_entry_for_test(
        &self,
        key: EvaluationCacheKey,
        result: TypeId,
        registration_window_tainted: bool,
    ) {
        self.insert_eval_cache_entry(key, result);
        if registration_window_tainted {
            self.registration_window_eval_keys.borrow_mut().insert(key);
        }
    }

    #[cfg(test)]
    pub(crate) fn eval_cache_contains_key_for_test(&self, key: &EvaluationCacheKey) -> bool {
        self.eval_cache.borrow().contains_key(key)
    }
}
