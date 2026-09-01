use std::collections::HashSet;

use super::Checker;
use crate::semantics::types::{DeferredType, TypeId, TypeKind, TypeStore};

impl Checker<'_> {
    fn type_graph_contains(
        &self,
        mut pending: Vec<TypeId>,
        predicate: impl Fn(&TypeKind) -> bool,
    ) -> bool {
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if seen.insert(ty) {
                let kind = self.store.kind(ty);
                if predicate(kind) {
                    return true;
                }
                TypeStore::push_type_children(kind, &mut pending);
            }
        }
        false
    }

    /// Definitive caches may only retain graphs whose complete structure is
    /// itself definitive. A clean outer array/object/union must not conceal
    /// an error, invalid projection, or deferred query in a nested position.
    pub(super) fn is_cacheable_type(&self, ty: TypeId) -> bool {
        !self.type_graph_contains(vec![ty], |kind| {
            matches!(
                kind,
                TypeKind::Error | TypeKind::Invalid(_) | TypeKind::Deferred(_)
            )
        })
    }

    /// A force query is request-local when any symbolic operand is request-local.
    pub(super) fn deferred_result_is_query_local(&self, deferred: &DeferredType) -> bool {
        let mut pending = Vec::new();
        TypeStore::push_deferred_children(deferred, &mut pending);
        deferred.is_query_local()
            || self.type_graph_contains(
                pending,
                |kind| matches!(kind, TypeKind::Deferred(value) if value.is_query_local()),
            )
    }
}
