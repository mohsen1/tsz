//! Generic type argument validation (TS2344 constraint checking).

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CallTypeArgumentValidation {
    pub count_mismatch: bool,
}

// =============================================================================
// Generic Type Argument Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Check if a type node is an `infer` type, looking through parentheses.
    /// Returns true for `infer T`, `(infer T)`, `((infer T))`, etc.
    fn is_infer_type_node_through_parens(&self, mut node_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        for _ in 0..10 {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE {
                return true;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            {
                node_idx = wrapped.type_node;
                continue;
            }
            return false;
        }
        false
    }

    fn type_nodes_structurally_equal(&self, left: NodeIndex, right: NodeIndex) -> bool {
        if let (Some(left_text), Some(right_text)) = (self.node_text(left), self.node_text(right))
            && left_text == right_text
        {
            return true;
        }

        let left_node = self.ctx.arena.get(left);
        let right_node = self.ctx.arena.get(right);
        match (left_node, right_node) {
            (Some(l), Some(r)) if l.kind == r.kind => {
                if let (Some(li), Some(ri)) = (
                    self.ctx.arena.get_identifier(l),
                    self.ctx.arena.get_identifier(r),
                ) {
                    return li.escaped_text == ri.escaped_text;
                }
                if let (Some(llt), Some(rlt)) = (
                    self.ctx.arena.get_literal_type(l),
                    self.ctx.arena.get_literal_type(r),
                ) {
                    return self.type_nodes_structurally_equal(llt.literal, rlt.literal);
                }
                if let (Some(ll), Some(rl)) =
                    (self.ctx.arena.get_literal(l), self.ctx.arena.get_literal(r))
                {
                    return ll.text == rl.text;
                }
                if let (Some(lref), Some(rref)) = (
                    self.ctx.arena.get_type_ref(l),
                    self.ctx.arena.get_type_ref(r),
                ) {
                    if !self.type_nodes_structurally_equal(lref.type_name, rref.type_name) {
                        return false;
                    }
                    let left_args = lref.type_arguments.as_ref();
                    let right_args = rref.type_arguments.as_ref();
                    return match (left_args, right_args) {
                        (None, None) => true,
                        (Some(la), Some(ra)) => {
                            la.nodes.len() == ra.nodes.len()
                                && la
                                    .nodes
                                    .iter()
                                    .zip(ra.nodes.iter())
                                    .all(|(&ln, &rn)| self.type_nodes_structurally_equal(ln, rn))
                        }
                        _ => false,
                    };
                }
                if let (Some(lidx), Some(ridx)) = (
                    self.ctx.arena.get_indexed_access_type(l),
                    self.ctx.arena.get_indexed_access_type(r),
                ) {
                    return self.type_nodes_structurally_equal(lidx.object_type, ridx.object_type)
                        && self.type_nodes_structurally_equal(lidx.index_type, ridx.index_type);
                }
                if let (Some(lpar), Some(rpar)) = (
                    self.ctx.arena.get_parenthesized(l),
                    self.ctx.arena.get_parenthesized(r),
                ) {
                    return self.type_nodes_structurally_equal(lpar.expression, rpar.expression);
                }
                false
            }
            _ => false,
        }
    }

    fn is_descendant_type_node(&self, node_idx: NodeIndex, ancestor_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        let Some(ancestor) = self.ctx.arena.get(ancestor_idx) else {
            return false;
        };
        if node.pos >= ancestor.pos && node.end <= ancestor.end {
            return true;
        }

        let mut current = self.ctx.arena.parent_of(node_idx);
        while let Some(parent_idx) = current {
            if parent_idx == ancestor_idx {
                return true;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        false
    }

    fn type_argument_is_narrowed_by_conditional_true_branch(
        &mut self,
        arg_idx: NodeIndex,
        constraint: TypeId,
    ) -> bool {
        let constraint = self.resolve_lazy_type(constraint);
        // Collect extends types from all enclosing conditional true branches
        // where the type argument is (or references) the check type.
        // In nested conditionals like `T extends A ? T extends B ? X<T> : ...`,
        // the effective constraint on T is the intersection A & B.
        let mut accumulated_extends: Vec<TypeId> = Vec::new();
        let mut current = self.ctx.arena.parent_of(arg_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                && self.is_descendant_type_node(arg_idx, cond.true_type)
            {
                // Case 1: The type arg IS the check type (e.g., `T extends U ? X<T> : never`).
                // Collect the extends type for intersection-based constraint checking.
                if self.type_nodes_structurally_equal(arg_idx, cond.check_type) {
                    let extends_type = self.get_type_from_type_node(cond.extends_type);
                    if extends_type != TypeId::ERROR {
                        // Check if this single extends type satisfies the constraint
                        // (bidirectional assignability for exact match)
                        if extends_type == constraint
                            || (self.is_assignable_to(extends_type, constraint)
                                && self.is_assignable_to(constraint, extends_type))
                        {
                            return true;
                        }
                        accumulated_extends.push(extends_type);
                    }
                }

                // Case 2: The type arg is derived from (but not identical to) the
                // check type (e.g., `T extends O ? X<ReturnType<T['m']>> : never`).
                // Only suppress when the arg is DERIVED from the check type — if the
                // arg IS the check type, Case 1 already handled it and correctly
                // didn't suppress when extends doesn't satisfy the constraint.
                if !self.type_nodes_structurally_equal(arg_idx, cond.check_type)
                    && self.type_node_contains_reference(arg_idx, cond.check_type)
                {
                    return true;
                }

                // Case 3: The check type wraps the type argument (e.g.,
                // `[T] extends [{ a: string }] ? ... : ...`). Only trigger when
                // the check type WRAPS the arg (not when they're identical).
                if !self.type_nodes_structurally_equal(cond.check_type, arg_idx)
                    && self.type_node_contains_reference(cond.check_type, arg_idx)
                {
                    return true;
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        // If we collected multiple extends types from nested conditionals,
        // check if their intersection satisfies the constraint.
        // e.g., `T extends {a: string} ? T extends {b: number} ? T35<T> : ...`
        // → intersection is `{a: string} & {b: number}` which satisfies `{a: string, b: number}`.
        if accumulated_extends.len() >= 2 {
            let intersection = self.ctx.types.intersection(accumulated_extends);
            if self.is_assignable_to(intersection, constraint) {
                return true;
            }
        }
        false
    }

    /// Check if `node_idx` contains a syntactic reference to a type node that
    /// is structurally equal to `target_type_node`. Used to detect when a type
    /// argument like `T['m']` references the conditional's check type `T`.
    fn type_node_contains_reference(
        &self,
        node_idx: NodeIndex,
        target_type_node: NodeIndex,
    ) -> bool {
        if self.type_nodes_structurally_equal(node_idx, target_type_node) {
            return true;
        }
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.type_node_contains_reference(child_idx, target_type_node) {
                return true;
            }
        }
        false
    }

    /// Check if a type argument is inside the FALSE branch of a conditional type
    /// where the check type is (or contains) the same type parameter.
    fn type_arg_is_in_conditional_false_branch_of_check_type(&self, arg_idx: NodeIndex) -> bool {
        let mut current = self.ctx.arena.parent_of(arg_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                && self.is_descendant_type_node(arg_idx, cond.false_type)
                && (self.type_nodes_structurally_equal(arg_idx, cond.check_type)
                    || self.type_node_contains_reference(cond.check_type, arg_idx)
                    || self.type_node_contains_reference(arg_idx, cond.check_type))
            {
                return true;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        false
    }

    fn type_arg_identifier_name_local(&self, arg_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let arg_node = self.ctx.arena.get(arg_idx)?;
        if arg_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let tr = self.ctx.arena.get_type_ref(arg_node)?;
            let name_node = self.ctx.arena.get(tr.type_name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        } else if arg_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(arg_node)?;
            Some(ident.escaped_text.clone())
        } else {
            None
        }
    }

    fn infer_type_param_has_name_local(
        &self,
        infer_data: &tsz_parser::parser::node::InferTypeData,
        name: &str,
    ) -> bool {
        if let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
            && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
            && let Some(name_node) = self.ctx.arena.get(tp_data.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            ident.escaped_text == name
        } else {
            false
        }
    }

    fn type_reference_name_matches_local(&self, type_name_idx: NodeIndex, name: &str) -> bool {
        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            ident.escaped_text == name
        } else {
            false
        }
    }

    fn extends_clause_has_weak_key_constrained_infer_named_local(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::INFER_TYPE
            && let Some(infer_data) = self.ctx.arena.get_infer_type(node)
            && self.infer_type_param_has_name_local(infer_data, name)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let is_weak_collection = self
                .type_reference_name_matches_local(type_ref.type_name, "WeakMap")
                || self.type_reference_name_matches_local(type_ref.type_name, "WeakSet");
            if is_weak_collection
                && let Some(type_args) = &type_ref.type_arguments
                && let Some(&first_arg) = type_args.nodes.first()
                && let Some(first_node) = self.ctx.arena.get(first_arg)
                && first_node.kind == syntax_kind_ext::INFER_TYPE
                && let Some(infer_data) = self.ctx.arena.get_infer_type(first_node)
                && self.infer_type_param_has_name_local(infer_data, name)
            {
                return true;
            }

            if let Some(type_args) = &type_ref.type_arguments {
                for &arg in &type_args.nodes {
                    if self.extends_clause_has_weak_key_constrained_infer_named_local(arg, name) {
                        return true;
                    }
                }
            }
        }

        if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
            for &elem_idx in &tuple.elements.nodes {
                if self.extends_clause_has_weak_key_constrained_infer_named_local(elem_idx, name) {
                    return true;
                }
            }
        }

        if let Some(named_member) = self.ctx.arena.get_named_tuple_member(node)
            && self.extends_clause_has_weak_key_constrained_infer_named_local(
                named_member.type_node,
                name,
            )
        {
            return true;
        }

        if (node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || node.kind == syntax_kind_ext::OPTIONAL_TYPE
            || node.kind == syntax_kind_ext::REST_TYPE)
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            && self
                .extends_clause_has_weak_key_constrained_infer_named_local(wrapped.type_node, name)
        {
            return true;
        }

        false
    }

    fn is_infer_with_weak_key_implicit_constraint_in_conditional_local(
        &self,
        arg_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(name) = self.type_arg_identifier_name_local(arg_idx) else {
            return false;
        };
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        let mut current = arg_idx;
        for _ in 0..30 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                    && let Some(true_node) = self.ctx.arena.get(cond.true_type)
                    && arg_node.pos >= true_node.pos
                    && arg_node.end <= true_node.end
                    && self.extends_clause_has_weak_key_constrained_infer_named_local(
                        cond.extends_type,
                        &name,
                    )
                {
                    return true;
                }
                if parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    return false;
                }
            }
            current = parent;
        }
        false
    }

    fn has_hidden_conditional_infer_constraint_local(&self, arg_idx: NodeIndex) -> bool {
        self.is_infer_with_implicit_constraint_in_conditional(arg_idx)
            || self.is_infer_with_weak_key_implicit_constraint_in_conditional_local(arg_idx)
    }

    // =========================================================================
    // Type Argument Validation
    // =========================================================================

    /// Validate explicit type arguments against their constraints for call expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    /// Reports TS2558 when a non-generic function is called with type arguments.
    /// Returns validation state for downstream call diagnostics.
    pub(crate) fn validate_call_type_arguments(
        &mut self,
        callee_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        call_idx: NodeIndex,
    ) -> CallTypeArgumentValidation {
        use tsz_scanner::SyntaxKind;

        if let Some(call_expr) = self.ctx.arena.get_call_expr_at(call_idx)
            && let Some(callee_node) = self.ctx.arena.get(call_expr.expression)
            && callee_node.kind == SyntaxKind::SuperKeyword as u16
            && !type_args_list.nodes.is_empty()
        {
            // The parser already reports TS2754 for `super<T>(...)`.
            // Skip re-emitting it here to avoid duplicate diagnostics.
            return CallTypeArgumentValidation::default();
        }

        let callee_type_orig = callee_type;
        let callee_type = self.evaluate_application_type(callee_type);
        // Resolve Lazy types so the classifier can see callable/function signatures.
        let callee_type = {
            let resolved = self.resolve_lazy_type(callee_type);
            if resolved != callee_type {
                resolved
            } else {
                callee_type
            }
        };

        let got = type_args_list.nodes.len();
        let type_arg_error_anchor = type_args_list.nodes.first().copied().unwrap_or(call_idx);

        if got > 0
            && callee_type != TypeId::ANY
            && self.replace_function_type_for_call(callee_type_orig, callee_type) == TypeId::ANY
        {
            self.error_at_node(
                call_idx,
                crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
            );
            // Still resolve type arguments even when the call is untyped.
            // This ensures identifiers in type arguments are marked as referenced
            // for noUnusedLocals (TS6133).
            for &arg_idx in &type_args_list.nodes {
                self.get_type_of_node(arg_idx);
            }
            return CallTypeArgumentValidation::default();
        }

        // Get the type parameters from the callee type. For callables with overloads,
        // prefer a signature whose type parameter arity matches the provided type args.
        // Delegates to solver query which handles Function/Callable/overload matching.
        let Some(type_params) = query::extract_type_params_for_call(
            self.ctx.types.as_type_database(),
            callee_type,
            got,
        ) else {
            // None = multiple overloads match or not a callable type; skip validation.
            return CallTypeArgumentValidation::default();
        };

        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();

        if type_params.is_empty() {
            // Type params are empty but we have type args. Check if the callee has
            // overloads with different type param counts (TS2743).
            if got > 0 {
                if let Some(counts) = query::overload_type_param_counts(
                    self.ctx.types.as_type_database(),
                    callee_type,
                ) {
                    // TS2743: No overload expects N type arguments, but overloads
                    // do exist that expect either A or B type arguments.
                    if counts.len() == 2 {
                        self.error_at_node_msg(
                            type_arg_error_anchor,
                            crate::diagnostics::diagnostic_codes::NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR,
                            &[
                                &got.to_string(),
                                &counts[0].to_string(),
                                &counts[1].to_string(),
                            ],
                        );
                        return CallTypeArgumentValidation {
                            count_mismatch: true,
                        };
                    }
                }
                // TS2558: Expected 0 type arguments, but got N.
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                    &["0", &got.to_string()],
                );
                return CallTypeArgumentValidation {
                    count_mismatch: true,
                };
            }
            return CallTypeArgumentValidation::default();
        }

        if got < min_required || got > max_expected {
            // Check if the callee has overloads with different type param counts (TS2743)
            if let Some(counts) =
                query::overload_type_param_counts(self.ctx.types.as_type_database(), callee_type)
                && counts.len() == 2
                && !counts.contains(&got)
            {
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR,
                    &[
                        &got.to_string(),
                        &counts[0].to_string(),
                        &counts[1].to_string(),
                    ],
                );
                return CallTypeArgumentValidation {
                    count_mismatch: true,
                };
            }
            // TS2558: Expected N type arguments, but got M.
            // When there are type params with defaults, show the range
            let expected_str = if min_required == max_expected {
                max_expected.to_string()
            } else {
                format!("{min_required}-{max_expected}")
            };
            self.error_at_node_msg(
                type_arg_error_anchor,
                crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                &[&expected_str, &got.to_string()],
            );
            return CallTypeArgumentValidation {
                count_mismatch: true,
            };
        }

        self.validate_type_args_against_params(&type_params, type_args_list);
        CallTypeArgumentValidation::default()
    }

    /// Validate type arguments against their constraints for type references (e.g., `A<X, Y>`).
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    ///
    /// This handles cases like `class A<T, U extends T>` where `A<{a: string}, {b: string}>`
    /// should error because `{b: string}` doesn't extend `{a: string}`.
    /// Validate type arguments on a type reference.
    ///
    /// Returns `true` when the type argument count is wrong (TS2314/TS2707 emitted).
    /// Callers can use this to return `TypeId::ERROR` and suppress cascading
    /// diagnostics (matching tsc's `errorType` propagation for bad type arg counts).
    pub(crate) fn validate_type_reference_type_arguments(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        type_args_list: &tsz_parser::parser::NodeList,
        type_ref_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        let mut sym_id = sym_id;
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::ALIAS)
        {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                sym_id = target;
            }
        }

        if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(sym_id) {
            let lib_binders = self.get_lib_binders();
            if let Some(name) = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map(|symbol| symbol.escaped_name.clone())
            {
                // Mirror the no-explicit-type-args path so lib declarations with
                // defaulted generics (for example Iterable<T, TReturn = any, TNext = any>)
                // are validated against their merged parameter list instead of the
                // pre-primed AST fallback.
                self.prime_lib_type_params(&name);
            }
        }

        let lib_binders = self.get_lib_binders();
        let base_name = self
            .ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
        let type_params = self.get_reference_type_params_for_symbol(sym_id, &base_name);
        if type_params.is_empty() {
            // Before emitting TS2315, check if this symbol's declaration actually has
            // type parameters. Cross-arena symbols (e.g., lib types like Awaited<T>)
            // may fail to resolve type parameters because their declaration is in a
            // different arena. In that case, check the declaration directly to avoid
            // false positives.
            let has_type_params_in_decl = self.symbol_declaration_has_type_parameters(sym_id);
            // Suppress TS2315 for symbols from unresolved modules (type is ERROR)
            let symbol_type = self.get_type_of_symbol(sym_id);
            if !has_type_params_in_decl
                && symbol_type != TypeId::ERROR
                && symbol_type != TypeId::ANY
                && !type_args_list.nodes.is_empty()
            {
                // TSC points the TS2315 error at the type name (e.g. `C` in
                // `C<string>`), not at the first type argument. Extract the
                // type_name from the TypeReference node.
                let error_anchor = self
                    .ctx
                    .arena
                    .get(type_ref_idx)
                    .and_then(|node| self.ctx.arena.get_type_ref(node))
                    .map(|tr| tr.type_name)
                    .unwrap_or(type_ref_idx);
                self.error_at_node_msg(
                    error_anchor,
                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                    &[base_name.as_str()],
                );
            }
            // Still resolve type arguments even when the type is not generic.
            // This ensures identifiers in type arguments are marked as referenced
            // for noUnusedLocals (TS6133).
            for &arg_idx in &type_args_list.nodes {
                self.get_type_of_node(arg_idx);
            }
            return false;
        }

        let type_arg_error_anchor = self
            .ctx
            .arena
            .get(type_ref_idx)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            .map(|type_ref| type_ref.type_name)
            .unwrap_or(type_ref_idx);
        let display_name = Self::format_generic_display_name_with_interner(
            &base_name,
            &type_params,
            self.ctx.types,
        );
        let min_required = self.count_required_reference_type_params(sym_id, &base_name);
        self.validate_type_reference_type_arguments_against_params(
            &type_params,
            min_required,
            type_args_list,
            type_arg_error_anchor,
            &display_name,
        )
    }

    pub(crate) fn validate_type_reference_type_arguments_against_params(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        min_required: usize,
        type_args_list: &tsz_parser::parser::NodeList,
        type_arg_error_anchor: NodeIndex,
        display_name: &str,
    ) -> bool {
        let got = type_args_list.nodes.len();
        let max_expected = type_params.len();
        if got < min_required || got > max_expected {
            if min_required < max_expected {
                let min_str = min_required.to_string();
                let max_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[display_name, &min_str, &max_str],
                );
            } else {
                let count_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[display_name, &count_str],
                );
            }
            return true;
        }

        // NOTE: TS4109 (circular type arguments) is intentionally NOT checked here
        // via a syntactic walk.  In TSC, TS4109 is detected during actual type
        // argument resolution (`resolveTypeArguments`) via `pushTypeResolution`/
        // `popTypeResolution`, which naturally distinguishes between true cycles
        // and harmless recursive references (e.g. `type T = string | Promise<T>`).
        // A syntactic check cannot reliably distinguish these cases and produces
        // false positives.  TS4109 should be emitted from the solver's type
        // resolution path once cycle detection is implemented there.
        self.validate_type_args_against_params(type_params, type_args_list);
        false
    }

    pub(crate) fn validate_jsdoc_type_reference_type_arguments_against_params(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args_list: &tsz_parser::parser::NodeList,
        type_arg_error_anchor: NodeIndex,
        display_name: &str,
    ) -> bool {
        let got = type_args_list.nodes.len();
        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();
        if got < min_required || got > max_expected {
            if min_required < max_expected {
                let min_str = min_required.to_string();
                let max_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[display_name, &min_str, &max_str],
                );
            } else {
                let count_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[display_name, &count_str],
                );
            }
            return true;
        }

        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| {
                self.check_type_node_for_static_member_class_type_param_refs(arg_idx);
                self.check_type_node(arg_idx);
                self.get_type_from_type_node(arg_idx)
            })
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            let Some(constraint) = param.constraint else {
                continue;
            };
            if type_arg == TypeId::ERROR
                || query::is_this_type(self.ctx.types.as_type_database(), type_arg)
            {
                continue;
            }
            // Evaluate the constraint before checking assignability. Constraints
            // like `WeakKeyTypes[keyof WeakKeyTypes]` (indexed access types) need
            // to be reduced to their concrete form (e.g., `object | symbol`) for
            // the assignability check to work correctly.
            let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
            if self.is_assignable_to(type_arg, evaluated_constraint) {
                continue;
            }
            let error_anchor = type_args_list
                .nodes
                .get(i)
                .copied()
                .unwrap_or(type_arg_error_anchor);
            self.error_at_node_msg(
                error_anchor,
                crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[
                    &self.format_type_diagnostic(type_arg),
                    &self.format_type_diagnostic(constraint),
                ],
            );
            return false;
        }

        false
    }

    /// Validate explicit type arguments against their constraints for new expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    pub(crate) fn validate_new_expression_type_arguments(
        &mut self,
        constructor_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        call_idx: NodeIndex,
    ) {
        // Get the type parameters from the constructor type
        let Some(shape) = query::callable_shape_for_type(self.ctx.types, constructor_type) else {
            return;
        };
        let got = type_args_list.nodes.len();
        let type_arg_error_anchor = type_args_list.nodes.first().copied().unwrap_or(call_idx);

        if shape.construct_signatures.is_empty() {
            return;
        }

        // For callable types with overloaded construct signatures, prefer
        // a signature whose type parameter arity matches the provided args.
        let type_params = {
            let matching: Vec<_> = shape
                .construct_signatures
                .iter()
                .filter(|sig| {
                    let max = sig.type_params.len();
                    let min = sig
                        .type_params
                        .iter()
                        .filter(|tp| tp.default.is_none())
                        .count();
                    got >= min && got <= max
                })
                .collect();
            // When multiple overloads match the arity, skip eager TS2344 validation.
            // TSC only emits TS2344 when NO overload's constraints are satisfied.
            if matching.len() > 1 {
                return;
            }
            if let Some(sig) = matching.first() {
                sig.type_params.clone()
            } else {
                // Fall back to first signature for diagnostics
                shape
                    .construct_signatures
                    .first()
                    .map(|sig| sig.type_params.clone())
                    .unwrap_or_default()
            }
        };

        // Collect distinct type param counts from construct signatures for TS2743
        let construct_param_counts = {
            let mut counts: Vec<usize> = shape
                .construct_signatures
                .iter()
                .map(|sig| sig.type_params.len())
                .collect();
            counts.sort_unstable();
            counts.dedup();
            counts
        };

        if type_params.is_empty() {
            if got > 0 {
                // Check for TS2743: overloads exist with different type param counts
                if construct_param_counts.len() >= 2 && !construct_param_counts.contains(&got) {
                    self.error_at_node_msg(
                        type_arg_error_anchor,
                        crate::diagnostics::diagnostic_codes::NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR,
                        &[
                            &got.to_string(),
                            &construct_param_counts[0].to_string(),
                            &construct_param_counts[1].to_string(),
                        ],
                    );
                } else {
                    // TS2558: Expected 0 type arguments, but got N.
                    self.error_at_node_msg(
                        type_arg_error_anchor,
                        crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                        &["0", &got.to_string()],
                    );
                }
            }
            return;
        }

        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();
        if got < min_required || got > max_expected {
            // Check for TS2743: overloads exist with different type param counts
            if construct_param_counts.len() >= 2 && !construct_param_counts.contains(&got) {
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::NO_OVERLOAD_EXPECTS_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR,
                    &[
                        &got.to_string(),
                        &construct_param_counts[0].to_string(),
                        &construct_param_counts[1].to_string(),
                    ],
                );
            } else {
                // TS2558: Expected N type arguments, but got M.
                let expected_str = if min_required == max_expected {
                    max_expected.to_string()
                } else {
                    format!("{min_required}-{max_expected}")
                };
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                    &[&expected_str, &got.to_string()],
                );
            }
            return;
        }

        self.validate_type_args_against_params(&type_params, type_args_list);
    }

    /// Validate explicit type arguments on a JSX element (e.g. `<MyComp<Prop>>`).
    ///
    /// JSX elements with explicit type arguments behave like `new MyComp<Prop>()` for
    /// class components and `MyComp<Prop>(props)` for function components. This method
    /// validates the type argument count and constraints, emitting:
    ///   - TS2558 when the count doesn't match the component's type parameter count.
    ///   - TS2344 when a type argument doesn't satisfy its constraint.
    ///
    /// Returns `true` when the type argument count is wrong (TS2558 emitted), so the
    /// caller can skip props-type checking for that element (tsc does not emit TS2322
    /// for JSX elements that have a wrong type-argument arity).
    pub(crate) fn validate_jsx_element_type_arguments(
        &mut self,
        component_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        element_idx: NodeIndex,
    ) -> bool {
        let got = type_args_list.nodes.len();
        if got == 0 {
            return false;
        }

        // Resolve Lazy/Application types to expose Callable/Function shapes.
        let resolved = {
            let ev = self.evaluate_application_type(component_type);
            let ev = self.evaluate_type_with_env(ev);
            let r = self.resolve_lazy_type(ev);
            if r != ev { r } else { ev }
        };

        // If the component has construct signatures (class component), validate against
        // them — mirrors `validate_new_expression_type_arguments` for `new` expressions.
        if let Some(shape) = query::callable_shape_for_type(self.ctx.types, resolved)
            && !shape.construct_signatures.is_empty() {
                let type_arg_error_anchor =
                    type_args_list.nodes.first().copied().unwrap_or(element_idx);
                let matching: Vec<_> = shape
                    .construct_signatures
                    .iter()
                    .filter(|sig| {
                        let max = sig.type_params.len();
                        let min = sig
                            .type_params
                            .iter()
                            .filter(|tp| tp.default.is_none())
                            .count();
                        got >= min && got <= max
                    })
                    .collect();
                // When multiple overloads match, skip validation (ambiguous).
                if matching.len() > 1 {
                    return false;
                }
                let type_params = if let Some(sig) = matching.first() {
                    sig.type_params.clone()
                } else {
                    shape
                        .construct_signatures
                        .first()
                        .map(|sig| sig.type_params.clone())
                        .unwrap_or_default()
                };

                let max_expected = type_params.len();
                let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();

                if type_params.is_empty() {
                    if got > 0 {
                        self.error_at_node_msg(
                            type_arg_error_anchor,
                            crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                            &["0", &got.to_string()],
                        );
                        return true;
                    }
                    return false;
                }

                if got < min_required || got > max_expected {
                    let expected_str = if min_required == max_expected {
                        max_expected.to_string()
                    } else {
                        format!("{min_required}-{max_expected}")
                    };
                    self.error_at_node_msg(
                        type_arg_error_anchor,
                        crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                        &[&expected_str, &got.to_string()],
                    );
                    return true;
                }

                // Count is correct — check constraints (TS2344).
                self.validate_type_args_against_params(&type_params, type_args_list);
                return false;
            }

        // Function component (call signatures): validate via call type-arg path.
        let result = self.validate_call_type_arguments(resolved, type_args_list, element_idx);
        result.count_mismatch
    }
}

mod constraint_validation;
