use tsz_solver::TypeId;

use super::{CheckerContext, EnvEvalCacheEntry};

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
        self.env_eval_cache.borrow().get(&type_id).copied()
    }

    pub(crate) fn env_eval_cache_seed_entries(&self) -> Vec<(TypeId, TypeId)> {
        let cache = self.env_eval_cache.borrow();
        if cache.is_empty() {
            return Vec::new();
        }
        if !env_eval_seed_cap_disabled() && cache.len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP {
            return Vec::new();
        }
        let mut entries = Vec::with_capacity(cache.len());
        for (&k, v) in cache.iter() {
            if k != v.result && !k.is_intrinsic() && !v.depth_exceeded {
                entries.push((k, v.result));
            }
        }
        entries
    }

    pub(crate) fn cache_env_eval_result(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        self.env_eval_cache.borrow_mut().insert(
            type_id,
            EnvEvalCacheEntry {
                result,
                depth_exceeded,
            },
        );
    }

    pub(crate) fn cache_env_eval_result_if_absent(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        self.env_eval_cache
            .borrow_mut()
            .entry(type_id)
            .or_insert(EnvEvalCacheEntry {
                result,
                depth_exceeded,
            });
    }

    pub(crate) fn clear_env_eval_cache(&self) {
        self.env_eval_cache.borrow_mut().clear();
    }

    pub(crate) fn clear_type_evaluation_caches_for_def(&self, def_id: tsz_solver::DefId) {
        self.env_eval_cache.borrow_mut().retain(|&key, value| {
            !self.type_mentions_def(key, def_id) && !self.type_mentions_def(value.result, def_id)
        });
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
        let mut cache = self.env_eval_cache.borrow_mut();
        if !cap_disabled && cache.len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP {
            return;
        }
        for (k, v) in entries {
            if !cap_disabled && cache.len() > ENV_EVAL_SEED_PERSIST_SOFT_CAP {
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
                cache.entry(k).or_insert(EnvEvalCacheEntry {
                    result: v,
                    depth_exceeded: false,
                });
            }
        }
    }
}
