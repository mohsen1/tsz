//! Explicit-annotation implicit-any deferral helpers.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    pub(crate) fn explicit_annotation_can_defer_implicit_any_context(
        &self,
        annotation_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            // An indexed access whose object is an unresolved-module import is
            // poisoned to `any` (see `get_type_from_type_node`); it cannot
            // supply contextual parameter types, so `tsc` reports the
            // implicit-`any` parameter diagnostics rather than deferring them.
            return !self.indexed_access_object_is_unresolved_import(node);
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            return matches!(
                self.resolve_identifier_symbol_in_type_position_without_tracking(
                    type_ref.type_name
                ),
                crate::symbol_resolver::TypeSymbolResolution::Type(_)
            );
        }
        false
    }
}
