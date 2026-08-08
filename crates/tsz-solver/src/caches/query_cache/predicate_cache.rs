//! `TypePredicateCache` implementation for `QueryCache`.
//!
//! Each method delegates to the interned type-predicate caches on the shared
//! `TypeInterner`. Extracted from `query_cache.rs` to keep that file under its
//! size ratchet; behavior is unchanged.

use super::QueryCache;
use crate::caches::db::TypePredicateCache;
use crate::types::TypeId;

impl TypePredicateCache for QueryCache<'_> {
    fn contains_this_type_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_this_type_cached(type_id)
    }

    fn set_contains_this_type_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_this_type_cache(type_id, result);
    }

    fn contains_infer_types_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_infer_types_cached(type_id)
    }

    fn set_contains_infer_types_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_infer_types_cache(type_id, result);
    }

    fn contains_type_query_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_type_query_cached(type_id)
    }

    fn set_contains_type_query_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_type_query_cache(type_id, result);
    }

    fn contains_type_query_full_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_type_query_full_cached(type_id)
    }

    fn set_contains_type_query_full_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_type_query_full_cache(type_id, result);
    }

    fn contains_never_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_never_cached(type_id)
    }

    fn set_contains_never_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_never_cache(type_id, result);
    }

    fn contains_error_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_error_cached(type_id)
    }

    fn set_contains_error_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_error_cache(type_id, result);
    }

    fn contains_free_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_free_type_params_cached(type_id)
    }

    fn set_contains_free_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_free_type_params_cache(type_id, result);
    }

    fn contains_extractable_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner
            .contains_extractable_type_params_cached(type_id)
    }

    fn set_contains_extractable_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_extractable_type_params_cache(type_id, result);
    }

    fn contains_free_infer_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_free_infer_cached(type_id)
    }

    fn set_contains_free_infer_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_free_infer_cache(type_id, result);
    }

    fn contains_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_type_params_cached(type_id)
    }

    fn set_contains_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_type_params_cache(type_id, result);
    }

    fn contains_lazy_or_recursive_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_lazy_or_recursive_cached(type_id)
    }

    fn set_contains_lazy_or_recursive_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_lazy_or_recursive_cache(type_id, result);
    }

    fn contains_unresolved_application_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner
            .contains_unresolved_application_cached(type_id)
    }

    fn set_contains_unresolved_application_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_unresolved_application_cache(type_id, result);
    }

    fn contains_resolver_dependent_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_resolver_dependent_cached(type_id)
    }

    fn set_contains_resolver_dependent_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_resolver_dependent_cache(type_id, result);
    }

    fn structurally_eval_inert_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.structurally_eval_inert_cached(type_id)
    }

    fn set_structurally_eval_inert_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_structurally_eval_inert_cache(type_id, result);
    }

    fn contains_conditional_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_conditional_cached(type_id)
    }

    fn set_contains_conditional_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_conditional_cache(type_id, result);
    }

    fn contains_param_or_infer_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_param_or_infer_root_cached(type_id)
    }

    fn set_contains_param_or_infer_root_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_param_or_infer_root_cache(type_id, result);
    }

    fn contains_generic_params_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_generic_params_root_cached(type_id)
    }

    fn set_contains_generic_params_root_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_generic_params_root_cache(type_id, result);
    }

    fn is_generic_with_union_constraint_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner
            .is_generic_with_union_constraint_cached(type_id)
    }

    fn set_is_generic_with_union_constraint_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_is_generic_with_union_constraint_cache(type_id, result);
    }

    fn is_generic_without_nullable_constraint_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner
            .is_generic_without_nullable_constraint_cached(type_id)
    }

    fn set_is_generic_without_nullable_constraint_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_is_generic_without_nullable_constraint_cache(type_id, result);
    }

    fn eval_contains_infer_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.eval_contains_infer_cached(type_id)
    }

    fn set_eval_contains_infer_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_eval_contains_infer_cache(type_id, result);
    }

    fn contains_file_relative_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_file_relative_cached(type_id)
    }

    fn set_contains_file_relative_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_file_relative_cache(type_id, result);
    }
}
