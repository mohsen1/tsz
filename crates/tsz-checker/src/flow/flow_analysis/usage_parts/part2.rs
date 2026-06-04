impl<'a> CheckerState<'a> {
    /// Check if a flow assignment's AST node targets the given symbol.
    fn flow_assignment_targets_symbol(&self, node_idx: NodeIndex, target_sym: SymbolId) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        // Binary expression with assignment operator (e.g., `x = value`)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(node)
        {
            return self.node_resolves_to_symbol(bin.left, target_sym);
        }
        // Variable declaration (e.g., `let x = value` — though these have initializers)
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.ctx.arena.get_variable_declaration(node)
        {
            return self.node_resolves_to_symbol(decl.name, target_sym);
        }
        // Prefix/postfix unary (e.g., `++x`, `x--`)
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(node)
        {
            return self.node_resolves_to_symbol(unary.operand, target_sym);
        }
        // For-in/for-of statement initializer
        if (node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && let Some(for_data) = self.ctx.arena.get_for_in_of(node)
        {
            return self.node_resolves_to_symbol(for_data.initializer, target_sym);
        }
        false
    }

    /// Check if a node resolves to the given symbol via the binder.
    fn node_resolves_to_symbol(&self, idx: NodeIndex, target_sym: SymbolId) -> bool {
        if let Some(sym) = self.resolve_for_of_header_expression_symbol(idx) {
            return sym == target_sym;
        }
        if let Some(sym) = self.ctx.binder.get_node_symbol(idx) {
            return sym == target_sym;
        }
        if let Some(sym) = self.resolve_identifier_symbol_without_tracking(idx) {
            return sym == target_sym;
        }
        false
    }
}
