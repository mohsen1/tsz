use rustc_hash::{FxHashMap, FxHashSet};
use tsz_solver::{DefId, TypeId};

use super::{AssignabilityEvalStamp, CheckerContext, EnvEvalCacheEntry};

/// Soft cap on the number of persistent `env_eval_cache` entries that the
/// per-evaluation *intermediate* seed/persist memo will marshal.
///
/// The top-level result memo (`cache_env_eval_result` / `lookup_env_eval_cache`)
/// is always honored and is the only correctness-relevant cache. The
/// intermediate seed/persist path is a pure speed memo: it pre-populates a
/// fresh evaluator's per-run cache with already-computed `(key -> value)` pairs
/// and saves drained intermediates for future runs. Because each
/// `evaluate_type_with_env_impl` call re-marshals the *entire* growing cache
/// (clone into a `Vec`, `extend` a fresh map, then re-scan every drained entry
/// with recursive `contains_*` predicates), the round-trip is `O(cache_size)`
/// per call and `O(N^2)` across a file with `N` alias-sharing type positions.
///
/// Once the cache exceeds this many entries the marshalling cost dominates the
/// memo benefit, so the intermediate seed/persist is skipped. Skipping only
/// changes speed, never results: a deterministic evaluator recomputes the same
/// sub-term values on demand. The cap is keyed by cache size (a structural
/// invariant), not by any fixture, file name, or identifier.
pub(crate) const ENV_EVAL_SEED_PERSIST_SOFT_CAP: usize = 256;

pub(crate) type ContextualSignatureNormalizationStamp =
    (AssignabilityEvalStamp, bool, bool, bool, bool);

#[derive(Default)]
pub(crate) struct EnvEvalCache {
    entries: FxHashMap<TypeId, EnvEvalCacheEntry>,
    defs_by_key: FxHashMap<TypeId, Box<[DefId]>>,
    keys_by_def: FxHashMap<DefId, FxHashSet<TypeId>>,
    contextual_signature_normalizations: TypeIdResultCache,
}

#[derive(Default)]
struct TypeIdResultCache {
    entries: FxHashMap<TypeId, StampedTypeIdResult>,
    defs_by_key: FxHashMap<TypeId, Box<[DefId]>>,
    keys_by_def: FxHashMap<DefId, FxHashSet<TypeId>>,
}

#[derive(Clone, Copy)]
struct StampedTypeIdResult {
    stamp: ContextualSignatureNormalizationStamp,
    result: TypeId,
}

impl TypeIdResultCache {
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn entry_capacity(&self) -> usize {
        self.entries.capacity()
    }

    fn get(&self, type_id: TypeId, stamp: ContextualSignatureNormalizationStamp) -> Option<TypeId> {
        let entry = self.entries.get(&type_id).copied()?;
        (entry.stamp == stamp).then_some(entry.result)
    }

    fn insert(
        &mut self,
        type_id: TypeId,
        stamp: ContextualSignatureNormalizationStamp,
        result: TypeId,
        dependency_defs: Box<[DefId]>,
    ) {
        let entry = StampedTypeIdResult { stamp, result };
        if self.entries.insert(type_id, entry).is_some() {
            self.remove_key_from_index(type_id);
        }
        self.insert_key_into_index(type_id, dependency_defs);
    }

    fn remove(&mut self, type_id: TypeId) -> Option<StampedTypeIdResult> {
        let removed = self.entries.remove(&type_id);
        if removed.is_some() {
            self.remove_key_from_index(type_id);
        }
        removed
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.defs_by_key.clear();
        self.keys_by_def.clear();
    }

    fn invalidate_for_def(&mut self, def_id: DefId) -> usize {
        let Some(keys) = self.keys_by_def.remove(&def_id) else {
            return 0;
        };
        let keys: Vec<_> = keys.into_iter().collect();
        let removed = keys.len();
        for key in keys {
            self.entries.remove(&key);
            self.remove_key_from_index(key);
        }
        removed
    }

