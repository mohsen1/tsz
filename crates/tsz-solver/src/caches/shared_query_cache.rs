//! Thread-safe shared query cache for cross-file type checking.

use crate::caches::instantiation_cache::InstantiationCacheKey;
use crate::caches::shared_instantiation::shared_instantiation_family_requested;
use crate::caches::shared_instantiation::{
    collect_application_eval_entry_def_dependencies, collect_eval_entry_def_dependencies,
};
use crate::def::{DefId, DefinitionStore};
use crate::evaluation::request::EvaluationCacheKey;
use crate::intern::TypeInterner;
use crate::types::{RelationCacheKey, RelationCacheValue, TypeId};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashSet};

// The trailing two `bool`s are `no_unchecked_indexed_access` and
// `exact_optional_property_types`. Evaluating a generic application can expand
// a homomorphic mapped type whose optional-modifier stripping depends on
// `exactOptionalPropertyTypes`, so both options are part of the cache identity
// (issue #10970).
pub(super) type ApplicationEvalCacheKey = (DefId, smallvec::SmallVec<[TypeId; 4]>, bool, bool);

/// Thread-safe shared query cache for cross-file type checking.
///
/// In multi-file projects (e.g., ts-toolbelt with 242 files), each file checker
/// gets its own `QueryCache` with `RefCell`-based local caches. Without sharing,
/// the same type evaluations, subtype checks, and assignability checks are
/// recomputed independently by every file checker.
///
/// `SharedQueryCache` uses `DashMap` for concurrent read/write access across
/// Rayon worker threads. Each per-file `QueryCache` checks its local cache first
/// (zero overhead), then falls back to the shared cache on miss. Results are
/// written to both local and shared caches.
///
/// Only the highest-impact caches are shared:
/// - `eval_cache`: type evaluation (conditional types, mapped types, etc.)
/// - `subtype_cache`: subtype relation results
/// - `assignability_cache`: assignability relation results
///
/// For the relation caches the sharing covers *both* the top-level
/// `is_cached_policy_relation` entry and the inner `QueryDatabase` entries
/// driven by the `SubtypeChecker`'s recursive descent. The latter is the
/// dominant cache traffic in deep mapped/conditional utility-type code, and
/// without it sibling per-file checkers re-derive the same subtree relation
/// in every file. Inner writes are gated by `cache_definitive!` in the
/// `SubtypeChecker`, so only lazy-resolution-stable results reach the
/// shared store (#10921).
///
/// `application_eval_cache` and `instantiation_cache` are intentionally NOT
/// shared cross-file: parallel file checking can observe incomplete lib-merge
/// state during the first evaluation of a generic type alias (e.g. `Promise<T>`,
/// `Awaited<T>`), producing a stale result that is then returned to sibling
/// files. Keeping those caches per-file eliminates the ordering-sensitive
/// correctness risk. See issue #9507. `TSZ_SHARE_INSTANTIATION_CACHES=1`
/// enables the experimental #13240 witness path, but instantiation entries are
/// promoted only when the instantiation result boundary reports stable ambient
/// request state.
pub struct SharedQueryCache {
    pub(super) eval_cache: DashMap<EvaluationCacheKey, TypeId, FxBuildHasher>,
    pub(super) eval_dependency_index: DashMap<DefId, FxHashSet<EvaluationCacheKey>, FxBuildHasher>,
    eval_key_dependency_index: DashMap<EvaluationCacheKey, FxHashSet<DefId>, FxBuildHasher>,
    pub(super) subtype_cache: DashMap<RelationCacheKey, RelationCacheValue, FxBuildHasher>,
    pub(super) assignability_cache: DashMap<RelationCacheKey, RelationCacheValue, FxBuildHasher>,
    pub(super) application_eval_cache: DashMap<ApplicationEvalCacheKey, TypeId, FxBuildHasher>,
    pub(super) application_eval_dependency_index:
        DashMap<DefId, FxHashSet<ApplicationEvalCacheKey>, FxBuildHasher>,
    application_eval_key_dependency_index:
        DashMap<ApplicationEvalCacheKey, FxHashSet<DefId>, FxBuildHasher>,
    pub(super) instantiation_cache: DashMap<InstantiationCacheKey, TypeId, FxBuildHasher>,
    share_instantiation_family: bool,
}

