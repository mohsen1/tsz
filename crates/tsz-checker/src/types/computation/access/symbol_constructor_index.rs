use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn shadowed_symbol_constructor_member_index(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(base_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        let Some(base_ident) = self.ctx.arena.get_identifier(base_node) else {
            return false;
        };
        if base_ident.escaped_text != "Symbol"
            || self.identifier_resolves_to_unshadowed_global(access.expression, "Symbol")
        {
            return false;
        }

        let Some(base_sym_id) = self.resolve_identifier_symbol_without_tracking(access.expression)
        else {
            return false;
        };
        let Some(type_annotation) =
            crate::types_domain::window_global_this_annotation::declared_type_annotation_for_symbol(
                &self.ctx,
                base_sym_id,
            )
        else {
            return false;
        };
        self.type_annotation_resolves_to_actual_lib_symbol_constructor(type_annotation)
    }

    fn type_annotation_resolves_to_actual_lib_symbol_constructor(
        &self,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return false;
        };
        if self.ctx.arena.get_identifier_text(type_ref.type_name) != Some("SymbolConstructor") {
            return false;
        }
        let Some(sym_id) = crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
            "SymbolConstructor",
            self.ctx.binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        ) else {
            return false;
        };
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .is_some_and(|symbol| {
                symbol.escaped_name == "SymbolConstructor"
                    && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                        || self.ctx.symbol_is_from_lib(sym_id))
            })
    }
}
