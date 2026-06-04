impl<'a> CheckerState<'a> {
    /// Update the stable flow cache for a symbol after flow analysis.
    /// If `is_stable` is true (flow returned the declared type), record the current
    /// flow node. If false (narrowing occurred), remove the entry.
    fn update_symbol_flow_confirmed(&self, idx: NodeIndex, declared_type: TypeId, is_stable: bool) {
        if let Some(flow_node) = self.ctx.binder.get_node_flow(idx)
            && let Some(sym_id) = self
                .ctx
                .binder
                .get_node_symbol(idx)
                .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        {
            let key = (sym_id, declared_type);
            if is_stable {
                self.ctx
                    .symbol_flow_confirmed
                    .borrow_mut()
                    .insert(key, flow_node);
            } else {
                self.ctx.symbol_flow_confirmed.borrow_mut().remove(&key);
            }
        }
    }

    pub(crate) fn is_keyword_type_used_as_value_position(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        if matches!(
            parent_node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION
                || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
        ) {
            return true;
        }

        // Recovery path: malformed value expressions like `number[]` are parsed
        // through ARRAY_TYPE wrappers, but still need TS2693 at the keyword.
        if parent_node.kind == syntax_kind_ext::ARRAY_TYPE {
            let Some(parent_ext) = self.ctx.arena.get_extended(parent) else {
                return false;
            };
            let grandparent = parent_ext.parent;
            if grandparent.is_none() {
                return false;
            }
            let Some(grandparent_node) = self.ctx.arena.get(grandparent) else {
                return false;
            };
            return matches!(
                grandparent_node.kind,
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::LABELED_STATEMENT
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION
                    || k == syntax_kind_ext::BINARY_EXPRESSION
                    || k == syntax_kind_ext::RETURN_STATEMENT
                    || k == syntax_kind_ext::VARIABLE_DECLARATION
                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
            );
        }

        false
    }

    /// Compute the type of a node (internal, not cached).
    ///
    /// This method first dispatches through `ExpressionChecker`. If the
    /// dispatcher returns [`crate::ExprCheckResult::Delegate`], we fall back
    /// to the full `CheckerState` implementation that has access to symbol
    /// resolution, contextual typing, and other complex type checking
    /// features. Delegation is control flow — it never appears as a `TypeId`.
    #[allow(dead_code)]
    fn compute_type_of_node_complex(&mut self, idx: NodeIndex) -> TypeId {
        self.compute_type_of_node_complex_with_request(idx, &crate::context::TypingRequest::NONE)
    }

    fn compute_type_of_node_complex_with_request(
        &mut self,
        idx: NodeIndex,
        request: &crate::context::TypingRequest,
    ) -> TypeId {
        use crate::dispatch::ExpressionDispatcher;

        let mut dispatcher = ExpressionDispatcher::new(self);
        dispatcher.dispatch_type_computation_with_request(idx, request)
    }

    // Type resolution, type analysis, type environment, and checking methods
    // are in type_resolution/, type_analysis/, type_environment/,
    // state_checking.rs, and state_checking_members/
}