impl SharedQueryCache {
    pub fn new() -> Self {
        SharedQueryCache {
            eval_cache: DashMap::with_hasher(FxBuildHasher),
            eval_dependency_index: DashMap::with_hasher(FxBuildHasher),
            eval_key_dependency_index: DashMap::with_hasher(FxBuildHasher),
            subtype_cache: DashMap::with_hasher(FxBuildHasher),
            assignability_cache: DashMap::with_hasher(FxBuildHasher),
            application_eval_cache: DashMap::with_hasher(FxBuildHasher),
            application_eval_dependency_index: DashMap::with_hasher(FxBuildHasher),
            application_eval_key_dependency_index: DashMap::with_hasher(FxBuildHasher),
            instantiation_cache: DashMap::with_hasher(FxBuildHasher),
            share_instantiation_family: shared_instantiation_family_requested(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_instantiation_family_test(share_instantiation_family: bool) -> Self {
        let mut cache = Self::new();
        cache.share_instantiation_family = share_instantiation_family;
        cache
    }

    /// Number of entries across all shared caches.
    pub fn total_entries(&self) -> usize {
        self.eval_cache.len()
            + self.subtype_cache.len()
            + self.assignability_cache.len()
            + self.application_eval_cache.len()
            + self.instantiation_cache.len()
    }

    #[inline]
    pub(super) const fn shares_instantiation_family(&self) -> bool {
        self.share_instantiation_family
    }

    pub(super) fn insert_application_eval_cache(
        &self,
        interner: &TypeInterner,
        definition_store: Option<&DefinitionStore>,
        key: ApplicationEvalCacheKey,
        result: TypeId,
    ) {
        let old_result = self.application_eval_cache.insert(key.clone(), result);
        if let Some(old_result) = old_result {
            self.remove_application_eval_dependencies(&key, old_result);
        }
        let deps = collect_application_eval_entry_def_dependencies(
            interner,
            definition_store,
            &key,
            result,
        );
        if deps.is_empty() {
            return;
        }
        let deps: FxHashSet<_> = deps.into_iter().collect();
        for &def_id in &deps {
            self.application_eval_dependency_index
                .entry(def_id)
                .or_default()
                .insert(key.clone());
        }
        self.application_eval_key_dependency_index.insert(key, deps);
    }

    pub(super) fn invalidate_application_eval_cache_for_def(
        &self,
        _interner: &TypeInterner,
        _definition_store: Option<&DefinitionStore>,
        def_id: DefId,
    ) {
        self.invalidate_eval_cache_for_def(def_id);
        if let Some((_, keys)) = self.application_eval_dependency_index.remove(&def_id) {
            for key in keys {
                if let Some((_, old_result)) = self.application_eval_cache.remove(&key) {
                    self.remove_application_eval_dependencies(&key, old_result);
                }
            }
        }
    }

    pub(super) fn insert_eval_cache(
        &self,
        interner: &TypeInterner,
        definition_store: Option<&DefinitionStore>,
        key: EvaluationCacheKey,
        result: TypeId,
    ) {
        let old_result = self.eval_cache.insert(key, result);
        self.record_eval_dependencies(interner, definition_store, key, old_result, result);
    }

    pub(super) fn insert_eval_cache_if_absent(
        &self,
        interner: &TypeInterner,
        definition_store: Option<&DefinitionStore>,
        key: EvaluationCacheKey,
        result: TypeId,
    ) {
        match self.eval_cache.entry(key) {
            Entry::Occupied(_) => {}
            Entry::Vacant(vacant) => {
                vacant.insert(result);
                self.record_eval_dependencies(interner, definition_store, key, None, result);
            }
        }
    }

    fn invalidate_eval_cache_for_def(&self, def_id: DefId) {
        let Some((_, keys)) = self.eval_dependency_index.remove(&def_id) else {
            return;
        };
        for key in keys {
            if let Some((_, old_result)) = self.eval_cache.remove(&key) {
                self.remove_eval_dependencies(key, old_result);
            }
        }
    }

    fn record_eval_dependencies(
        &self,
        interner: &TypeInterner,
        definition_store: Option<&DefinitionStore>,
        key: EvaluationCacheKey,
        old_result: Option<TypeId>,
        result: TypeId,
    ) {
        if let Some(old_result) = old_result {
            self.remove_eval_dependencies(key, old_result);
        }
        let deps = collect_eval_entry_def_dependencies(interner, definition_store, key, result);
        if deps.is_empty() {
            return;
        }
        let deps: FxHashSet<_> = deps.into_iter().collect();
        for &def_id in &deps {
            self.eval_dependency_index
                .entry(def_id)
                .or_default()
                .insert(key);
        }
        self.eval_key_dependency_index.insert(key, deps);
    }

    fn remove_eval_dependencies(&self, key: EvaluationCacheKey, _result: TypeId) {
        let Some((_, deps)) = self.eval_key_dependency_index.remove(&key) else {
            return;
        };
        for def_id in deps {
            let Some(mut keys) = self.eval_dependency_index.get_mut(&def_id) else {
                continue;
            };
            keys.remove(&key);
            let empty = keys.is_empty();
            drop(keys);
            if empty {
                self.eval_dependency_index
                    .remove_if(&def_id, |_, keys| keys.is_empty());
            }
        }
    }

    fn remove_application_eval_dependencies(&self, key: &ApplicationEvalCacheKey, _result: TypeId) {
        let Some((_, deps)) = self.application_eval_key_dependency_index.remove(key) else {
            return;
        };
        for def_id in deps {
            let Some(mut keys) = self.application_eval_dependency_index.get_mut(&def_id) else {
                continue;
            };
            keys.remove(key);
            let empty = keys.is_empty();
            drop(keys);
            if empty {
                self.application_eval_dependency_index
                    .remove_if(&def_id, |_, keys| keys.is_empty());
            }
        }
    }

    /// Estimate the resident heap bytes of the shared cache maps.
    ///
    /// Entry-count-based estimate (`DashMap` does not expose bucket capacity)
    /// for residency accounting (#13249 step 1); called once at perf-counter
    /// snapshot time, never on a checking hot path.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // DashMap per-entry overhead: bucket slot + hash + shard padding.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;
        let mut size = std::mem::size_of::<Self>();
        size += self.eval_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<EvaluationCacheKey>()
                + std::mem::size_of::<TypeId>());
        size += self.eval_dependency_index.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<DefId>()
                + std::mem::size_of::<FxHashSet<EvaluationCacheKey>>());
        size += self.eval_key_dependency_index.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<EvaluationCacheKey>()
                + std::mem::size_of::<FxHashSet<DefId>>());
        for keys in &self.eval_dependency_index {
            let keys = keys.value();
            size +=
                keys.len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<EvaluationCacheKey>());
        }
        for deps in &self.eval_key_dependency_index {
            size += deps.value().len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<DefId>());
        }
        size += (self.subtype_cache.len() + self.assignability_cache.len())
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<RelationCacheKey>()
                + std::mem::size_of::<bool>());
        size += self.application_eval_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<ApplicationEvalCacheKey>()
                + std::mem::size_of::<TypeId>());
        size += self.application_eval_dependency_index.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<DefId>()
                + std::mem::size_of::<FxHashSet<ApplicationEvalCacheKey>>());
        size += self.application_eval_key_dependency_index.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<ApplicationEvalCacheKey>()
                + std::mem::size_of::<FxHashSet<DefId>>());
        for keys in &self.application_eval_dependency_index {
            let keys = keys.value();
            size += keys.len()
                * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<ApplicationEvalCacheKey>());
        }
        for deps in &self.application_eval_key_dependency_index {
            size += deps.value().len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<DefId>());
        }
        size += self.instantiation_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<InstantiationCacheKey>()
                + std::mem::size_of::<TypeId>());
        size
    }
}

impl Default for SharedQueryCache {
    fn default() -> Self {
        Self::new()
    }
}
