impl<'a> CheckerState<'a> {
    pub(crate) fn explicit_annotation_can_defer_implicit_any_context(
        &self,
        annotation_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            return true;
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
