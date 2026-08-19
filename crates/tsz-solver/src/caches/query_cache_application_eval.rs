//! Application-evaluation cache trait implementation for [`QueryCache`].
//!
//! Split out of `query_cache.rs` to keep that shard under the file-size cap.
//! This is a child module of `query_cache`, so it keeps access to the cache's
//! private fields.

use super::{
    DefId, EvaluationCacheKey, QueryCache, TypeId, application_eval_index, eval_dependency_index,
};
use crate::caches::db::{TypeApplicationEvalCache, TypeCompilerOptions};

impl TypeApplicationEvalCache for QueryCache<'_> {
    // Provisional class-instance registry (#16055): shared on the interner so
    // every database view observes the same registrations.
    fn provisional_class_instance(
        &self,
        type_id: TypeId,
    ) -> Option<(DefId, std::sync::Arc<[crate::types::TypeParamInfo]>)> {
        self.interner.provisional_class_instance(type_id)
    }

    fn register_provisional_class_instance(
        &self,
        type_id: TypeId,
        def_id: DefId,
        params: std::sync::Arc<[crate::types::TypeParamInfo]>,
    ) {
        self.interner
            .register_provisional_class_instance(type_id, def_id, params);
    }

    fn unregister_provisional_class_instances_for_def(&self, def_id: DefId) {
        self.interner
            .unregister_provisional_class_instances_for_def(def_id);
    }

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
            &self.application_eval_dependency_index,
            &self.application_eval_cache,
            def_id,
        );
        eval_dependency_index::invalidate_for_def(
            &self.eval_dependency_index,
            &self.eval_cache,
            def_id,
        );
        eval_dependency_index::invalidate_for_def(
            &self.closed_eval_dependency_index,
            &self.closed_eval_cache,
            def_id,
        );
        if let Some(shared) = self.shared {
            shared.invalidate_application_eval_cache_for_def(
                self.interner,
                self.definition_store(),
                def_id,
            );
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
        self.insert_eval_cache_entry_if_absent(key, result);
        if let Some(shared) = self.shared {
            shared.insert_eval_cache_if_absent(self.interner, self.definition_store(), key, result);
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
        self.insert_closed_eval_cache_entry(
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
        exact_optional_property_types: bool,
    ) -> Option<bool> {
        self.conditional_branch_verdict_cache
            .borrow()
            .get(&(
                check,
                extends,
                no_unchecked_indexed_access,
                exact_optional_property_types,
            ))
            .copied()
    }

    fn insert_conditional_branch_verdict(
        &self,
        check: TypeId,
        extends: TypeId,
        no_unchecked_indexed_access: bool,
        exact_optional_property_types: bool,
        verdict: bool,
    ) {
        self.conditional_branch_verdict_cache.borrow_mut().insert(
            (
                check,
                extends,
                no_unchecked_indexed_access,
                exact_optional_property_types,
            ),
            verdict,
        );
    }

    fn lookup_permissive_false_branch_verdict(
        &self,
        check: TypeId,
        extends: TypeId,
        no_unchecked_indexed_access: bool,
        exact_optional_property_types: bool,
    ) -> Option<bool> {
        self.permissive_false_branch_cache
            .borrow()
            .get(&(
                check,
                extends,
                no_unchecked_indexed_access,
                exact_optional_property_types,
            ))
            .copied()
    }

    fn insert_permissive_false_branch_verdict(
        &self,
        check: TypeId,
        extends: TypeId,
        no_unchecked_indexed_access: bool,
        exact_optional_property_types: bool,
        verdict: bool,
    ) {
        self.permissive_false_branch_cache.borrow_mut().insert(
            (
                check,
                extends,
                no_unchecked_indexed_access,
                exact_optional_property_types,
            ),
            verdict,
        );
    }
}
