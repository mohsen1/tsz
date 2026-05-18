use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

use super::{CheckerContext, EnvEvalCacheEntry};

impl<'a> CheckerContext<'a> {
    fn env_eval_entry_deps(&self, key: TypeId, result: TypeId) -> FxHashSet<tsz_solver::DefId> {
        let mut deps = FxHashSet::default();
        deps.extend(crate::query_boundaries::common::collect_lazy_def_ids(
            self.types, key,
        ));
        deps.extend(crate::query_boundaries::common::collect_lazy_def_ids(
            self.types, result,
        ));
        deps
    }

    fn remove_env_eval_index_for_key(&self, key: TypeId) {
        let Some(deps) = self.env_eval_cache.entry_deps.borrow_mut().remove(&key) else {
            return;
        };
        let mut empty_defs = Vec::new();
        {
            let mut index = self.env_eval_cache.def_index.borrow_mut();
            for def_id in deps {
                if let Some(keys) = index.get_mut(&def_id) {
                    keys.remove(&key);
                    if keys.is_empty() {
                        empty_defs.push(def_id);
                    }
                }
            }
            for def_id in empty_defs {
                index.remove(&def_id);
            }
        }
    }

    fn index_env_eval_cache_entry(&self, key: TypeId, result: TypeId) {
        self.remove_env_eval_index_for_key(key);

        let deps = self.env_eval_entry_deps(key, result);
        if deps.is_empty() {
            return;
        }

        {
            let mut index = self.env_eval_cache.def_index.borrow_mut();
            for &def_id in &deps {
                index.entry(def_id).or_default().insert(key);
            }
        }
        self.env_eval_cache
            .entry_deps
            .borrow_mut()
            .insert(key, deps);
    }

    pub(crate) fn lookup_env_eval_cache(&self, type_id: TypeId) -> Option<EnvEvalCacheEntry> {
        self.env_eval_cache.entries.borrow().get(&type_id).copied()
    }

    pub(crate) fn env_eval_cache_seed_entries(&self) -> Vec<(TypeId, TypeId)> {
        let cache = self.env_eval_cache.entries.borrow();
        if cache.is_empty() {
            return Vec::new();
        }
        cache.iter().map(|(&k, v)| (k, v.result)).collect()
    }

    pub(crate) fn cache_env_eval_result(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        self.env_eval_cache.entries.borrow_mut().insert(
            type_id,
            EnvEvalCacheEntry {
                result,
                depth_exceeded,
            },
        );
        self.index_env_eval_cache_entry(type_id, result);
    }

    pub(crate) fn cache_env_eval_result_if_absent(
        &self,
        type_id: TypeId,
        result: TypeId,
        depth_exceeded: bool,
    ) {
        let inserted = {
            let mut cache = self.env_eval_cache.entries.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(type_id) {
                entry.insert(EnvEvalCacheEntry {
                    result,
                    depth_exceeded,
                });
                true
            } else {
                false
            }
        };
        if inserted {
            self.index_env_eval_cache_entry(type_id, result);
        }
    }

    pub(crate) fn clear_env_eval_cache(&self) {
        self.env_eval_cache.entries.borrow_mut().clear();
        self.env_eval_cache.def_index.borrow_mut().clear();
        self.env_eval_cache.entry_deps.borrow_mut().clear();
    }

    pub(crate) fn clear_type_evaluation_caches_for_def(&self, def_id: tsz_solver::DefId) {
        let env_eval_keys = self
            .env_eval_cache
            .def_index
            .borrow_mut()
            .remove(&def_id)
            .unwrap_or_default();
        if !env_eval_keys.is_empty() {
            {
                let mut cache = self.env_eval_cache.entries.borrow_mut();
                for key in &env_eval_keys {
                    cache.remove(key);
                }
            }
            for key in env_eval_keys {
                self.remove_env_eval_index_for_key(key);
            }
        }
        self.narrowing_cache
            .invalidate_resolve_cache_for_def(def_id);
        self.narrowing_cache
            .invalidate_contextual_resolve_cache_for_def(def_id);
    }

    pub(crate) fn persist_env_eval_cache_entries(&self, entries: Vec<(TypeId, TypeId)>) {
        use crate::query_boundaries::common::{contains_this_type, is_union_type};
        use crate::query_boundaries::state::type_environment::{
            contains_infer_types_db, contains_type_query_db, is_application_type,
        };

        // Declaration files like react16.d.ts generate very large volumes of
        // transient evaluator entries. Persisting every intermediate entry
        // forces an expensive recursive `contains_infer_types_db` scan that can
        // cost more than the cache helps. Keep the top-level env-eval cache, but
        // skip bulk persistence for ambient declaration graphs.
        if self.is_declaration_file() {
            return;
        }

        let mut inserted_entries = Vec::new();
        let mut cache = self.env_eval_cache.entries.borrow_mut();
        for (k, v) in entries {
            if k != v
                && !k.is_intrinsic()
                && !contains_this_type(self.types, k)
                && !contains_this_type(self.types, v)
                && !contains_infer_types_db(self.types, v)
                && !contains_type_query_db(self.types, v)
            {
                // Guard against union->non-union cache poisoning: when the
                // evaluator maps a union type to a non-union Application,
                // this indicates a failed or incomplete evaluation (e.g.,
                // an Application whose DefId wasn't yet resolved in the
                // TypeEnvironment). Caching such entries causes downstream
                // assignability checks to fail because union member checking
                // is bypassed.
                if is_union_type(self.types, k)
                    && !is_union_type(self.types, v)
                    && is_application_type(self.types, v)
                {
                    continue;
                }
                if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(k) {
                    entry.insert(EnvEvalCacheEntry {
                        result: v,
                        depth_exceeded: false,
                    });
                    inserted_entries.push((k, v));
                }
            }
        }
        drop(cache);
        for (k, v) in inserted_entries {
            self.index_env_eval_cache_entry(k, v);
        }
    }
}
