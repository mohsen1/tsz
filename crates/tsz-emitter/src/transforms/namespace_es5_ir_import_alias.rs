use super::*;

impl<'a> NamespaceES5Transformer<'a> {
    pub(super) fn namespace_statement_erases_runtime(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return true;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                .arena
                .get_export_decl(member_node)
                .is_none_or(|export_data| {
                    self.namespace_statement_erases_runtime(export_data.export_clause)
                }),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::NAMED_EXPORTS =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl_at(member_idx)
                .is_none_or(|import| {
                    self.import_equals_uses_external_module_ref(member_idx)
                        || !self.import_equals_target_has_runtime_value(
                            member_idx,
                            import.module_specifier,
                        )
                }),
            _ => false,
        }
    }

    pub(super) fn import_equals_uses_external_module_ref(&self, import_idx: NodeIndex) -> bool {
        let Some(import) = self.arena.get_import_decl_at(import_idx) else {
            return false;
        };
        self.arena.get(import.module_specifier).is_some_and(|node| {
            node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                || node.kind == SyntaxKind::StringLiteral as u16
        })
    }
}
