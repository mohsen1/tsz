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

    /// The `require(...)` module specifier for an object binding pattern's
    /// initializer, when the target resolves to a real CommonJS export
    /// surface (not an untyped/`any` module).
    fn commonjs_require_destructure_module_specifier(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<String> {
        if !self.is_js_file() {
            return None;
        }
        let var_decl_idx = self
            .ctx
            .arena
            .get_extended(pattern_idx)
            .map(|ext| ext.parent)?;
        let var_decl_node = self.ctx.arena.get(var_decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(var_decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let module_specifier = self.get_require_module_specifier(var_decl.initializer)?;
        let surface = self.resolve_js_export_surface_for_module(
            &module_specifier,
            Some(self.ctx.current_file_idx),
        )?;
        surface.has_commonjs_exports.then_some(module_specifier)
    }

    /// `tsc` types a JS `require()` call's result as a module instance type,
    /// so a destructured property it lacks is diagnosed the same way a named
    /// `import { p } from "mod"` miss is — TS2305 ("has no exported
    /// member"), not the generic TS2339 the destructuring property-not-found
    /// path otherwise emits.
    ///
    /// Reports TS2305 and returns `true` when `pattern_idx`'s initializer is
    /// a `require()` into a real CommonJS export surface; otherwise reports
    /// nothing and returns `false` so the caller falls back to TS2339.
    pub(crate) fn report_require_destructure_missing_export(
        &mut self,
        pattern_idx: NodeIndex,
        error_node: NodeIndex,
        prop_name: &str,
    ) -> bool {
        let Some(module_specifier) =
            self.commonjs_require_destructure_module_specifier(pattern_idx)
        else {
            return false;
        };
        let quoted_module = format!("\"{module_specifier}\"");
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
            &[&quoted_module, prop_name],
        );
        self.error_at_node(
            error_node,
            &message,
            crate::diagnostics::diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
        );
        true
    }
}
