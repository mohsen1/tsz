use super::FlowAnalyzer;
use tsz_binder::{FlowNodeId, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> FlowAnalyzer<'a> {
    /// Check if the target reference (or its base) has been assigned to between
    /// the alias declaration and the current condition, which would invalidate
    /// aliased type guard narrowing.
    ///
    /// For simple identifiers (e.g., `e`): walks the flow graph from the
    /// condition's antecedent backward to the alias declaration, checking only
    /// the current flow path for assignments.
    ///
    /// For property accesses (e.g., `obj.x`, `this.x`): additionally checks
    /// ALL assignment flow nodes in the function after the alias declaration
    /// position, since property mutations can occur through paths not visible
    /// in the local flow graph.
    pub(crate) fn is_alias_reference_mutated(
        &self,
        alias_sym_id: SymbolId,
        target: NodeIndex,
        antecedent_id: FlowNodeId,
    ) -> bool {
        use tsz_binder::flow_flags;

        // Get the alias declaration position
        let alias_pos = match self.binder.get_symbol(alias_sym_id) {
            Some(sym) if sym.value_declaration.is_some() => self
                .arena
                .get(sym.value_declaration)
                .map(|n| n.pos)
                .unwrap_or(0),
            _ => return false,
        };

        // Walk the flow graph backward from the condition's antecedent.
        // Check if any ASSIGNMENT node on the current path targets the reference
        // (or its base). Stop when we reach nodes at or before the alias position.
        let mut visited = rustc_hash::FxHashSet::default();
        let mut stack = vec![antecedent_id];

        while let Some(flow_id) = stack.pop() {
            if flow_id.is_none() || !visited.insert(flow_id) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
                continue;
            };

            // Stop walking if this flow node is at or before the alias declaration
            if let Some(node) = self.arena.get(flow.node)
                && node.pos <= alias_pos
            {
                continue;
            }

            // Check if this is an assignment targeting our reference or its base
            if flow.has_any_flags(flow_flags::ASSIGNMENT)
                && (self.assignment_targets_reference_node(flow.node, target)
                    || self.assignment_targets_base_of_reference(flow.node, target))
            {
                return true;
            }

            // Continue to antecedents
            for &ant in &flow.antecedent {
                stack.push(ant);
            }
        }

        // Mirrors tsc's `isConstantReference` (checker.ts ~28978):
        //   * `this`/`super` are constant.
        //   * An identifier is constant iff it is a const variable, or a
        //     parameter/mutable local that has no reassignments.
        //   * A property access is constant iff the base is constant AND the
        //     accessed property is readonly.
        //
        // Alias-based narrowing is gated on the target being a constant
        // reference (tsc applies the alias initializer only when the
        // reference is constant). When the target is *not* a constant
        // reference, we invalidate alias narrowing entirely — independent of
        // whether a function-wide assignment is observable. This matches
        // tsc's behavior on cases like `f27`, where `outer.obj` is never
        // reassigned but `obj` is mutable so the alias is not projected.
        if !self.is_constant_alias_target(target) {
            return true;
        }

        false
    }

    /// Determine whether `target` is a constant reference for the purposes of
    /// alias-based narrowing. See `is_alias_reference_mutated` for the rules.
    pub(crate) fn is_constant_alias_target(&self, target: NodeIndex) -> bool {
        let target = self.skip_parenthesized(target);
        let Some(node) = self.arena.get(target) else {
            return false;
        };

        let kind = node.kind;
        if kind == SyntaxKind::ThisKeyword as u16 || kind == SyntaxKind::SuperKeyword as u16 {
            return true;
        }

        if kind == SyntaxKind::Identifier as u16 {
            // Mirror tsc's `isConstantReference` for identifiers:
            //   isConstantVariable(s) || isParameterOrMutableLocal(s) && !isSymbolAssigned(s)
            //
            // tsc's `isSymbolAssigned` is "any reassignment exists" —
            // independent of whether the assignment is before or after the
            // use site. So:
            //   * Const variables are always constant references.
            //   * Parameter / non-exported let are constant only when no
            //     reassignment exists anywhere in the containing function.
            let Some(symbol_id) = self.binder.resolve_identifier(self.arena, target) else {
                return false;
            };
            let Some(symbol) = self.binder.get_symbol(symbol_id) else {
                return false;
            };
            let decl_id = symbol.value_declaration;
            if self.declaration_is_const(decl_id) {
                return true;
            }
            // Catch clause variables behave as constant references in tsc
            // (`isParameterOrMutableLocalVariable` accepts
            // `isVariableDeclaration(d) && isCatchClause(d.parent)`), but our
            // `is_effectively_const_for_narrowing` helper does not cover
            // them. Honor the rule here, gated on no reassignments.
            if self.is_catch_clause_variable(decl_id) {
                return self.get_last_assignment_pos(symbol_id, target) == 0;
            }
            // For non-const identifiers, require parameter/let-style local
            // (`is_effectively_const_for_narrowing` enforces the eligibility
            // gate) AND zero reassignments anywhere in the function. The
            // narrower's existing helper allows assignments strictly before
            // the reference; here we tighten it to tsc's "no assignments".
            if !self.is_effectively_const_for_narrowing(target) {
                return false;
            }
            return self.get_last_assignment_pos(symbol_id, target) == 0;
        }

        if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            if access.question_dot_token {
                return false;
            }
            // Recursively check the base.
            if !self.is_constant_alias_target(access.expression) {
                return false;
            }
            // Property must be readonly on the base type.
            return self.is_access_property_readonly(access);
        }

        // NonNull / type-assertion / parenthesized wrappers should be
        // transparent. Recurse through them to mirror tsc's behavior, where
        // these wrappers don't affect `isConstantReference`.
        if kind == syntax_kind_ext::NON_NULL_EXPRESSION
            || kind == syntax_kind_ext::TYPE_ASSERTION
            || kind == syntax_kind_ext::AS_EXPRESSION
            || kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                return self.is_constant_alias_target(unary.expression);
            }
            if let Some(assertion) = self.arena.get_type_assertion(node) {
                return self.is_constant_alias_target(assertion.expression);
            }
        }

        false
    }

    /// True iff `decl_id` is (or is contained within) the variable
    /// declaration directly belonging to a `try { } catch (e) { }` clause.
    /// tsc treats such variables as constant references inside the catch
    /// block when they are not reassigned.
    fn is_catch_clause_variable(&self, decl_id: NodeIndex) -> bool {
        if decl_id.is_none() {
            return false;
        }
        // Walk up to find a VARIABLE_DECLARATION (catch's variableDeclaration
        // wraps any binding pattern). Then check whether its direct parent is
        // a CATCH_CLAUSE.
        let mut current = decl_id;
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let Some(ext) = self.arena.get_extended(current) else {
                    return false;
                };
                if ext.parent.is_none() {
                    return false;
                }
                let Some(parent) = self.arena.get(ext.parent) else {
                    return false;
                };
                return parent.kind == syntax_kind_ext::CATCH_CLAUSE;
            }
            if node.kind != syntax_kind_ext::BINDING_ELEMENT
                && node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                && node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                && node.kind != SyntaxKind::Identifier as u16
            {
                return false;
            }
            let Some(ext) = self.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
        false
    }

    /// True iff `decl_id` is (or is contained within) a `const` variable
    /// declaration. Mirrors tsc's `isConstantVariable` lookup
    /// (`getDeclarationNodeFlagsFromSymbol & NodeFlags.Constant`).
    ///
    /// Symbols for destructured bindings store the *binding element identifier*
    /// as their `value_declaration`, so checking the identifier directly via
    /// `is_const_variable_declaration` would miss the `const` keyword on the
    /// enclosing `VariableDeclarationList`. Walk up through binding patterns
    /// until we reach a `VariableDeclaration`.
    fn declaration_is_const(&self, decl_id: NodeIndex) -> bool {
        if decl_id.is_none() {
            return false;
        }
        // Direct hit (simple `const x = ...` declarations).
        if self.arena.is_const_variable_declaration(decl_id) {
            return true;
        }
        let mut current = decl_id;
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return self.arena.is_const_variable_declaration(current);
            }
            // Only walk through binding-pattern wrappers; bail at anything
            // else so we don't accidentally claim other `const` ancestors.
            if node.kind != syntax_kind_ext::BINDING_ELEMENT
                && node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                && node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                && node.kind != SyntaxKind::Identifier as u16
            {
                return false;
            }
            let Some(ext) = self.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
        false
    }

    /// Check whether the property accessed by `access` is a readonly member of
    /// the base expression's type. Falls back to `false` (i.e. the property is
    /// treated as mutable, which makes `is_constant_alias_target` reject the
    /// access) if we cannot resolve a property name or base type.
    pub(crate) fn is_access_property_readonly(
        &self,
        access: &tsz_parser::parser::node::AccessExprData,
    ) -> bool {
        // Resolve the property name.
        let name_idx = access.name_or_argument;
        let name_atom = if let Some(ident) = self.arena.get_identifier_at(name_idx) {
            self.interner.intern_string(&ident.escaped_text)
        } else if let Some(atom) = self.literal_atom_from_node_or_type(name_idx) {
            atom
        } else {
            // Computed/dynamic key — conservatively report as non-readonly.
            return false;
        };

        // Resolve the base type. Prefer the cached node type; for a bare `this`
        // expression with no cached entry, fall back to `concrete_this_type`.
        let Some(node_types) = self.node_types else {
            return false;
        };
        let base_type = if let Some(&t) = node_types.get(&access.expression.0) {
            t
        } else if let Some(t) = self.concrete_this_type
            && let Some(base_node) = self.arena.get(access.expression)
            && base_node.kind == SyntaxKind::ThisKeyword as u16
        {
            t
        } else {
            return false;
        };

        let prop_text = self.interner.resolve_atom_ref(name_atom);
        self.interner
            .is_property_readonly(base_type, prop_text.as_ref())
    }
    /// Check if any assignment flow node in the containing function targets
    /// the base of the given reference after the specified position. This is a
    /// conservative function-wide check used for property access aliases.
    ///
    /// Scoped to the containing function to avoid false positives from
    /// assignments in sibling class constructors/methods.
    fn has_base_assignment_after_pos(&self, target: NodeIndex, after_pos: u32) -> bool {
        use tsz_binder::flow_flags;

        // Find the containing function's position bounds to scope the search.
        // This prevents matching `this.x = 10` in class C11 when checking
        // an alias in class C10.
        let (fn_start, fn_end) = self.containing_function_bounds(target);

        let flow_count = self.binder.flow_nodes.len();
        for i in 0..flow_count {
            let flow_id = tsz_binder::FlowNodeId(i as u32);
            let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
                continue;
            };

            if !flow.has_any_flags(flow_flags::ASSIGNMENT) {
                continue;
            }

            let Some(node) = self.arena.get(flow.node) else {
                continue;
            };

            // Only consider assignments after the alias declaration
            if node.pos <= after_pos {
                continue;
            }

            // Only consider assignments within the same function
            if node.pos < fn_start || node.pos > fn_end {
                continue;
            }

            // Check if this assignment targets the reference itself or its base
            if self.assignment_targets_reference_node(flow.node, target)
                || self.assignment_targets_base_of_reference(flow.node, target)
            {
                return true;
            }
        }
        false
    }

    /// Get the position bounds (start, end) of the containing function-like
    /// node for the given reference. Returns (0, `u32::MAX`) if no containing
    /// function is found (source file level).
    fn containing_function_bounds(&self, reference: NodeIndex) -> (u32, u32) {
        let mut current = reference;
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            if current.is_none() {
                break;
            }
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.is_function_like() {
                return (node.pos, node.end);
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        (0, u32::MAX)
    }

    /// Returns `true` when `switch_expr` is an identifier that is a const alias
    /// for a property access of `reference` (e.g. `const kind = obj.kind; switch(kind)`) or a
    /// destructuring alias (`const { kind } = obj; switch(kind)`). This allows switch
    /// narrowing to work for aliased discriminants.
    pub(crate) fn is_aliased_discriminant_switch_expr(
        &self,
        switch_expr: NodeIndex,
        reference: NodeIndex,
    ) -> bool {
        let expr = self.skip_parenthesized(switch_expr);
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        // Case 1: `const alias = reference.prop` (simple property access alias)
        if let Some((_, initializer)) = self.const_condition_initializer(expr) {
            let init_expr = self.skip_parenthesized(initializer);
            let Some(init_node) = self.arena.get(init_expr) else {
                return false;
            };
            if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                return self
                    .relative_discriminant_path(init_expr, reference)
                    .is_some_and(|(path, _)| !path.is_empty());
            }
        }
        // Case 2: `const { prop: alias } = reference` (destructuring alias)
        if let Some((base, _)) = self.binding_element_property_alias(expr) {
            return self.is_matching_reference(base, reference);
        }
        false
    }
}
