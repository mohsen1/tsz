use std::collections::HashSet;

use super::Checker;
use crate::semantics::types::{TypeId, TypeKind, TypeStore};

impl Checker<'_> {
    /// Definitive caches may only retain graphs whose complete structure is
    /// itself definitive. A clean outer array/object/union must not conceal
    /// an error, invalid projection, or deferred query in a nested position.
    pub(super) fn is_cacheable_type(&self, ty: TypeId) -> bool {
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            let kind = self.store.kind(ty);
            if matches!(
                kind,
                TypeKind::Error | TypeKind::Invalid(_) | TypeKind::Deferred(_)
            ) {
                return false;
            }
            TypeStore::push_type_children(kind, &mut pending);
        }
        true
    }
}
