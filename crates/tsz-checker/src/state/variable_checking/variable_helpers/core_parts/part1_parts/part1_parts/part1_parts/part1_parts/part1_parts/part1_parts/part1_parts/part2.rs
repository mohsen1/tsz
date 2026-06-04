impl<'a> CheckerState<'a> {
    /// `expr satisfies T`, `expr as T`, and `<T>expr` should be treated as
    /// opaque wrappers for variable-declaration assignability elaboration.
    /// tsc anchors the resulting TS2322 at the variable binding with the
    /// outer assignment types instead of drilling into the wrapped
    /// expression's inner structure.
    ///
    /// Excludes `as const` — const assertions don't change the structural
    /// shape of the inner expression (they only freeze literals and add
    /// readonly), so any mismatch with the declared type is a per-property
    /// issue inside the literal. tsc drills into the property in that case.
    pub(crate) fn initializer_is_type_assertion(&self, init_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        self.ctx.arena.get(init_idx).is_some_and(|n| {
            (n.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || n.kind == syntax_kind_ext::AS_EXPRESSION
                || n.kind == syntax_kind_ext::TYPE_ASSERTION)
                && !self.is_const_assertion_node(init_idx)
        })
    }

    pub(crate) fn expression_is_object_assign_call(&self, expr_idx: NodeIndex) -> bool {
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::CALL_EXPRESSION
                && self.ctx.arena.get_call_expr(node).is_some_and(|call| {
                    self.ctx
                        .arena
                        .get(call.expression)
                        .and_then(|callee| self.ctx.arena.get_access_expr(callee))
                        .is_some_and(|access| {
                            self.identifier_resolves_to_proven_lib_global(
                                access.expression,
                                "Object",
                            ) && self.ctx.arena.get_identifier_text(access.name_or_argument)
                                == Some("assign")
                        })
                })
        })
    }

    pub(crate) fn first_non_portable_object_assign_object_literal_reference(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(expr_node)?;
        if !self.expression_is_object_assign_call(expr_idx) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        for &arg_idx in &args.nodes {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            let Some(literal) = self.ctx.arena.get_literal_expr(arg_node) else {
                continue;
            };
            for &member_idx in &literal.elements.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let value_idx = self
                    .ctx
                    .arena
                    .get_shorthand_property(member_node)
                    .map(|property| property.name)
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_property_assignment(member_node)
                            .map(|property| property.initializer)
                    });
                let Some(value_idx) = value_idx else {
                    continue;
                };
                if self.expression_resolves_to_exported_value(value_idx) {
                    continue;
                }
                let value_type = self.get_type_of_node(value_idx);
                let resolved_type = self.resolve_lazy_type(value_type);
                if let Some(reference) = self
                    .first_non_portable_type_reference(value_type)
                    .or_else(|| self.first_non_portable_type_reference(resolved_type))
                {
                    return Some(reference);
                }
            }
        }
        None
    }

    fn expression_resolves_to_exported_value(&self, expr_idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(expr_idx) else {
            return false;
        };
        self.ctx.binder.symbols.get(sym_id).is_some_and(|symbol| {
            symbol.is_exported || symbol.has_any_flags(symbol_flags::EXPORT_VALUE)
        })
    }
}
