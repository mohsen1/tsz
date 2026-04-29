use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn assignment_target_is_control_flow_typed_any_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }

        let mut decl_idx = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(parent_idx) = self.ctx.arena.parent_of(decl_idx)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_idx = parent_idx;
            decl_node = parent_node;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || self.ctx.arena.is_in_ambient_context(decl_idx)
        {
            return false;
        }

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some()
            || self.jsdoc_type_annotation_for_node(decl_idx).is_some()
            || self.ctx.arena.is_const_variable_declaration(decl_idx)
            || self
                .ctx
                .arena
                .get(var_decl.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .is_none()
        {
            return false;
        }

        if var_decl.initializer.is_none() {
            return true;
        }
        let initializer = self.ctx.arena.skip_parenthesized(var_decl.initializer);
        self.ctx.arena.get(initializer).is_some_and(|node| {
            node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
                || node.kind == tsz_scanner::SyntaxKind::UndefinedKeyword as u16
                || self
                    .ctx
                    .arena
                    .get_identifier(node)
                    .is_some_and(|ident| ident.escaped_text == "undefined")
        })
    }
}
