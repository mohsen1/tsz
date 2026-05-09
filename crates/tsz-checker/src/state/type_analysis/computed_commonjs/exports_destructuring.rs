use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn commonjs_destructured_named_export_exists(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(expr_idx) else {
            return false;
        };
        let Some(symbol) = self
            .get_symbol_globally(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .cloned()
        else {
            return false;
        };

        let value_decl = symbol.value_declaration;
        if !value_decl.is_some() {
            return false;
        }
        let Some(value_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        let be_idx = if value_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_extended(value_decl)
                .map(|ext| ext.parent)
                .filter(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        } else if value_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            value_decl
        } else {
            return false;
        };
        let Some(be_node) = self.ctx.arena.get(be_idx) else {
            return false;
        };
        if be_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return false;
        }
        let Some(be_data) = self.ctx.arena.get_binding_element(be_node) else {
            return false;
        };

        let Some(pat_idx) = self.ctx.arena.get_extended(be_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(pat_node) = self.ctx.arena.get(pat_idx) else {
            return false;
        };
        if pat_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
        }

        let Some(var_decl_idx) = self.ctx.arena.get_extended(pat_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
            return false;
        };
        if !var_decl.initializer.is_some() {
            return false;
        }

        let Some(module_specifier) = self.get_require_module_specifier(var_decl.initializer) else {
            return false;
        };
        let export_name = if be_data.property_name.is_some() {
            self.get_identifier_text_from_idx(be_data.property_name)
        } else {
            Some(symbol.escaped_name)
        };
        let Some(export_name) = export_name else {
            return false;
        };

        self.js_export_surface_has_export(
            &module_specifier,
            &export_name,
            Some(self.ctx.current_file_idx),
        )
    }
}
