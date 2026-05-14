use tsz_solver::TypeId;

use super::{CheckerContext, EnvEvalCacheEntry};

impl<'a> CheckerContext<'a> {
    pub(crate) fn lookup_env_eval_cache(&self, type_id: TypeId) -> Option<EnvEvalCacheEntry> {
        self.env_eval_cache.borrow().get(&type_id).copied()
    }

    pub(crate) fn env_eval_cache_seed_entries(&self) -> Vec<(TypeId, TypeId)> {
        let cache = self.env_eval_cache.borrow();
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

        let mut cache = self.env_eval_cache.borrow_mut();
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
                cache.entry(k).or_insert(EnvEvalCacheEntry {
                    result: v,
                    depth_exceeded: false,
                });
            }
        }
    }
}