    fn invalidate_matching(
        &mut self,
        should_remove: impl Fn(TypeId, StampedTypeIdResult) -> bool,
    ) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        let keys: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&key, &value)| should_remove(key, value).then_some(key))
            .collect();
        let removed = keys.len();
        for key in keys {
            self.remove(key);
        }
        removed
    }

    fn insert_key_into_index(&mut self, type_id: TypeId, dependency_defs: Box<[DefId]>) {
        if dependency_defs.is_empty() {
            return;
        }
        for &def_id in &dependency_defs {
            self.keys_by_def.entry(def_id).or_default().insert(type_id);
        }
        self.defs_by_key.insert(type_id, dependency_defs);
    }

    fn remove_key_from_index(&mut self, type_id: TypeId) {
        let Some(defs) = self.defs_by_key.remove(&type_id) else {
            return;
        };
        for def_id in &defs {
            if let Some(keys) = self.keys_by_def.get_mut(def_id) {
                keys.remove(&type_id);
                if keys.is_empty() {
                    self.keys_by_def.remove(def_id);
                }
            }
        }
    }
}

impl EnvEvalCache {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn get(&self, type_id: TypeId) -> Option<EnvEvalCacheEntry> {
        self.entries.get(&type_id).copied()
    }

    pub(crate) fn contains_key(&self, type_id: TypeId) -> bool {
        self.entries.contains_key(&type_id)
    }

    pub(crate) fn entry_capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(crate) fn contextual_signature_normalization_len(&self) -> usize {
        self.contextual_signature_normalizations.len()
    }

    pub(crate) fn contextual_signature_normalization_entry_capacity(&self) -> usize {
        self.contextual_signature_normalizations.entry_capacity()
    }

    pub(crate) fn get_contextual_signature_normalization(
        &self,
        type_id: TypeId,
        stamp: ContextualSignatureNormalizationStamp,
    ) -> Option<TypeId> {
        self.contextual_signature_normalizations.get(type_id, stamp)
    }

    pub(crate) fn insert_contextual_signature_normalization(
        &mut self,
        type_id: TypeId,
        stamp: ContextualSignatureNormalizationStamp,
        result: TypeId,
        dependency_defs: Box<[DefId]>,
    ) {
        self.contextual_signature_normalizations
            .insert(type_id, stamp, result, dependency_defs);
    }

    pub(crate) fn remove_contextual_signature_normalization(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        self.contextual_signature_normalizations
            .remove(type_id)
            .map(|entry| entry.result)
    }

    pub(crate) fn invalidate_contextual_signature_normalizations_matching(
        &mut self,
        should_remove: impl Fn(TypeId, TypeId) -> bool,
    ) -> usize {
        self.contextual_signature_normalizations
            .invalidate_matching(|key, entry| should_remove(key, entry.result))
    }

    pub(crate) fn insert(
        &mut self,
        type_id: TypeId,
        entry: EnvEvalCacheEntry,
        dependency_defs: Box<[DefId]>,
    ) {
        if self.entries.insert(type_id, entry).is_some() {
            self.remove_key_from_index(type_id);
        }
        self.insert_key_into_index(type_id, dependency_defs);
    }

    pub(crate) fn insert_if_absent(
        &mut self,
        type_id: TypeId,
        entry: EnvEvalCacheEntry,
        dependency_defs: Box<[DefId]>,
    ) {
        if self.entries.contains_key(&type_id) {
            return;
        }
        self.entries.insert(type_id, entry);
        self.insert_key_into_index(type_id, dependency_defs);
    }

    pub(crate) fn remove(&mut self, type_id: TypeId) -> Option<EnvEvalCacheEntry> {
        let removed = self.entries.remove(&type_id);
        if removed.is_some() {
            self.remove_key_from_index(type_id);
        }
        removed
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.defs_by_key.clear();
        self.keys_by_def.clear();
        self.contextual_signature_normalizations.clear();
    }

