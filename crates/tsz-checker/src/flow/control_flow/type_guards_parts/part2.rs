impl<'a> FlowAnalyzer<'a> {
    /// Find the parameter whose declaration owns the symbol referenced by
    /// `target`. Returns `(index, name)` on a match. Identifiers that don't
    /// resolve, that resolve to a non-parameter, or whose symbol does not match
    /// any of the supplied parameter declarations are rejected.
    fn match_guard_target_to_parameter(
        &self,
        target: NodeIndex,
        params_list: &[NodeIndex],
        params: &[ParamInfo],
    ) -> Option<(usize, Atom)> {
        let target = self.skip_parenthesized(target);
        let target_node = self.arena.get(target)?;
        if target_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym = self.binder.resolve_identifier(self.arena, target)?;
        // `params_list` includes every parameter declaration (including a
        // `this` parameter when present); `params` excludes the synthetic
        // `this` parameter. Walk both in lockstep, advancing the `params`
        // cursor only for non-`this` declarations so the returned index always
        // refers to the correct `ParamInfo`.
        let mut info_idx = 0usize;
        for &param_idx in params_list {
            if self.is_this_parameter_decl(param_idx) {
                continue;
            }
            if self.parameter_owns_symbol(param_idx, sym)
                && let Some(name) = params.get(info_idx).and_then(|p| p.name)
            {
                return Some((info_idx, name));
            }
            info_idx += 1;
        }
        None
    }

    /// Returns true if `param_idx` is a `this` parameter declaration
    /// (`function f(this: T, ...)`). `this` parameters appear in the AST
    /// parameter list but are excluded from `ParamInfo` collections.
    fn is_this_parameter_decl(&self, param_idx: NodeIndex) -> bool {
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(param.name) else {
            return false;
        };
        name_node.kind == SyntaxKind::ThisKeyword as u16
    }

    /// Returns true when the parameter declaration node owns `sym` — either by
    /// `value_declaration` pointing back at the parameter node, or by the
    /// parameter's name node resolving to that symbol. Both directions are
    /// needed because destructuring parameters and parameter properties have
    /// slightly different value-declaration shapes.
    fn parameter_owns_symbol(&self, param_idx: NodeIndex, sym: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.binder.get_symbol(sym) else {
            return false;
        };
        if symbol.value_declaration == param_idx {
            return true;
        }
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(param.name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        self.binder.get_node_symbol(param.name) == Some(sym)
    }

    /// Run the solver's narrowing primitive against `param_type` for the
    /// truthy branch of `guard`. Wires up the type environment when present so
    /// `Lazy(DefId)` parameter types resolve correctly during narrowing.
    fn narrow_with_inferred_predicate_guard(
        &self,
        param_type: TypeId,
        guard: &TypeGuard,
    ) -> TypeId {
        let env_borrow = self.type_environment.as_ref().map(|env| env.borrow());
        flow_query::narrow_inferred_predicate_guard(
            self.interner,
            env_borrow.as_deref(),
            param_type,
            guard,
        )
    }

    /// Check if a node is a simple reference (identifier or property access).
    fn is_simple_reference(&self, node: NodeIndex) -> bool {
        // Skip parentheses and comma expressions to get the actual reference
        let node = self.skip_parenthesized(node);
        if let Some(node_data) = self.arena.get(node) {
            node_data.kind == SyntaxKind::Identifier as u16
                || matches!(
                    node_data.kind,
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
        } else {
            false
        }
    }

    /// Get the operand of a typeof expression.
    pub(crate) fn get_typeof_operand(&self, node: NodeIndex) -> Option<NodeIndex> {
        let node_data = self.arena.get(node)?;
        if node_data.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }

        let unary = self.arena.get_unary_expr(node_data)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }

        // Skip parentheses and comma expressions in typeof operand
        // This handles cases like: typeof (a, b).prop
        Some(self.skip_parenthesized(unary.operand))
    }

    /// Detect `x.constructor === SomeClass` or `SomeClass === x.constructor`.
    ///
    /// Returns `(TypeGuard::Constructor(instance_type), base_expr)` where
    /// `base_expr` is the object whose `.constructor` is being checked.
    fn constructor_comparison(
        &self,
        bin: &tsz_parser::parser::node::BinaryExprData,
    ) -> Option<(TypeGuard, NodeIndex)> {
        let is_equality = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::EqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;
        if !is_equality {
            return None;
        }

        // Try left.constructor === right
        if let Some(base) = self.get_constructor_property_base(bin.left)
            && let Some(instance_type) = self.instance_type_from_constructor(bin.right)
        {
            return Some((TypeGuard::Constructor(instance_type), base));
        }
        // Try left === right.constructor
        if let Some(base) = self.get_constructor_property_base(bin.right)
            && let Some(instance_type) = self.instance_type_from_constructor(bin.left)
        {
            return Some((TypeGuard::Constructor(instance_type), base));
        }
        None
    }
}
