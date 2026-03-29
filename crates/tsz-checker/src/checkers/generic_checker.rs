//! Generic type argument validation (TS2344 constraint checking).

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

// =============================================================================
// Generic Type Argument Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
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

        let mut current = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
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
        let mut current = self.ctx.arena.get_extended(arg_idx).map(|ext| ext.parent);
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

                // Case 2: The type arg is derived from the check type (e.g.,
                // `T extends O ? X<ReturnType<T['m']>> : never`). In the true branch,
                // TSC wraps the check type with a substitute type carrying the extends
                // constraint. Any use of the check type in the true branch benefits
                // from this constraint, so TS2344 checks are deferred to instantiation
                // time. Suppress when the arg contains a syntactic reference to the
                // conditional's check type.
                if self.type_node_contains_reference(arg_idx, cond.check_type) {
                    return true;
                }

                // Case 3: The check type wraps the type argument (e.g.,
                // `[T] extends [{ a: string }] ? ... : ...`). In non-distributive
                // conditionals, the check type contains T in a wrapper. In the true
                // branch, T is narrowed by the corresponding component of the extends
                // type. Since constraint satisfaction depends on the narrowing, defer
                // to instantiation time.
                if self.type_node_contains_reference(cond.check_type, arg_idx) {
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
    /// Returns `true` when a type argument count mismatch was detected (TS2558 emitted),
    /// signaling that the caller should skip argument type checking against the
    /// incorrectly-instantiated signature.
    pub(crate) fn validate_call_type_arguments(
        &mut self,
        callee_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        call_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        if let Some(call_expr) = self.ctx.arena.get_call_expr_at(call_idx)
            && let Some(callee_node) = self.ctx.arena.get(call_expr.expression)
            && callee_node.kind == SyntaxKind::SuperKeyword as u16
            && !type_args_list.nodes.is_empty()
        {
            // The parser already reports TS2754 for `super<T>(...)`.
            // Skip re-emitting it here to avoid duplicate diagnostics.
            return false;
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
            return false;
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
            return false;
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
                        return false;
                    }
                }
                // TS2558: Expected 0 type arguments, but got N.
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                    &["0", &got.to_string()],
                );
                // For non-generic functions (0 type params), tsc still proceeds with argument
                // type checking against the original signature. Return false (not a count mismatch)
                // so the caller continues to check argument types.
                return false;
            }
            return false;
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
                return true;
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
            return true;
        }

        self.validate_type_args_against_params(&type_params, type_args_list);
        false
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
            && symbol.flags & symbol_flags::ALIAS != 0
        {
            let mut visited_aliases = Vec::new();
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

        let type_params = self.get_type_params_for_symbol(sym_id);
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
                let lib_binders = self.get_lib_binders();
                let name = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
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
                    &[name.as_str()],
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
        let lib_binders = self.get_lib_binders();
        let base_name = self
            .ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
        let display_name = Self::format_generic_display_name_with_interner(
            &base_name,
            &type_params,
            self.ctx.types,
        );
        let min_required = self
            .count_required_type_params_from_ast(sym_id)
            .unwrap_or_else(|| type_params.iter().filter(|tp| tp.default.is_none()).count());
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
            if self.is_assignable_to(type_arg, constraint) {
                continue;
            }
            let widened_arg =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, type_arg);
            let error_anchor = type_args_list
                .nodes
                .get(i)
                .copied()
                .unwrap_or(type_arg_error_anchor);
            self.error_at_node_msg(
                error_anchor,
                crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[
                    &self.format_type_diagnostic(widened_arg),
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

    /// Validate each type argument against its corresponding type parameter constraint.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    ///
    /// Shared implementation used by call expressions, new expressions, and type references.
    fn validate_type_args_against_params(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args_list: &tsz_parser::parser::NodeList,
    ) {
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
            if let Some(constraint) = param.constraint {
                // Skip constraint checking when the type argument is an error type
                // (avoids cascading errors from unresolved references)
                if type_arg == TypeId::ERROR {
                    continue;
                }

                // Skip constraint checking when the type argument is `this` type.
                // The `this` type is polymorphic (like a type parameter constrained
                // to the enclosing type) and its constraint satisfaction depends on
                // the instantiation context. TSC defers this check, so we should too.
                // Example: `interface Bar extends Foo { other: BoxOfFoo<this>; }`
                // where `BoxOfFoo<T extends Foo>` — `this` satisfies `T extends Foo`
                // because `this` in `Bar` is bounded by `Bar` which extends `Foo`.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg) {
                    continue;
                }

                // Skip constraint checking for `infer` type arguments in conditional
                // types (e.g., `R extends Reducer<any, infer A>`). TSC does not emit
                // TS2344 for infer positions — constraints on inferred type params
                // are checked during conditional type evaluation, not here.
                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                    && arg_node.kind == tsz_parser::parser::syntax_kind_ext::INFER_TYPE
                {
                    continue;
                }

                // Skip constraint checking for `this` type arguments. The polymorphic
                // `this` type is type-parameter-like and its concrete type is only known
                // at instantiation time. TSC defers constraint validation for `this` to
                // instantiation, so we must skip it here to avoid false TS2344 errors.
                // Example: `interface Bar extends Foo { other: BoxOfFoo<this>; }` where
                // `BoxOfFoo<T extends Foo>` — `this` in Bar satisfies `Foo` but we can't
                // prove it structurally at definition time.
                if query::is_this_type(self.ctx.types.as_type_database(), type_arg) {
                    continue;
                }

                // When the type argument contains type parameters, we generally skip
                // constraint checking (deferred to instantiation time). However, when
                // the type arg IS a bare type parameter, check its base constraint
                // against the required constraint. This matches tsc: `U extends number`
                // used as `T extends string` → TS2344 because `number` is not
                // assignable to `string`.
                let type_arg_contains_type_parameters =
                    query::contains_type_parameters(self.ctx.types, type_arg);
                let base_constraint_type = type_arg_contains_type_parameters
                    .then(|| self.constraint_check_base_type(type_arg))
                    .filter(|&base| base != type_arg)
                    // Discard degenerate base constraints (undefined, null, never)
                    // that arise from incomplete evaluation of composite generic types
                    // like NonNullable<T["states"]>[K]. These are artifacts of the
                    // base-constraint resolution failing to see through type-level
                    // applications (NonNullable, Extract, etc.) and should not be used
                    // to make eager TS2344 decisions.
                    .filter(|&base| {
                        base != TypeId::UNDEFINED
                            && base != TypeId::NULL
                            && base != TypeId::NEVER
                            && base != TypeId::VOID
                    });
                if type_arg_contains_type_parameters {
                    let is_bare_type_param =
                        query::is_bare_type_parameter(self.ctx.types.as_type_database(), type_arg);
                    if !is_bare_type_param {
                        // Composite type with type parameters (e.g., `T[K]`, `GetProps<C>`,
                        // `Parameters<Target[K]>`). Prefer checking against its resolved
                        // base constraint when one exists; otherwise defer to instantiation
                        // time. This matches tsc for generic indexed-access cases like
                        // `ReturnType<DataFetchFns[T][F]>` while still avoiding false
                        // positives for unconstrained composite generics.
                        if let Some(base) = base_constraint_type
                            && base != TypeId::UNKNOWN
                            && base != type_arg
                        {
                            // Base constraint still contains type parameters.
                            // For most cases, defer to instantiation time. However,
                            // when the required constraint is a callable signature
                            // (e.g. `(...args: any) => any` for `ReturnType<T>`),
                            // tsc eagerly reports TS2344 if the base type is not
                            // provably callable (e.g. generic indexed access types
                            // like `DataFetchFns[T][F]` are not callable). This
                            // matches tsc behavior for ReturnType/Parameters/etc.
                            if query::contains_type_parameters(self.ctx.types, base) {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let db = self.ctx.types.as_type_database();

                                // Check if the base is a conditional type whose extends
                                // type satisfies the constraint. This check applies
                                // regardless of whether the constraint is callable.
                                // For `Extract<T, C>` (= `T extends C ? T : never`),
                                // the result is always a subtype of C, so if C satisfies
                                // the constraint, skip. If C does NOT satisfy, emit TS2344.
                                //
                                // IMPORTANT: Only apply the eager extends-type check when
                                // the conditional is truly Extract-like (true_type ==
                                // check_type). For general conditionals like
                                // `T extends object ? { [K in keyof T]: T[K] } : never`,
                                // the true branch is a different type from the check type,
                                // so the extends type is NOT a reliable proxy for the
                                // result. Defer those to instantiation time.
                                if let Some((cond_check, cond_extends, cond_true, cond_false)) =
                                    query::full_conditional_type_components(
                                        self.ctx.types.as_type_database(),
                                        base,
                                    )
                                {
                                    if cond_false == TypeId::NEVER {
                                        // Determine if this conditional is "Extract-like":
                                        // the extends type can serve as a proxy for the result.
                                        //
                                        // Extract-like cases (check extends type vs constraint):
                                        // - `T extends C ? T : never` (true == check, classic Extract)
                                        // - `C extends X<infer P> ? P : never` (true is bare type param,
                                        //   opaque — no structural guarantee, e.g. GetProps<C>)
                                        //
                                        // Non-Extract cases (defer to instantiation):
                                        // - `S extends object ? { [K in keyof S]: S[K] } : never`
                                        //   (true branch structurally derived from check type)
                                        // - `T[K] extends Function ? K : never` where K is the
                                        //   index of the check type (key-filtering pattern like
                                        //   FunctionPropertyNames<T>). The result K is always a
                                        //   subtype of keyof T, but the extends type (Function)
                                        //   is not a proxy for this relationship. Defer.
                                        let cond_true_is_bare_param = query::is_bare_type_parameter(
                                            self.ctx.types.as_type_database(),
                                            cond_true,
                                        );
                                        // When the true branch is a bare type param AND the check
                                        // type is an indexed access containing that param as its
                                        // index, this is a key-filtering pattern, not Extract-like.
                                        // Example: `{ [K in keyof T]: T[K] extends Fn ? K : never }[keyof T]`
                                        // Here cond_check = T[K], cond_true = K. K is always a
                                        // key of T, so the result satisfies `keyof T` by construction.
                                        // Deferring avoids false TS2344 for Pick<T, FilteredKeys<T>>.
                                        let is_key_filtering_pattern = cond_true_is_bare_param && {
                                            let db = self.ctx.types.as_type_database();
                                            if let Some((_obj, idx)) =
                                                query::index_access_components(db, cond_check)
                                            {
                                                idx == cond_true
                                            } else {
                                                false
                                            }
                                        };
                                        let is_extract_like = cond_true == cond_check
                                            || (cond_true_is_bare_param
                                                && !is_key_filtering_pattern);
                                        if !is_extract_like {
                                            // True branch is a structural type derived from the
                                            // check type (e.g., mapped type). Constraint satisfaction
                                            // depends on the structure, not the extends type.
                                            // Defer to instantiation time.
                                            continue;
                                        }
                                        let ext_resolved = self.resolve_lazy_type(cond_extends);
                                        let ext_evaluated =
                                            self.evaluate_type_for_assignability(ext_resolved);
                                        if self.is_assignable_to(ext_evaluated, constraint_resolved)
                                            || self
                                                .is_assignable_to(ext_resolved, constraint_resolved)
                                        {
                                            continue;
                                        }
                                        // Extract-like pattern (? T : never) but the
                                        // extends type does NOT satisfy the constraint. tsc
                                        // reports TS2344 in this case. Instantiate constraint
                                        // with type args for accurate error messages.
                                        let mut subst =
                                            crate::query_boundaries::common::TypeSubstitution::new(
                                            );
                                        for (j, p) in type_params.iter().enumerate() {
                                            if let Some(&arg) = type_args.get(j) {
                                                subst.insert(p.name, arg);
                                            }
                                        }
                                        let inst_constraint = if subst.is_empty() {
                                            constraint_resolved
                                        } else {
                                            crate::query_boundaries::common::instantiate_type(
                                                self.ctx.types,
                                                constraint_resolved,
                                                &subst,
                                            )
                                        };
                                        if let Some(&arg_idx) = type_args_list.nodes.get(i)
                                            && !self
                                                .type_argument_is_narrowed_by_conditional_true_branch(
                                                    arg_idx,
                                                    inst_constraint,
                                                )
                                        {
                                            self.error_type_constraint_not_satisfied(
                                                type_arg,
                                                inst_constraint,
                                                arg_idx,
                                            );
                                        }
                                        continue;
                                    } else {
                                        // General conditional with type params — defer
                                        // to instantiation time, matching tsc behavior.
                                        continue;
                                    }
                                }

                                let constraint_is_callable =
                                    query::is_callable_type(db, constraint_resolved);
                                if !constraint_is_callable {
                                    continue;
                                }
                                // Constraint is callable — check if base is callable too.
                                // If base still has type params and is not callable, emit TS2344.
                                // Also try evaluating the base (e.g., mapped type indexed access
                                // like `FunctionsObj<T>[keyof T]` → `() => unknown`).
                                let base_is_callable = query::is_callable_type(db, base);
                                if base_is_callable {
                                    // Base is callable even with type params — satisfied.
                                    continue;
                                }
                                // When the base is an indexed access into a mapped type
                                // (e.g., `{ [K in keyof T]: () => unknown }[keyof T]`),
                                // the template type gives the actual value type. If the
                                // template is callable, the indexed access is callable.
                                if let Some((obj, _idx)) = query::index_access_components(db, base)
                                    && let Some(mapped_id) = query::mapped_type_id(db, obj)
                                    && query::is_mapped_template_callable(db, mapped_id)
                                {
                                    continue;
                                }
                                // Try evaluating base further — indexed access through mapped
                                // types may resolve to a callable template type.
                                let base_evaluated = self.evaluate_type_for_assignability(base);
                                if base_evaluated != base {
                                    let base_eval_callable = query::is_callable_type(
                                        self.ctx.types.as_type_database(),
                                        base_evaluated,
                                    ) || query::callable_shape_for_type(
                                        self.ctx.types.as_type_database(),
                                        base_evaluated,
                                    )
                                    .is_some();
                                    if base_eval_callable {
                                        continue;
                                    }
                                }
                                // Check if base is a mapped type whose template is callable.
                                // For `{ [K in keyof T]: () => unknown }`, the template
                                // `() => unknown` is callable, so indexing yields a callable type.
                                if let Some(template) = query::mapped_type_template(db, base) {
                                    let template_evaluated =
                                        self.evaluate_type_for_assignability(template);
                                    let template_callable = query::is_callable_type(
                                        self.ctx.types.as_type_database(),
                                        template_evaluated,
                                    ) || query::callable_shape_for_type(
                                        self.ctx.types.as_type_database(),
                                        template_evaluated,
                                    )
                                    .is_some();
                                    if template_callable {
                                        continue;
                                    }
                                }
                                // When the base is an indexed access into a type
                                // parameter (e.g., `FuncMap[keyof FuncMap]`), we cannot
                                // determine callability at definition time. The type
                                // parameter's constraint may guarantee callable values
                                // (e.g., `FuncMap extends Record<string, Function>`),
                                // but we can't fully resolve this without instantiation.
                                // Defer to instantiation time to avoid false TS2344.
                                if let Some((obj, _idx)) = query::index_access_components(db, base)
                                    && query::is_bare_type_parameter(db, obj)
                                {
                                    continue;
                                }
                                // Base is not callable and constraint is callable → TS2344.
                                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                                    && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                        arg_idx,
                                        constraint_resolved,
                                    )
                                {
                                    self.error_type_constraint_not_satisfied(
                                        type_arg,
                                        constraint_resolved,
                                        arg_idx,
                                    );
                                }
                                continue;
                            }
                            // When the type argument is an Application type
                            // (e.g., `Merge2<X>`, `Same<U>`) containing type
                            // parameters, the base constraint was obtained by
                            // eagerly evaluating the application with type
                            // parameter constraints substituted. This may
                            // produce a concrete type that doesn't accurately
                            // represent the actual type at instantiation time
                            // (e.g., mapped types like `{ [P in keyof T]: T[P] }`
                            // preserve index signatures from T, but the eagerly-
                            // resolved base may lose this relationship). TSC
                            // defers constraint checking for such Application
                            // types to instantiation time.
                            if query::is_application_type(
                                self.ctx.types.as_type_database(),
                                type_arg,
                            ) {
                                continue;
                            }
                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            let mut subst =
                                crate::query_boundaries::common::TypeSubstitution::new();
                            for (j, p) in type_params.iter().enumerate() {
                                if let Some(&arg) = type_args.get(j) {
                                    subst.insert(p.name, arg);
                                }
                            }
                            let inst_constraint = if subst.is_empty() {
                                constraint_resolved
                            } else {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    constraint_resolved,
                                    &subst,
                                )
                            };
                            if query::contains_type_parameters(self.ctx.types, inst_constraint) {
                                continue;
                            }
                            let db = self.ctx.types.as_type_database();
                            let original_constraint = param.constraint.unwrap_or(TypeId::NEVER);

                            // Special case: tsc eagerly reports TS2344 for generic indexed access
                            // types (A[B] where A contains type params) when the constraint is
                            // callable, even if the evaluated base constraint is callable.
                            // Example: `ReturnType<DataFetchFns[T][F]>` → TS2344 because
                            // `DataFetchFns[T][F]` is not provably callable (T is free).
                            // By contrast, `ReturnType<DataFetchFns['Boat'][F]>` → no TS2344
                            // because 'Boat' is concrete and all its values are callable.
                            let constraint_is_callable =
                                query::is_callable_type(db, inst_constraint)
                                    || self.is_function_constraint(original_constraint);
                            if constraint_is_callable
                                && self.is_generic_indexed_access(type_arg)
                                && !self.indexed_access_resolves_to_callable(type_arg)
                                && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                )
                            {
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                                continue;
                            }

                            // When the base constraint has no type parameters but the
                            // original type argument did, constraint resolution fully
                            // substituted type params with their constraints. This
                            // substitution is lossy — mapped types and intersections
                            // may lose index signature relationships that hold at
                            // instantiation time (e.g., `{ [P in keyof T]: T[P] }`
                            // preserves T's index signatures, but constraint resolution
                            // may produce inconsistent index signatures). For non-callable
                            // constraints, defer to instantiation time to match tsc.
                            if !query::contains_type_parameters(self.ctx.types, base)
                                && !query::is_callable_type(db, inst_constraint)
                                && !self.is_function_constraint(original_constraint)
                            {
                                // Check if the type arg is an Application type (type alias
                                // instantiation). These are especially prone to lossy
                                // constraint resolution because the type alias body may
                                // structurally preserve constraints that the base constraint
                                // computation cannot track.
                                let type_arg_is_application = query::application_base_def_id(
                                    self.ctx.types.as_type_database(),
                                    type_arg,
                                )
                                .is_some();
                                if type_arg_is_application {
                                    continue;
                                }
                            }

                            let mut is_satisfied = self.is_assignable_to(base, inst_constraint)
                                || self.satisfies_array_like_constraint(base, inst_constraint);
                            if !is_satisfied {
                                // When the constraint is a function type (e.g., `(...args: any) => any`),
                                // accept any callable base type. For type parameters with callable
                                // constraints (e.g., `F extends Function`), check the constraint.
                                // Also check the structural Function interface pattern (apply/call/bind)
                                // since Function may be lowered as an Object without call signatures.
                                let is_fn_constraint = self
                                    .is_function_constraint(original_constraint)
                                    || query::is_callable_type(db, original_constraint);
                                let base_is_callable = query::is_callable_type(db, base)
                                    || self.type_parameter_has_callable_constraint(base)
                                    || self.is_function_constraint(base)
                                    || query::is_function_interface_structural(db, base);
                                is_satisfied = is_fn_constraint && base_is_callable;
                            }
                            if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                                if self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                ) {
                                    continue;
                                }
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                            }
                        }
                        // When base_constraint_type is None (composite type with type params
                        // that can't be simplified further), check if the required constraint
                        // is callable. Tsc eagerly emits TS2344 when the constraint is a
                        // callable signature and the composite type arg is not provably callable.
                        // Example: `ReturnType<TypeHardcodedAsParameterWithoutReturnType<T,F>>`
                        // where `TypeHardcodedAsParameterWithoutReturnType<T,F>` = `DataFetchFns[T][F]`.
                        //
                        // The constraint TypeId may come from a lib arena (cross-arena). Resolve
                        // it fully and evaluate before checking callability.
                        if base_constraint_type.is_none() {
                            // When the type argument is (or evaluates to) a conditional
                            // type like `Extract<T, C>` (= `T extends C ? T : never`),
                            // the result is always a subtype of C (or never). If C
                            // satisfies the required constraint, skip TS2344.
                            // Also handles Application types like `Extract<T, C>` that
                            // evaluate to conditional types.
                            let db = self.ctx.types.as_type_database();
                            let type_arg_evaluated = self.evaluate_type_for_assignability(type_arg);
                            let cond_components = query::conditional_type_components(db, type_arg)
                                .or_else(|| {
                                    query::conditional_type_components(
                                        self.ctx.types.as_type_database(),
                                        type_arg_evaluated,
                                    )
                                });
                            if let Some((extends_type, false_type)) = cond_components {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                let extends_resolved = self.resolve_lazy_type(extends_type);
                                let extends_evaluated =
                                    self.evaluate_type_for_assignability(extends_resolved);
                                // If false branch is `never` (Extract pattern) and the
                                // extends type satisfies the constraint, skip TS2344.
                                if false_type == TypeId::NEVER
                                    && (self
                                        .is_assignable_to(extends_evaluated, constraint_resolved)
                                        || self.is_assignable_to(
                                            extends_resolved,
                                            constraint_resolved,
                                        ))
                                {
                                    // Skip: Extract<T, C> always produces subtype of C
                                } else {
                                    // General conditional: defer to instantiation when
                                    // the type argument has unresolved type parameters.
                                    // tsc defers constraint checks for conditional types
                                    // with free type variables.
                                }
                            } else {
                                let constraint_resolved = self.resolve_lazy_type(constraint);
                                // Also try evaluating the constraint in case it's a lazy reference
                                // to a function type from the lib (e.g., `(...args: any) => any`).
                                let constraint_evaluated =
                                    self.evaluate_type_for_assignability(constraint_resolved);
                                let constraint_is_callable =
                                    query::is_callable_type(db, constraint_resolved)
                                        || query::is_callable_type(db, constraint_evaluated)
                                        || self.is_function_constraint(constraint)
                                        || self.is_function_constraint(constraint_resolved);
                                // For indexed access types like `T[M]` where T's constraint
                                // is a mapped type with a callable template, the indexed
                                // access result is callable — skip TS2344.
                                // Example: `ReturnType<T[M]>` where
                                // `T extends { [K in keyof T]: () => unknown }`.
                                let type_arg_is_callable_via_mapped = constraint_is_callable
                                    && self.indexed_access_resolves_to_callable(type_arg);
                                // When the type arg is an indexed access into a type
                                // parameter (e.g., `FuncMap[P]`), the result type depends
                                // on the type parameter's actual type at instantiation
                                // time. We cannot determine callability at definition
                                // time — defer to instantiation to avoid false TS2344.
                                let type_arg_is_indexed_into_type_param = {
                                    let db2 = self.ctx.types.as_type_database();
                                    query::index_access_components(db2, type_arg).is_some_and(
                                        |(obj, _)| query::is_bare_type_parameter(db2, obj),
                                    )
                                };
                                if constraint_is_callable
                                    && !type_arg_is_callable_via_mapped
                                    && !type_arg_is_indexed_into_type_param
                                    && !query::is_callable_type(db, type_arg)
                                    && query::callable_shape_for_type(db, type_arg).is_none()
                                    && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                    && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                        arg_idx,
                                        constraint_resolved,
                                    )
                                {
                                    self.error_type_constraint_not_satisfied(
                                        type_arg,
                                        constraint_resolved,
                                        arg_idx,
                                    );
                                }
                            }
                        }
                        continue;
                    }
                    if is_bare_type_param && base_constraint_type.is_none() {
                        // Bare `Infer` type parameter — base_constraint_of_type returns
                        // the type unchanged for Infer types, so base_constraint_type is
                        // None. Check if the infer variable has an implicit constraint
                        // from its structural position (e.g., template literal → string,
                        // rest element → array). If so, skip TS2344 — tsc defers these
                        // checks to conditional type evaluation.
                        let has_implicit_constraint =
                            type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                self.has_hidden_conditional_infer_constraint_local(arg_idx)
                            });
                        if has_implicit_constraint {
                            continue;
                        }
                    }
                    if is_bare_type_param && let Some(base) = base_constraint_type {
                        // Bare type parameter — check its base constraint instead of
                        // eagerly validating the unresolved type parameter itself.
                        if base == TypeId::UNKNOWN {
                            // Base constraint is UNKNOWN. This can mean either:
                            // (a) The type param is truly unconstrained (no `extends`)
                            // (b) The constraint wasn't resolved (cross-arena,
                            //     function type params, mapped type keys, etc.)
                            //
                            // For case (a), tsc reports TS2344 when the required
                            // constraint is non-trivial. For case (b), we must
                            // skip to avoid false positives.
                            //
                            // Detect case (b) by checking if the type arg's AST
                            // source has an explicit constraint or is inside a
                            // mapped type body (implicit constraint).
                            let has_hidden_constraint =
                                type_args_list.nodes.get(i).copied().is_some_and(|arg_idx| {
                                    self.is_inside_mapped_type(arg_idx)
                                        || self.type_arg_has_explicit_constraint_in_ast(arg_idx)
                                        || self
                                            .has_hidden_conditional_infer_constraint_local(arg_idx)
                                });
                            if has_hidden_constraint {
                                continue;
                            }

                            let constraint_resolved = self.resolve_lazy_type(constraint);
                            let mut subst =
                                crate::query_boundaries::common::TypeSubstitution::new();
                            for (j, p) in type_params.iter().enumerate() {
                                if let Some(&arg) = type_args.get(j) {
                                    subst.insert(p.name, arg);
                                }
                            }
                            let inst_constraint = if subst.is_empty() {
                                constraint_resolved
                            } else {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    constraint_resolved,
                                    &subst,
                                )
                            };
                            // Skip trivial constraints (unknown/any) and bare type
                            // parameter constraints (deferred to instantiation).
                            let is_checkable = inst_constraint != TypeId::UNKNOWN
                                && inst_constraint != TypeId::ANY
                                && !query::is_bare_type_parameter(
                                    self.ctx.types.as_type_database(),
                                    inst_constraint,
                                );
                            if is_checkable
                                && !self.is_assignable_to(base, inst_constraint)
                                && !self.satisfies_array_like_constraint(base, inst_constraint)
                                && let Some(&arg_idx) = type_args_list.nodes.get(i)
                                && !self.type_argument_is_narrowed_by_conditional_true_branch(
                                    arg_idx,
                                    inst_constraint,
                                )
                            {
                                self.error_type_constraint_not_satisfied(
                                    type_arg,
                                    inst_constraint,
                                    arg_idx,
                                );
                            }
                            continue;
                        }
                        // If the base constraint is a union, skip. Union-constrained type
                        // params often appear in conditional types where the true branch
                        // narrows to a specific union member. Checking the full union
                        // against the narrowed constraint would produce false positives.
                        if query::has_union_members(self.ctx.types.as_type_database(), base) {
                            continue;
                        }
                        if query::contains_type_parameters(self.ctx.types, base) {
                            // Base constraint itself contains type parameters
                            // (e.g., from outer generic scope). Defer check.
                            continue;
                        }
                        let constraint_resolved = self.resolve_lazy_type(constraint);
                        if query::contains_type_parameters(self.ctx.types, constraint_resolved) {
                            continue;
                        }
                        // Instantiate the constraint with all provided type arguments
                        let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
                        for (j, p) in type_params.iter().enumerate() {
                            if let Some(&arg) = type_args.get(j) {
                                subst.insert(p.name, arg);
                            }
                        }
                        let inst_constraint = if subst.is_empty() {
                            constraint_resolved
                        } else {
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                constraint_resolved,
                                &subst,
                            )
                        };
                        if query::contains_type_parameters(self.ctx.types, inst_constraint) {
                            continue;
                        }
                        let mut is_satisfied = self.is_assignable_to(base, inst_constraint)
                            || self.satisfies_array_like_constraint(base, inst_constraint);
                        if !is_satisfied {
                            // When the constraint is a function type, accept callable bases.
                            // The `Function` interface may be lowered as an Object type
                            // (without call signatures), so also check for the structural
                            // pattern (apply/call/bind properties).
                            let db2 = self.ctx.types.as_type_database();
                            let is_fn_constraint = self.is_function_constraint(inst_constraint)
                                || query::is_callable_type(db2, inst_constraint);
                            let base_is_callable = query::is_callable_type(db2, base)
                                || self.type_parameter_has_callable_constraint(base)
                                || self.is_function_constraint(base)
                                || query::is_function_interface_structural(db2, base);
                            is_satisfied = is_fn_constraint && base_is_callable;
                        }
                        if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                            if self.type_argument_is_narrowed_by_conditional_true_branch(
                                arg_idx,
                                inst_constraint,
                            ) {
                                continue;
                            }
                            self.error_type_constraint_not_satisfied(
                                type_arg,
                                inst_constraint,
                                arg_idx,
                            );
                        }
                        continue;
                    }
                }

                // Resolve the constraint in case it's a Lazy type
                let constraint = self.resolve_lazy_type(constraint);

                if let Some(&arg_idx) = type_args_list.nodes.get(i)
                    && self
                        .type_argument_is_narrowed_by_conditional_true_branch(arg_idx, constraint)
                {
                    continue;
                }

                // Evaluate type arguments before substitution so that unevaluated
                // IndexAccess types (e.g., `SettingsTypes["audio" | "video"]`) are
                // resolved to their concrete types. This prevents the instantiated
                // constraint from containing unresolvable Lazy(DefId) references
                // inside nested types (KeyOf, IndexAccess, Mapped).
                let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
                for (j, p) in type_params.iter().enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        let evaluated_arg = self.evaluate_type_with_env(arg);
                        subst.insert(p.name, evaluated_arg);
                    }
                }
                let instantiated_constraint = if subst.is_empty() {
                    constraint
                } else {
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        constraint,
                        &subst,
                    )
                };
                // Skip if the instantiated constraint still contains type parameters.
                // This avoids false positive TS2344 when the constraint cannot be fully
                // resolved (e.g., conditional type narrowing contexts like
                // `Parameters<Target[K]>` inside a `Target[K] extends Function` branch).
                if query::contains_type_parameters(self.ctx.types, instantiated_constraint) {
                    continue;
                }

                let mut is_satisfied = self.is_assignable_to(type_arg, instantiated_constraint);

                // Fallback for recursive generic constraints (coinductive semantics).
                //
                // For self-referential constraints like `T extends AA<T>` in
                // `interface AA<T extends AA<T>>`, checking if a type arg satisfies
                // the constraint leads to circular structural checks that the
                // subtype checker can't resolve (pre-evaluation destroys DefId
                // identity needed for cycle detection).
                //
                // Coinductive fix: if the constraint is an Application of some base
                // interface, and the type arg's interface extends that same base
                // interface (via heritage), the constraint is coinductively satisfied.
                // e.g., for `interface BB extends AA<AA<BB>>`, BB extends AA, so
                // BB satisfies any AA<...> constraint.
                if !is_satisfied {
                    is_satisfied = self
                        .satisfies_recursive_heritage_constraint(type_arg, instantiated_constraint);
                }

                // Fallback: if assignability failed but the constraint is the Function
                // interface and the type argument is callable, accept it. This handles
                // the case where Function has multiple TypeIds that aren't recognized
                // as equivalent during assignability checking (RefCell borrow conflict
                // prevents boxed type lookup during type evaluation).
                if !is_satisfied {
                    // Check original (pre-resolution) constraint which may still be
                    // Lazy(DefId), making it easier to identify via boxed DefId lookup.
                    let original_constraint = param.constraint.unwrap_or(TypeId::NEVER);
                    let db = self.ctx.types.as_type_database();
                    is_satisfied = self.is_function_constraint(original_constraint)
                        && query::is_callable_type(db, type_arg);
                }
                if !is_satisfied {
                    is_satisfied =
                        self.satisfies_array_like_constraint(type_arg, instantiated_constraint);
                }
                if !is_satisfied
                    && let Some(base) = base_constraint_type
                    && base != TypeId::UNKNOWN
                    && !query::contains_type_parameters(self.ctx.types, base)
                {
                    is_satisfied = self.is_assignable_to(base, instantiated_constraint)
                        || self.satisfies_array_like_constraint(base, instantiated_constraint);
                }

                if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    if self.type_argument_is_narrowed_by_conditional_true_branch(
                        arg_idx,
                        instantiated_constraint,
                    ) {
                        continue;
                    }
                    // Check if the failure is due to a weak type violation (TS2559).
                    // In tsc, when the constraint is a "weak type" (all-optional properties)
                    // and the type argument shares no common properties, tsc emits TS2559
                    // instead of TS2344. However, primitive types satisfy weak type
                    // constraints in tsc (e.g., `bigint extends {t?: string}` is valid).
                    let analysis =
                        self.analyze_assignability_failure(type_arg, instantiated_constraint);
                    if matches!(
                        analysis.failure_reason,
                        Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                    ) {
                        // Primitives satisfy weak type constraints — skip TS2559
                        if !query::is_primitive_type(self.ctx.types.as_type_database(), type_arg) {
                            self.error_no_common_properties_constraint(
                                type_arg,
                                instantiated_constraint,
                                arg_idx,
                            );
                        }
                    } else {
                        self.error_type_constraint_not_satisfied(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
                    }
                }
            }
        }
    }

    fn constraint_check_base_type(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated != type_id {
            return self.constraint_check_base_type(evaluated);
        }

        let db = self.ctx.types.as_type_database();
        // For TypeParameter: returns constraint or UNKNOWN; for non-TypeParameter: returns type_id
        let base = query::base_constraint_of_type(db, type_id);
        if base != type_id {
            return self.evaluate_type_for_assignability(base);
        }
        if let Some((object_type, index_type)) = query::index_access_components(db, type_id) {
            let constrained_object_type =
                if query::is_bare_type_parameter(self.ctx.types.as_type_database(), object_type) {
                    self.constraint_check_base_type(object_type)
                } else {
                    object_type
                };
            let constrained_index_type = self.constraint_check_base_type(index_type);
            let resolved_object_type = if constrained_object_type == TypeId::UNKNOWN {
                object_type
            } else {
                constrained_object_type
            };
            let resolved_index_type = if constrained_index_type == TypeId::UNKNOWN {
                index_type
            } else {
                constrained_index_type
            };
            if let Some(indexed_value_type) = self.constraint_check_indexed_access_value_type(
                resolved_object_type,
                resolved_index_type,
            ) {
                return self.evaluate_type_for_assignability(indexed_value_type);
            }
            if resolved_object_type == object_type && resolved_index_type == index_type {
                type_id
            } else {
                let constrained_access = self
                    .ctx
                    .types
                    .index_access(resolved_object_type, resolved_index_type);
                self.evaluate_type_for_assignability(constrained_access)
            }
        } else {
            type_id
        }
    }

    fn constraint_check_indexed_access_value_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        let object_type = self.evaluate_type_for_assignability(object_type);
        let key_type = self.evaluate_type_for_assignability(index_type);
        let key_kind = query::classify_index_key(db, key_type);

        if let Some(shape) = query::get_object_shape(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        if let Some(shape) = query::callable_shape_for_type(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        // For mapped types `{ [K in C]: Template }`, the indexed access value
        // type is the template type. This handles cases like
        // `FunctionsObj<T>[keyof T]` where FunctionsObj is `{ [K in keyof T]: () => unknown }`.
        if let Some(template) = query::mapped_type_template(db, object_type) {
            return Some(template);
        }

        None
    }

    /// Check if a type represents the global `Function` interface from lib.d.ts.
    ///
    /// Checks via Lazy(DefId) against the interner's registered boxed `DefIds`,
    /// or by direct TypeId match against the interner's registered boxed type.
    fn is_function_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        // Direct match against interner's boxed Function TypeId
        if query::is_boxed_function_type(db, type_id) {
            return true;
        }
        // A function signature type (e.g., `(...args: any) => any`) is also a
        // function constraint. This handles cases like `Parameters<F>` where
        // the constraint is `T extends (...args: any) => any` and F extends Function.
        if query::is_callable_type(db, type_id) {
            return true;
        }
        // Cross-arena DefId equality alone is not strong enough here: imported
        // aliases can reuse a Lazy(DefId) shape that collides with boxed lib
        // DefIds, which falsely classifies constraints like `Key` as `Function`.
        // Guard the fallback by requiring the rendered type name to actually be
        // `Function`.
        if self.format_type(type_id) != "Function" {
            return false;
        }
        // Check if the type is Lazy(DefId) with a known Function boxed DefId
        query::is_boxed_function_def(db, type_id)
    }

    /// Check if a type parameter has a callable constraint (e.g., `F extends Function`).
    /// Used during constraint satisfaction to accept callable type parameters
    /// against function signature constraints.
    fn type_parameter_has_callable_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some(tsz_solver::TypeData::TypeParameter(tp)) = db.lookup(type_id) {
            if let Some(constraint) = tp.constraint {
                return query::is_callable_type(db, constraint)
                    || self.is_function_constraint(constraint);
            }
        }
        false
    }

    /// Check if a type is a generic indexed access (`T[M]`) where the object
    /// Check if a type is a "generic indexed access" — an `IndexAccess(A, B)` where
    /// the object part `A` contains free type parameters.
    ///
    /// tsc treats such types as not provably callable at definition time, even if
    /// substituting the type parameter's constraint would produce a callable union.
    /// For example, `DataFetchFns[T][F]` is a generic indexed access (T is a free
    /// type param in the object), while `DataFetchFns['Boat'][F]` is not (the object
    /// `DataFetchFns['Boat']` is concrete).
    fn is_generic_indexed_access(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some((object, _)) = query::index_access_components(db, type_id) {
            return query::contains_type_parameters(self.ctx.types, object);
        }
        false
    }

    /// Check if an indexed access type `T[M]` resolves to a callable type
    /// through its constraint chain. This handles cases like:
    /// `T[M]` where `T extends { [K in keyof T]: () => unknown }` and `M extends keyof T`.
    /// The mapped type template `() => unknown` is callable, so `T[M]` resolves
    /// to a callable type. It also handles callable string/number index
    /// signatures like `T extends { [key: string]: (...args: any) => void }`.
    fn indexed_access_resolves_to_callable(&mut self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((object, _index)) = query::index_access_components(db, type_id) else {
            return false;
        };
        // Resolve the object type's constraint chain to find a mapped type
        let object_constraint = if query::is_bare_type_parameter(db, object) {
            let base = query::base_constraint_of_type(db, object);
            if base != object {
                self.evaluate_type_for_assignability(base)
            } else {
                return false;
            }
        } else {
            return false;
        };
        let db = self.ctx.types.as_type_database();
        // Check if the resolved constraint is a mapped type with callable template
        if let Some(template) = query::mapped_type_template(db, object_constraint) {
            let template_eval = self.evaluate_type_for_assignability(template);
            let db2 = self.ctx.types.as_type_database();
            return query::is_callable_type(db2, template_eval)
                || query::callable_shape_for_type(db2, template_eval).is_some()
                || query::is_callable_type(db2, template);
        }
        for value_type in query::index_signature_value_types(db, object_constraint)
            .into_iter()
            .flatten()
        {
            let value_eval = self.evaluate_type_for_assignability(value_type);
            let db2 = self.ctx.types.as_type_database();
            if query::is_callable_type(db2, value_eval)
                || query::callable_shape_for_type(db2, value_eval).is_some()
                || query::is_callable_type(db2, value_type)
                || query::callable_shape_for_type(db2, value_type).is_some()
            {
                return true;
            }
        }
        false
    }

    fn satisfies_array_like_constraint(&mut self, source: TypeId, target: TypeId) -> bool {
        let source = self.evaluate_type_for_assignability(source);
        let target = self.evaluate_type_for_assignability(target);
        let target_elem = crate::query_boundaries::checkers::call::array_element_type_for_type(
            self.ctx.types,
            target,
        )
        .unwrap_or_else(|| self.get_element_access_type(target, TypeId::NUMBER, Some(0)));
        if target_elem == TypeId::ERROR {
            return false;
        }

        if !self.is_array_like_type(source) && !self.has_structural_array_surface(source, target) {
            return false;
        }

        if target_elem == TypeId::ANY {
            return true;
        }

        let source_elem = self.get_element_access_type(source, TypeId::NUMBER, Some(0));
        source_elem != TypeId::ERROR && self.is_assignable_to(source_elem, target_elem)
    }

    /// Check if a type argument coinductively satisfies a recursive constraint
    /// via its heritage chain.
    ///
    /// When an interface extends a generic base (e.g., `interface BB extends AA<AA<BB>>`),
    /// and the constraint is an Application of that same base (e.g., `AA<BB>`), the
    /// structural subtype check becomes circular. The subtype checker can't detect the
    /// cycle because pre-evaluation destroys DefId identity. This method detects the
    /// pattern and returns true (coinductive assumption).
    fn satisfies_recursive_heritage_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();

        // Get the Application base DefId from the constraint.
        // e.g., for AA<BB>, get the DefId of AA.
        let Some(constraint_base_def) = query::application_base_def_id(db, constraint) else {
            return false;
        };

        // Get the type_arg's DefId (it must be an interface/class, i.e., Lazy type).
        let type_arg_def = query::lazy_def_id(db, type_arg);
        let Some(type_arg_def) = type_arg_def else {
            // When the type_arg is an Application (e.g., AA<BB>) with the same base
            // as the constraint (e.g., AA<AA<BB>>), AND the inner type arguments of
            // the Application type_arg extend the constraint base via heritage, the
            // constraint is coinductively satisfied. This handles recursive constraints
            // like `T extends AA<T>` where `interface BB extends AA<AA<BB>>` — checking
            // `AA<BB>` against `AA<AA<BB>>` leads to infinite nesting that tsc resolves
            // via deeply-nested type detection.
            if let Some((Some(type_arg_base_def), ref type_arg_args)) =
                query::application_base_def_and_args(db, type_arg)
                && type_arg_base_def == constraint_base_def
            {
                // Same base type (e.g., both are AA<...>).
                // Check if any inner type argument extends the constraint base,
                // which would create the circular recursion pattern.
                for &inner_arg in type_arg_args.iter() {
                    if let Some(inner_def) = query::lazy_def_id(db, inner_arg) {
                        let inner_sym = self.ctx.def_to_symbol_id(inner_def);
                        let constraint_sym = self.ctx.def_to_symbol_id(constraint_base_def);
                        if let (Some(inner_sym_id), Some(constraint_sym_id)) =
                            (inner_sym, constraint_sym)
                            && self.interface_extends_symbol(inner_sym_id, constraint_sym_id)
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        };

        // Resolve DefIds to SymbolIds
        let type_arg_sym = self.ctx.def_to_symbol_id(type_arg_def);
        let constraint_base_sym = self.ctx.def_to_symbol_id(constraint_base_def);

        let (Some(type_arg_sym_id), Some(constraint_base_sym_id)) =
            (type_arg_sym, constraint_base_sym)
        else {
            return false;
        };

        // Check if type_arg's interface heritage chain includes the constraint's
        // base interface. Walk the heritage clauses in the binder to find if BB
        // extends any instantiation of AA.
        self.interface_extends_symbol(type_arg_sym_id, constraint_base_sym_id)
    }

    /// Check if an interface symbol extends (directly or transitively) a target symbol.
    fn interface_extends_symbol(
        &self,
        interface_sym_id: tsz_binder::SymbolId,
        target_sym_id: tsz_binder::SymbolId,
    ) -> bool {
        if interface_sym_id == target_sym_id {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(interface_sym_id) else {
            return false;
        };

        // Check each declaration's heritage clauses
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    // Extract the base expression (might be an ExpressionWithTypeArguments)
                    let expr_idx = if let Some(eta) = self.ctx.arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_sym) = self.resolve_heritage_symbol(expr_idx)
                        && base_sym == target_sym_id
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn has_structural_array_surface(&self, source: TypeId, target: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();

        let Some(shape) = query::get_object_shape(db, source) else {
            return false;
        };
        if shape.number_index.is_none() {
            return false;
        }

        for name in ["length", "concat", "slice"] {
            if !query::has_property_by_name(db, source, name) {
                return false;
            }
        }

        if !matches!(
            query::classify_array_like(db, target),
            query::ArrayLikeKind::Readonly(_)
        ) && !query::has_property_by_name(db, source, "push")
        {
            return false;
        }

        true
    }

    /// Check if a symbol's declaration has type parameters, even if they couldn't be
    /// resolved via `get_type_params_for_symbol` (e.g., cross-arena lib types).
    pub(crate) fn symbol_declaration_has_type_parameters(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };

        // Check the value declaration and all declarations for type parameters
        let decl_indices: Vec<_> = if symbol.value_declaration.is_some() {
            std::iter::once(symbol.value_declaration)
                .chain(symbol.declarations.iter().copied())
                .collect()
        } else {
            symbol.declarations.clone()
        };

        for decl_idx in decl_indices {
            // Try current arena first
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if let Some(ta) = self.ctx.arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = self.ctx.arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = self.ctx.arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try cross-arena (lib files)
            if let Some(decl_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try declaration_arenas
            if let Some(decl_arena) = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }
        }

        false
    }
}