    pub(crate) fn seed_entries(&self) -> Vec<(TypeId, TypeId)> {
        let mut entries = Vec::with_capacity(self.entries.len());
        for (&k, v) in &self.entries {
            if k != v.result && !k.is_intrinsic() && !v.depth_exceeded {
                entries.push((k, v.result));
            }
        }
        entries
    }

    pub(crate) fn invalidate_for_def(&mut self, def_id: DefId) {
        if let Some(keys) = self.keys_by_def.remove(&def_id) {
            let keys: Vec<_> = keys.into_iter().collect();
            for key in keys {
                self.entries.remove(&key);
                self.remove_key_from_index(key);
            }
        }
        self.contextual_signature_normalizations
            .invalidate_for_def(def_id);
    }

    pub(crate) fn invalidate_matching(
        &mut self,
        should_remove: impl Fn(TypeId, EnvEvalCacheEntry) -> bool,
    ) -> usize {
        if self.entries.is_empty() {
            return 0;
        }
        let keys: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&key, &entry)| should_remove(key, entry).then_some(key))
            .collect();
        let removed = keys.len();
        for key in keys {
            self.remove(key);
        }
        removed
    }

    #[cfg(test)]
    pub(crate) fn indexed_key_count_for_def(&self, def_id: DefId) -> usize {
        self.keys_by_def.get(&def_id).map_or(0, FxHashSet::len)
    }

    fn insert_key_into_index(&mut self, type_id: TypeId, dependency_defs: Box<[DefId]>) {
        if dependency_defs.is_empty() {
            return;
        }
        for &def_id in &dependency_defs {
            self.keys_by_def.entry(def_id).or_default().insert(type_id);
        }
        self.defs_by_key.insert(type_id, dependency_defs);
    }

    fn remove_key_from_index(&mut self, type_id: TypeId) {
        let Some(defs) = self.defs_by_key.remove(&type_id) else {
            return;
        };
        for def_id in &defs {
            if let Some(keys) = self.keys_by_def.get_mut(def_id) {
                keys.remove(&type_id);
                if keys.is_empty() {
                    self.keys_by_def.remove(def_id);
                }
            }
        }
    }
}

/// Kill-switch: set `TSZ_DISABLE_ENV_EVAL_SEED_CAP` to a non-empty, non-`0`
/// value to force the legacy behavior (always seed/persist the full cache).
///
/// Used to prove byte-identical diagnostics: with the cap on vs. off, the
/// intermediate seed/persist memo is speed-only, so output must be identical
/// for every input. The cap only ever skips a *performance* memo above
/// [`ENV_EVAL_SEED_PERSIST_SOFT_CAP`] entries.
pub(crate) fn env_eval_seed_cap_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_ENV_EVAL_SEED_CAP")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Kill-switch: set `TSZ_DISABLE_UNRESOLVED_DEF_CACHE_BACKSTOP` to a non-empty,
/// non-`0` value to restore the legacy behavior of persisting an `env_eval_cache`
/// result even when the evaluation observed an unresolved `Lazy(DefId)`.
///
/// The backstop (issue #13980) suppresses the authoritative `env_eval_cache`
/// write — and the intermediate seed/persist entries — for any evaluation pass
/// whose result is a *registration-window artifact*: the solver reports it via
/// [`tsz_solver::EvaluateResult::unresolved_def_seen`] when an `Application`'s
/// base `DefId` had no resolvable body (mid-registration, resolves to `unknown`,
/// or self-`Lazy`). Such a result is a function of which refs happened to be
/// resolved when the pass ran, not of the input `TypeId`, so caching it keyed
/// purely on the `TypeId` (with no generation guard) lets the under-resolved
/// answer permanently shadow the correct one once the def registers.
///
/// Under the legacy eager `ensure_refs_resolved` pre-walk the flag stayed
/// `false` (every ref was forced up front), so the backstop was inert. Under the
/// on-demand-forcing default (issue #12101) the flag can now be `true`, which is
/// why the previously-documented-but-unwired backstop became load-bearing. The
/// kill-switch lets the two modes be compared byte-for-byte: with it set, output
/// must match the legacy path on every input that never trips the flag.
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn unresolved_def_cache_backstop_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_UNRESOLVED_DEF_CACHE_BACKSTOP")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

