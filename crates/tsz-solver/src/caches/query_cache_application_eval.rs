//! Application-evaluation cache trait implementation for [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the file-size cap.
//! This is a child module of `query_cache`, so it keeps access to the cache's
//! private fields.

use super::{DefId, EvaluationCacheKey, QueryCache, TypeId, application_eval_index};
use crate::caches::db::{TypeApplicationEvalCache, TypeCompilerOptions};

impl TypeApplicationEvalCache for QueryCache<'_> {
    // #14345: delegate the project-wide instantiation cache to the interner
    // so query_db=Some passes share the same table the query_db=None callers read.
    fn lookup_proto_instantiation_cache(
        &self,
        key: &crate::caches::instantiation_cache::InstantiationCacheKey,
    ) -> Option<TypeId> {
        self.interner.proto_instantiation_memo(key)
    }

    fn insert_proto_instantiation_cache(
        &self,
        key: crate::caches::instantiation_cache::InstantiationCacheKey,
        result: TypeId,
    ) {
        self.interner.set_proto_instantiation_memo(key, result);
    }

    fn lookup_application_eval_cache(
        &self,
        def_id: DefId,
        args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.check_application_eval_cache((
            def_id,
            smallvec::SmallVec::from_slice(args),
            no_unchecked_indexed_access,
            self.exact_optional_property_types(),
        ))
    }

    fn insert_application_eval_cache(
        &self,
        def_id: DefId,
        args: &[TypeId],
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        QueryCache::insert_application_eval_cache(
            self,
            (
                def_id,
                smallvec::SmallVec::from_slice(args),
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ),
            result,
        );
    }

    fn invalidate_application_eval_cache_for_def(&self, def_id: DefId) {
        application_eval_index::invalidate_for_def(
            self.interner,
            &self.application_eval_dependency_index,
            &self.application_eval_cache,
            def_id,
        );
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
        {
            shared.invalidate_application_eval_cache_for_def(self.interner, def_id);
        }
    }

    /// Nested eval-memo read for plain evaluators (issue #13097).
    /// Same layered lookup the top-level boundary uses.
    fn lookup_eval_memo(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.lookup_eval_cache_layers(EvaluationCacheKey::new(
            type_id,
            no_unchecked_indexed_access,
            self.exact_optional_property_types(),
        ))
    }

    /// Write-through eval-memo store for plain evaluators (issue #13097).
    /// First write wins, matching the top-level boundary drain.
    fn insert_eval_memo(&self, type_id: TypeId, no_unchecked_indexed_access: bool, result: TypeId) {
        let key = EvaluationCacheKey::new(
            type_id,
            no_unchecked_indexed_access,
            self.exact_optional_property_types(),
        );
        self.eval_cache.borrow_mut().entry(key).or_insert(result);
        if let Some(shared) = self.shared {
            shared.eval_cache.entry(key).or_insert(result);
        }
    }

    fn lookup_closed_eval_cache(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.closed_eval_cache
            .borrow()
            .get(&EvaluationCacheKey::new(
                type_id,
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ))
            .copied()
    }

    fn insert_closed_eval_cache(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        self.closed_eval_cache.borrow_mut().insert(
            EvaluationCacheKey::new(
                type_id,
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ),
            result,
        );
    }

    fn lookup_conditional_branch_verdict(
        &self,
        check: TypeId,
        extends: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<bool> {
        self.conditional_branch_verdict_cache
            .borrow()
            .get(&(check, extends, no_unchecked_indexed_access))
            .copied()
    }

    fn insert_conditional_branch_verdict(
        &self,
        check: TypeId,
        extends: TypeId,
        no_unchecked_indexed_access: bool,
        verdict: bool,
    ) {
        self.conditional_branch_verdict_cache
            .borrow_mut()
            .insert((check, extends, no_unchecked_indexed_access), verdict);
    }
}
