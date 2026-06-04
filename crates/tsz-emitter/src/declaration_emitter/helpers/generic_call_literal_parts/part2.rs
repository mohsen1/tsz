impl<'a> DeclarationEmitter<'a> {
    fn call_expression_has_generic_callee(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        if self.function_expression_has_type_parameters(call.expression) {
            return true;
        }

        if call
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return true;
        }

        if self
            .arena
            .get(call.expression)
            .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            return true;
        }

        let Some(sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            func.type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
                .then_some(())
        })
        .is_some()
    }

    fn function_expression_has_type_parameters(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        self.arena
            .get_function(expr_node)
            .and_then(|func| func.type_parameters.as_ref())
            .is_some_and(|params| !params.nodes.is_empty())
    }
}