impl<'a> CheckerContext<'a> {
    /// Whether the per-evaluation intermediate seed/persist memo should run.
    ///
    /// Returns `false` once the persistent cache has grown past the soft cap,
    /// unless the kill-switch forces the legacy always-on behavior. The
    /// top-level result memo is unaffected by this gate.
    pub(crate) fn env_eval_seed_persist_enabled(&self) -> bool {
        if env_eval_seed_cap_disabled() {
            return true;
        }
        self.env_eval_cache.borrow().len() <= ENV_EVAL_SEED_PERSIST_SOFT_CAP
    }

    /// Memoized `collect_lazy_def_ids`: the walk is pure over the immutable
    /// interned type structure, so the reachable lazy-`DefId` set is cached per
    /// `type_id` and reused across the many hot callers that re-query the same
    /// (lib-heavy) types. Returns an `Rc` slice for cheap clone-on-hit.
    pub(crate) fn collect_lazy_def_ids_cached(
        &self,
        type_id: TypeId,
    ) -> std::rc::Rc<[tsz_solver::DefId]> {
        if let Some(cached) = self.lazy_def_ids_cache.borrow().get(&type_id) {
            return std::rc::Rc::clone(cached);
        }
        let collected: std::rc::Rc<[tsz_solver::DefId]> =
            crate::query_boundaries::common::collect_lazy_def_ids(self.types, type_id).into();
        self.lazy_def_ids_cache
            .borrow_mut()
            .insert(type_id, std::rc::Rc::clone(&collected));
        collected
    }

    /// Memoized `collect_type_queries`: like `collect_lazy_def_ids_cached`, the
    /// walk is pure over the immutable interned type structure, so the reachable
    /// `TypeQuery` (`typeof X`) `SymbolRef` set is stable per `type_id`. The
    /// relation-readiness worklist (`ensure_refs_resolved`) re-walks the whole
    /// signature tree once per distinct DOM/lib method signature; lib method
    /// signatures almost never contain `typeof`, so the walk usually returns the
    /// empty set after a full traversal. Caching that result (empty or not)
    /// removes the per-distinct-method re-walk. Returns an `Rc` slice for cheap
    /// clone-on-hit.
    pub(crate) fn collect_type_queries_cached(
        &self,
        type_id: TypeId,
    ) -> std::rc::Rc<[tsz_solver::SymbolRef]> {
        if let Some(cached) = self.type_queries_cache.borrow().get(&type_id) {
            return std::rc::Rc::clone(cached);
        }
        let collected: std::rc::Rc<[tsz_solver::SymbolRef]> =
            crate::query_boundaries::common::collect_type_queries(self.types, type_id).into();
        self.type_queries_cache
            .borrow_mut()
            .insert(type_id, std::rc::Rc::clone(&collected));
        collected
    }

    fn type_mentions_def(&self, type_id: TypeId, def_id: tsz_solver::DefId) -> bool {
        self.collect_lazy_def_ids_cached(type_id).contains(&def_id)
    }

    /// Whether `type_id` references any type-alias `DefId` flagged as
    /// unconditionally-infinite (TS2589). Such aliases are error types in tsc,
    /// so assignments involving them must not produce structural mismatches.
    pub(crate) fn type_involves_depth_poisoned_def(&self, type_id: TypeId) -> bool {
        self.collect_lazy_def_ids_cached(type_id)
            .iter()
            .any(|&def_id| self.definition_store.is_depth_poisoned(def_id))
    }

    pub(crate) fn lookup_env_eval_cache(&self, type_id: TypeId) -> Option<EnvEvalCacheEntry> {
        self.env_eval_cache.borrow().get(type_id)
    }

