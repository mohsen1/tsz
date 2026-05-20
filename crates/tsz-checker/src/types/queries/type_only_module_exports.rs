use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn symbol_has_runtime_value_declaration(&self, symbol: &tsz_binder::Symbol) -> bool {
        if !symbol.has_any_flags(
            symbol_flags::VARIABLE
                | symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::ENUM,
        ) || symbol.value_declaration.is_none()
        {
            return false;
        }

        self.ctx
            .arena
            .get(symbol.value_declaration)
            .is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::VARIABLE_DECLARATION
                        | syntax_kind_ext::FUNCTION_DECLARATION
                        | syntax_kind_ext::CLASS_DECLARATION
                        | syntax_kind_ext::ENUM_DECLARATION
                )
            })
    }

    pub(crate) fn identifier_is_type_only_module_exports_import_projection(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }

        if let Some(module_specifier) = symbol.import_module.as_deref() {
            let is_namespace_binding =
                symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*");
            if is_namespace_binding
                && self
                    .classify_cross_file_type_only_kind(module_specifier, "module.exports")
                    .is_some()
            {
                return true;
            }
        }

        let Some(decl_idx) = symbol.primary_declaration() else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return false;
        }
        let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node) else {
            return false;
        };
        let module_specifier = if let Some(module_node) =
            self.ctx.arena.get(import_decl.module_specifier)
            && module_node.kind == SyntaxKind::StringLiteral as u16
            && let Some(literal) = self.ctx.arena.get_literal(module_node)
        {
            Some(literal.text.clone())
        } else {
            self.get_require_module_specifier(import_decl.module_specifier)
        };
        module_specifier.as_deref().is_some_and(|module| {
            self.classify_cross_file_type_only_kind(module, "module.exports")
                .is_some()
        })
    }
}
