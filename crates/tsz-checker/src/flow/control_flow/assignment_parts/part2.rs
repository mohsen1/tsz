impl<'a> FlowAnalyzer<'a> {
    fn property_key_from_rhs_assignment_to_reference(
        &self,
        rhs: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let rhs_elements = self.arena.get_literal_expr(rhs_node)?;
        let mut inferred = None;
        for &elem in &rhs_elements.elements.nodes {
            if elem.is_none() {
                continue;
            }
            if let Some(key) = self.property_key_from_assignment_to_reference(elem, reference) {
                inferred = Some(key);
            }
        }
        inferred
    }

    fn property_key_from_assignment_to_reference(
        &self,
        expr: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let expr = self.skip_parens_and_assertions(expr);
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        if !self.is_matching_reference(bin.left, reference) {
            return None;
        }
        if let Some(value) = self.literal_number_from_node_or_type(bin.right)
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(PropertyKey::Index(value as usize));
        }
        self.literal_atom_from_node_or_type(bin.right)
            .map(PropertyKey::Atom)
    }

    pub(crate) fn find_property_in_object_literal(
        &self,
        literal: &tsz_parser::parser::node::LiteralExprData,
        target: Atom,
    ) -> Option<NodeIndex> {
        for &elem in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem) else {
                continue;
            };
            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_property_assignment(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.initializer);
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_shorthand_property(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.name);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn assignment_affects_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                self.is_assignment_operator(bin.operator_token)
                    && self.assignment_affects_reference(bin.left, target)
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_affects_reference(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_affects_reference(decl.name, target));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_affects_reference(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_affects_reference(assignment_node, target)
    }

    pub fn assignment_targets_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_targets_reference_node(assignment_node, target)
    }

    pub(crate) fn assignment_targets_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                let is_op = self.is_assignment_operator(bin.operator_token);
                let targets = self.assignment_targets_reference_internal(bin.left, target);
                is_op && targets
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_targets_reference_internal(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_targets_reference_internal(decl.name, target));
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_targets_reference_internal(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_targets_reference_internal(assignment_node, target)
    }

    /// Check if the assignment node reassigns a BASE of the reference.
    ///
    /// For example, if `reference` is `obj.prop` and the assignment is `obj = { prop: 1 }`,
    /// this returns true because `obj` (a base of `obj.prop`) is being reassigned.
    ///
    /// But if `reference` is `config['works']` and the assignment is `config.works.prop = 'test'`,
    /// this returns false because the LHS is deeper than the reference, not a base of it.
    pub(crate) fn assignment_targets_base_of_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> bool {
        // Walk up the bases of the reference and check if the assignment targets any of them
        let mut current = self.reference_base(reference);
        while let Some(base) = current {
            if self.assignment_targets_reference_node(assignment_node, base) {
                return true;
            }
            current = self.reference_base(base);
        }
        false
    }

    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
        boundary_is_assignment_operator(operator)
    }

    pub(crate) fn narrow_assignment(&self, initial_type: TypeId, assigned_type: TypeId) -> TypeId {
        if let Some(env) = &self.type_environment {
            let env = env.borrow();
            crate::query_boundaries::flow_analysis::narrow_assignment(
                self.interner,
                Some(&env),
                initial_type,
                assigned_type,
            )
        } else {
            crate::query_boundaries::flow_analysis::narrow_assignment(
                self.interner,
                None,
                initial_type,
                assigned_type,
            )
        }
    }

    /// Resolve a `Lazy(DefId)` type to its concrete representation using the
    /// `TypeEnvironment`. Returns the original type if not lazy or if the
    /// environment is unavailable / doesn't contain the DefId.
    pub(super) fn resolve_lazy_via_env(&self, type_id: TypeId) -> TypeId {
        if let Some(env) = &self.type_environment {
            let env = env.borrow();
            crate::query_boundaries::flow::resolve_lazy_def_with_env(
                self.interner,
                Some(&env),
                type_id,
            )
        } else {
            crate::query_boundaries::flow::resolve_lazy_def_with_env(self.interner, None, type_id)
        }
    }

    /// Narrow an enum-typed declaration by an assigned value while preserving
    /// nominal enum identity.
    ///
    /// Returns `assigned_type` only when it carries the same nominal enum
    /// identity as `initial_type` (same enum, a member of that enum, or a
    /// union of such members). Bare literals or values of unrelated enum
    /// types collapse back to `initial_type` so that subsequent assignments
    /// still see the declared enum and report cross-enum TS2322.
    pub(crate) fn narrow_enum_assignment_target(
        &self,
        initial_resolved: TypeId,
        assigned_resolved: TypeId,
        initial_type: TypeId,
    ) -> TypeId {
        if let Some(env) = &self.type_environment {
            let env = env.borrow();
            crate::query_boundaries::flow_analysis::narrow_enum_assignment_target(
                self.interner,
                Some(&env),
                initial_resolved,
                assigned_resolved,
                initial_type,
            )
        } else {
            crate::query_boundaries::flow_analysis::narrow_enum_assignment_target(
                self.interner,
                None,
                initial_resolved,
                assigned_resolved,
                initial_type,
            )
        }
    }
}
