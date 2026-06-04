use super::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::TypeId;

/// Operation-local memo table statistics for [`TypeEvaluator`].
///
/// Owner: one evaluator request. The caches are dropped with the evaluator and
/// are never shared across resolver, substitution, or compiler-option modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeEvaluatorCacheStatistics {
    /// Entries in the conditional subtype memo keyed by `(check_type, extends_type)`.
    pub conditional_subtype_entries: usize,
    /// Entries in the `contains infer` predicate memo keyed by `TypeId`.
    pub contains_infer_entries: usize,
    /// Entries in the infer-match expansion memo keyed by source/pattern `TypeId`.
    pub infer_match_eval_entries: usize,
    estimated_size_bytes: usize,
}

impl TypeEvaluatorCacheStatistics {
    /// Estimated heap bytes owned by the evaluator memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Return entry and size accounting for this evaluator's operation-local caches.
    #[must_use]
    pub fn cache_statistics(&self) -> TypeEvaluatorCacheStatistics {
        let conditional_subtype_entries = self.conditional_subtype_cache.len();
        let contains_infer_entries = self.contains_infer_cache.len();
        let infer_match_eval_entries = self.infer_match_eval_cache.borrow().len();
        let type_evaluator_cache_estimated_size_bytes = conditional_subtype_entries
            .saturating_mul(std::mem::size_of::<((TypeId, TypeId), bool)>())
            .saturating_add(
                contains_infer_entries.saturating_mul(std::mem::size_of::<(TypeId, bool)>()),
            )
            .saturating_add(
                infer_match_eval_entries.saturating_mul(std::mem::size_of::<(TypeId, TypeId)>()),
            );

        TypeEvaluatorCacheStatistics {
            conditional_subtype_entries,
            contains_infer_entries,
            infer_match_eval_entries,
            estimated_size_bytes: type_evaluator_cache_estimated_size_bytes,
        }
    }

    /// PERF: Look up a cached subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cached_conditional_subtype(
        &self,
        check: TypeId,
        extends: TypeId,
    ) -> Option<bool> {
        self.conditional_subtype_cache
            .get(&(check, extends))
            .copied()
    }

    /// PERF: Cache a subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cache_conditional_subtype(
        &mut self,
        check: TypeId,
        extends: TypeId,
        result: bool,
    ) {
        self.conditional_subtype_cache
            .insert((check, extends), result);
    }

    /// PERF: Look up whether a type contains `infer`.
    #[inline]
    pub(crate) fn cached_contains_infer(&self, type_id: TypeId) -> Option<bool> {
        self.contains_infer_cache.get(&type_id).copied()
    }

    /// PERF: Cache whether a type contains `infer`.
    #[inline]
    pub(crate) fn cache_contains_infer(&mut self, type_id: TypeId, result: bool) {
        self.contains_infer_cache.insert(type_id, result);
    }

    /// PERF: Look up an infer-match-only evaluated type.
    #[inline]
    pub(crate) fn cached_infer_match_eval(&self, type_id: TypeId) -> Option<TypeId> {
        self.infer_match_eval_cache.borrow().get(&type_id).copied()
    }

    /// PERF: Cache an infer-match-only evaluated type.
    #[inline]
    pub(crate) fn cache_infer_match_eval(&self, type_id: TypeId, result: TypeId) {
        self.infer_match_eval_cache
            .borrow_mut()
            .insert(type_id, result);
    }
}