    pub(crate) fn lookup_contextual_signature_normalization_cache(
        &self,
        type_id: TypeId,
        stamp: ContextualSignatureNormalizationStamp,
    ) -> Option<TypeId> {
        self.env_eval_cache
            .borrow()
            .get_contextual_signature_normalization(type_id, stamp)
    }

    pub(crate) fn env_eval_cache_seed_entries(&self) -> Vec<(TypeId, TypeId)> {
        let cache = self.env_eval_cache.borrow();
        if cache.is_empty() {
            return Vec::new();
        }
        if !env_eval_seed_cap_disabled() && cache.len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP {
            return Vec::new();
        }
        cache.seed_entries()
    }

    fn collect_env_eval_dependency_defs(
        &self,
        type_id: TypeId,
        defs: &mut FxHashSet<DefId>,
        stack: &mut Vec<DefId>,
    ) {
        for def_id in self.collect_lazy_def_ids_cached(type_id).iter().copied() {
            if defs.insert(def_id) {
                stack.push(def_id);
            }
        }
    }

    fn env_eval_entry_dependency_defs(&self, key: TypeId, result: TypeId) -> Box<[DefId]> {
        let mut defs = FxHashSet::default();
        let mut stack = Vec::new();
        self.collect_env_eval_dependency_defs(key, &mut defs, &mut stack);
        self.collect_env_eval_dependency_defs(result, &mut defs, &mut stack);
        while let Some(def_id) = stack.pop() {
            if let Some(body) = self.definition_store.get_body(def_id) {
                self.collect_env_eval_dependency_defs(body, &mut defs, &mut stack);
            }
        }
        defs.into_iter().collect::<Vec<_>>().into_boxed_slice()
    }

    pub(crate) fn cache_env_eval_result(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        let dependency_defs = self.env_eval_entry_dependency_defs(type_id, result);
        self.env_eval_cache.borrow_mut().insert(
            type_id,
            EnvEvalCacheEntry {
                result,
                depth_exceeded,
            },
            dependency_defs,
        );
    }

    pub(crate) fn cache_env_eval_result_if_absent(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        let dependency_defs = self.env_eval_entry_dependency_defs(type_id, result);
        self.env_eval_cache.borrow_mut().insert_if_absent(
            type_id,
            EnvEvalCacheEntry {
                result,
                depth_exceeded,
            },
            dependency_defs,
        );
    }

    pub(crate) fn cache_contextual_signature_normalization_result(
        &self,
        type_id: TypeId,
        stamp: ContextualSignatureNormalizationStamp,
        result: TypeId,
    ) {
        let dependency_defs = self.env_eval_entry_dependency_defs(type_id, result);
        self.env_eval_cache
            .borrow_mut()
            .insert_contextual_signature_normalization(type_id, stamp, result, dependency_defs);
    }

    pub(crate) fn clear_env_eval_cache(&self) {
        self.env_eval_cache.borrow_mut().clear();
    }

    /// Drop the single top-level `env_eval_cache` entry keyed by `type_id`, if
    /// present. Returns whether an entry was actually removed.
    ///
    /// This is the minimal targeted counterpart to [`Self::clear_env_eval_cache`]
    /// (global flush) and [`Self::clear_type_evaluation_caches_for_def`]
    /// (per-`DefId` sweep): it invalidates exactly one evaluation result without
    /// disturbing any other entry or paying an `O(cache)` structural scan. Use it
    /// when a specific `type_id` must be re-evaluated — e.g. re-resolving a type
    /// under a different resolution mode after a speculative (bounded) verdict —
    /// so the next [`Self::lookup_env_eval_cache`] recomputes rather than
    /// short-circuiting to the stale entry.
    pub(crate) fn invalidate_env_eval_for(&self, type_id: TypeId) -> bool {
        let mut cache = self.env_eval_cache.borrow_mut();
        let removed = cache.remove(type_id).is_some();
        cache.remove_contextual_signature_normalization(type_id);
        cache.invalidate_contextual_signature_normalizations_matching(|key, value| {
            key == type_id || value == type_id
        });
        removed
    }

