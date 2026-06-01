use super::super::DeclarationEmitter;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    /// Resolve a call expression to the canonical callee symbol used for
    /// emitter-side declaration lookups, following the same portability and
    /// import-aliasing chain as the rest of this module. Also returns the
    /// resolved import module specifier when the callee crosses a module
    /// boundary, since several callers need both.
    pub(in crate::declaration_emitter) fn resolve_call_expression_callee_symbol(
        &self,
        callee_expr: NodeIndex,
        raw_sym_id: SymbolId,
        binder: &BinderState,
    ) -> (SymbolId, Option<String>) {
        let imported_module = self
            .imported_value_module_specifier(raw_sym_id, binder)
            .or_else(|| self.imported_value_module_specifier_from_syntax(callee_expr));
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .or_else(|| {
                imported_module.as_deref().and_then(|module_specifier| {
                    self.imported_value_export_symbol_from_syntax(
                        callee_expr,
                        module_specifier,
                        binder,
                    )
                })
            })
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        (sym_id, imported_module)
    }

    /// Returns true iff the callee's declared return type is a bare reference
    /// to one of its own type parameters, for example `<T>(x: T): T`. Composed
    /// returns like `` `${T}` ``, `T | undefined`, `T[]`, `{ v: T }`, or
    /// `Promise<T>` return false; the initializer form is only safe when the
    /// consumer can recover the result by re-inferring the type parameter from
    /// the literal argument.
    ///
    /// The `type_arguments.is_some_and(...)` guard rejects `T<X>` shapes: a
    /// bare type parameter cannot syntactically carry type arguments, so a
    /// `TypeReference` that does is necessarily an alias or generic, not the
    /// identity reference we accept.
    pub(in crate::declaration_emitter) fn call_expression_returns_bare_type_parameter_reference(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(raw_sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let (sym_id, _imported_module) =
            self.resolve_call_expression_callee_symbol(call.expression, raw_sym_id, binder);

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let return_idx = callable.type_annotation.into_option()?;
            let type_params = callable.type_parameters?;
            let return_node = source_arena.get(source_arena.skip_parenthesized(return_idx))?;
            if return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return Some(false);
            }
            let type_ref = source_arena.get_type_ref(return_node)?;
            if type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|ta| !ta.nodes.is_empty())
            {
                return Some(false);
            }
            let return_name = self.identifier_text_from_arena(source_arena, type_ref.type_name)?;
            let matched = type_params.nodes.iter().any(|&param_idx| {
                source_arena
                    .get(param_idx)
                    .and_then(|n| source_arena.get_type_parameter(n))
                    .and_then(|tp| self.identifier_text_from_arena(source_arena, tp.name))
                    .is_some_and(|name| name == return_name)
            });
            Some(matched)
        })
        .unwrap_or(false)
    }
}