    /// Drop every `env_eval_cache` entry structurally reachable from `type_id`:
    /// the entry for `type_id` itself plus every entry whose key **or** cached
    /// result is a structural sub-term of `type_id`. Returns the number of
    /// entries removed.
    ///
    /// Re-evaluating a type under a different resolution mode is not enough to
    /// invalidate just its top-level result: the first (e.g. shallow/bounded)
    /// pass also cached results for the sub-terms it walked, and a later full
    /// pass would short-circuit to those stale sub-results. This transitive
    /// variant clears the type's reachable sub-evaluations in one pass so the
    /// re-evaluation recomputes the whole reachable closure.
    ///
    /// The reachable set is collected once (`collect_referenced_types`, which
    /// includes the root) and entries are tested by O(1) membership, so the
    /// sweep is `O(reachable(type_id)) + O(cache)` rather than re-walking per
    /// entry. The result side is matched with the same key-and-result discipline
    /// as [`Self::clear_type_evaluation_caches_for_def`]: an entry whose result
    /// embeds a reachable sub-term holds that sub-term's pre-re-evaluation form,
    /// so it is dropped too. Over-matching only forces a deterministic recompute;
    /// it never changes a result.
    ///
    /// Scope is the `env_eval_cache` only. Unlike the per-`DefId` sweep, this
    /// does not touch the narrowing `resolve_cache`/`contextual_resolve_cache`:
    /// those are invalidated through the def-keyed path and are not part of a
    /// per-type evaluation closure.
    pub(crate) fn invalidate_env_eval_reachable_from(&self, type_id: TypeId) -> usize {
        {
            let cache = self.env_eval_cache.borrow();
            if cache.is_empty() && cache.contextual_signature_normalization_len() == 0 {
                // Nothing to drop: skip the structural walk entirely (mirrors the
                // empty-cache early-out in `env_eval_cache_seed_entries`).
                return 0;
            }
        }
        let reachable = crate::query_boundaries::type_computation::core::collect_referenced_types(
            self.types, type_id,
        );
        let mut cache = self.env_eval_cache.borrow_mut();
        let removed = cache.invalidate_matching(|key, value| {
            reachable.contains(&key) || reachable.contains(&value.result)
        });
        let normalization_removed =
            cache.invalidate_contextual_signature_normalizations_matching(|key, value| {
                reachable.contains(&key) || reachable.contains(&value)
            });
        removed + normalization_removed
    }

    pub(crate) fn clear_type_evaluation_caches_for_def(&self, def_id: tsz_solver::DefId) {
        self.env_eval_cache.borrow_mut().invalidate_for_def(def_id);
        self.flow_shared
            .narrowing_cache
            .resolve_cache
            .borrow_mut()
            .retain(|&key, &mut value| {
                !self.type_mentions_def(key, def_id) && !self.type_mentions_def(value, def_id)
            });
        self.flow_shared
            .narrowing_cache
            .contextual_resolve_cache
            .borrow_mut()
            .retain(|&key, &mut value| {
                !self.type_mentions_def(key, def_id) && !self.type_mentions_def(value, def_id)
            });
    }

    pub(crate) fn persist_env_eval_cache_entries(&self, entries: Vec<(TypeId, TypeId)>) {
        use crate::query_boundaries::common::{contains_this_type, is_union_type};
        use crate::query_boundaries::state::type_environment::{
            contains_infer_types_db, contains_type_query_db, is_application_type,
        };

        if entries.is_empty() {
            return;
        }

        // Declaration files like react16.d.ts generate very large volumes of
        // transient evaluator entries. Persisting every intermediate entry
        // forces an expensive recursive `contains_infer_types_db` scan that can
        // cost more than the cache helps. Keep the top-level env-eval cache, but
        // skip bulk persistence for ambient declaration graphs.
        if self.is_declaration_file() {
            return;
        }

        // The drained evaluator cache is a speed-only intermediate memo. Keep
        // its persistence bounded and cheap: once the structural cap is crossed,
        // callers still use the authoritative top-level env-eval memo but stop
        // scanning and storing intermediate entries.
        let cap_disabled = env_eval_seed_cap_disabled();
        if !cap_disabled && self.env_eval_cache.borrow().len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP {
            return;
        }
        for (k, v) in entries {
            if !cap_disabled && self.env_eval_cache.borrow().len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP
            {
                break;
            }
            if k == v || k.is_intrinsic() {
                continue;
            }
            let key_contains_this = contains_this_type(self.types, k);
            if key_contains_this {
                continue;
            }
            let result_is_intrinsic = v.is_intrinsic();
            if result_is_intrinsic
                || (!contains_this_type(self.types, v)
                    && !contains_infer_types_db(self.types, v)
                    && !contains_type_query_db(self.types, v))
            {
                // Guard against union->non-union cache poisoning: when the
                // evaluator maps a union type to a non-union Application,
                // this indicates a failed or incomplete evaluation (e.g.,
                // an Application whose DefId wasn't yet resolved in the
                // TypeEnvironment). Caching such entries causes downstream
                // assignability checks to fail because union member checking
                // is bypassed.
                if !result_is_intrinsic
                    && is_union_type(self.types, k)
                    && !is_union_type(self.types, v)
                    && is_application_type(self.types, v)
                {
                    continue;
                }
                if !self.env_eval_cache.borrow().contains_key(k) {
                    let dependency_defs = self.env_eval_entry_dependency_defs(k, v);
                    self.env_eval_cache.borrow_mut().insert_if_absent(
                        k,
                        EnvEvalCacheEntry {
                            result: v,
                            depth_exceeded: false,
                        },
                        dependency_defs,
                    );
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn env_eval_cache_indexed_key_count_for_def(&self, def_id: DefId) -> usize {
        self.env_eval_cache
            .borrow()
            .indexed_key_count_for_def(def_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps(defs: &[DefId]) -> Box<[DefId]> {
        defs.to_vec().into_boxed_slice()
    }

    fn stamp(seed: u64) -> ContextualSignatureNormalizationStamp {
        (
            (seed, seed + 1, seed + 2, seed + 3),
            true,
            false,
            true,
            false,
        )
    }

    #[test]
    fn contextual_signature_normalization_cache_invalidates_by_def_dependency() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[DefId(7)]),
        );

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );
        cache.invalidate_for_def(DefId(8));
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );

        cache.invalidate_for_def(DefId(7));
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            None
        );
    }

    #[test]
    fn contextual_signature_normalization_cache_serves_only_matching_stamp() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[]),
        );

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            Some(TypeId(20))
        );
        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(2)),
            None
        );
    }

    #[test]
    fn contextual_signature_normalization_cache_invalidates_reachable_key_or_result() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[]),
        );
        cache.insert_contextual_signature_normalization(
            TypeId(30),
            stamp(1),
            TypeId(40),
            deps(&[]),
        );

        let removed =
            cache.invalidate_contextual_signature_normalizations_matching(|key, value| {
                key == TypeId(30) || value == TypeId(20)
            });

        assert_eq!(removed, 2);
        assert_eq!(cache.contextual_signature_normalization_len(), 0);
    }

    #[test]
    fn contextual_signature_normalization_cache_clear_drops_entries() {
        let mut cache = EnvEvalCache::default();
        cache.insert_contextual_signature_normalization(
            TypeId(10),
            stamp(1),
            TypeId(20),
            deps(&[DefId(7)]),
        );

        cache.clear();

        assert_eq!(
            cache.get_contextual_signature_normalization(TypeId(10), stamp(1)),
            None
        );
        assert_eq!(cache.contextual_signature_normalization_len(), 0);
    }
}
