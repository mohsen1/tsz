//! Generic type argument validation (TS2344 constraint checking).

use crate::query_boundaries::checkers::generic as query;
use crate::query_boundaries::common as common_query;
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
        let mut current = self.ctx.arena.get_extended(arg_idx).map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                && self.is_descendant_type_node(arg_idx, cond.true_type)
                && self.type_nodes_structurally_equal(arg_idx, cond.check_type)
            {
                let extends_type = self.get_type_from_type_node(cond.extends_type);
                if extends_type != TypeId::ERROR
                    && (extends_type == constraint
                        || (self.is_assignable_to(extends_type, constraint)
                            && self.is_assignable_to(constraint, extends_type)))
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
        false
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
            // TSC reports this error spanning from the `<` of the type
            // argument list to the `)` of the call. Approximate by using
            // the callee node's end (just after `super`) as the start and
            // the call expression end as the span end. This covers `<T>(x)`.
            let callee_end = callee_node.end;
            let call_node_end = self.ctx.arena.get(call_idx).map_or(callee_end, |n| n.end);
            let span_length = call_node_end.saturating_sub(callee_end);
            self.error_at_position(
                callee_end,
                span_length,
                crate::diagnostics::diagnostic_messages::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                crate::diagnostics::diagnostic_codes::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
            );
            // super<T>() is always invalid — not a count mismatch per se,
            // but the caller already handles the super bail-out separately.
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
        let Some(type_params) = tsz_solver::type_queries::data::extract_type_params_for_call(
            self.ctx.types,
            callee_type,
            got,
        ) else {
            // None = multiple overloads match or not a callable type; skip validation.
            return false;
        };

        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();

        if type_params.is_empty() {
            // TS2558: Expected 0 type arguments, but got N.
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

        let got = type_args_list.nodes.len();
        let type_arg_error_anchor = self
            .ctx
            .arena
            .get(type_ref_idx)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            .map(|type_ref| type_ref.type_name)
            .unwrap_or(type_ref_idx);
        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();
        if got < min_required || got > max_expected {
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
            if min_required < max_expected {
                // TS2707: Generic type 'X<T, U, V>' requires between N and M type arguments.
                let min_str = min_required.to_string();
                let max_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[&display_name, &min_str, &max_str],
                );
            } else {
                // TS2314: Generic type 'X<T, U>' requires N type argument(s).
                let count_str = max_expected.to_string();
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[&display_name, &count_str],
                );
            }
            return true;
        }

        if self.type_args_reference_resolving_alias(type_args_list) {
            let lib_binders = self.get_lib_binders();
            let name = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());

            self.error_at_node_msg(
                type_ref_idx,
                crate::diagnostics::diagnostic_codes::TYPE_ARGUMENTS_FOR_CIRCULARLY_REFERENCE_THEMSELVES,
                &[name.as_str()],
            );
        }

        // Validate type arguments against their constraints
        self.validate_type_args_against_params(&type_params, type_args_list);

        false
    }

    fn type_args_reference_resolving_alias(
        &self,
        type_args_list: &tsz_parser::parser::NodeList,
    ) -> bool {
        type_args_list
            .nodes
            .iter()
            .copied()
            .any(|arg_idx| self.type_node_references_resolving_alias(arg_idx))
    }

    fn type_node_references_resolving_alias(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            || node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
        {
            let sym_id = if node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(node).and_then(|type_ref| {
                    self.resolve_type_symbol_for_lowering(type_ref.type_name)
                        .map(tsz_binder::SymbolId)
                })
            } else {
                self.resolve_type_symbol_for_lowering(node_idx)
                    .map(tsz_binder::SymbolId)
            };

            if let Some(sym_id) = sym_id
                && self.ctx.symbol_resolution_set.contains(&sym_id)
                && self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0)
            {
                return true;
            }
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.type_node_references_resolving_alias(child_idx) {
                return true;
            }
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

        if type_params.is_empty() {
            // TS2558: Expected 0 type arguments, but got N.
            if got > 0 {
                self.error_at_node_msg(
                    type_arg_error_anchor,
                    crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                    &["0", &got.to_string()],
                );
            }
            return;
        }

        let max_expected = type_params.len();
        let min_required = type_params.iter().filter(|tp| tp.default.is_none()).count();
        if got < min_required || got > max_expected {
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
                    .filter(|&base| base != type_arg);
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
                            // Base constraint still contains type parameters — defer
                            // to instantiation time (matches tsc behavior).
                            if query::contains_type_parameters(self.ctx.types, base) {
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
                            let mut is_satisfied = self.is_assignable_to(base, inst_constraint)
                                || self.satisfies_array_like_constraint(base, inst_constraint);
                            if !is_satisfied {
                                is_satisfied = self.is_function_constraint(original_constraint)
                                    && tsz_solver::type_queries::is_callable_type(db, base);
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
                        continue;
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
                                        || self.is_infer_with_implicit_constraint_in_conditional(arg_idx)
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
                        if tsz_solver::type_queries::get_union_members(
                            self.ctx.types.as_type_database(),
                            base,
                        )
                        .is_some()
                        {
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
                        if !self.is_assignable_to(base, inst_constraint)
                            && !self.satisfies_array_like_constraint(base, inst_constraint)
                            && let Some(&arg_idx) = type_args_list.nodes.get(i)
                        {
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
                        && tsz_solver::type_queries::is_callable_type(db, type_arg);
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
                        if !tsz_solver::is_primitive_type(self.ctx.types, type_arg) {
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
        use tsz_solver::type_queries::IndexKeyKind;

        fn key_matches_string_index(
            db: &dyn tsz_solver::TypeDatabase,
            key_type: TypeId,
            kind: &IndexKeyKind,
        ) -> bool {
            match kind {
                IndexKeyKind::String
                | IndexKeyKind::Number
                | IndexKeyKind::StringLiteral
                | IndexKeyKind::NumberLiteral
                | IndexKeyKind::NumericStringLike
                | IndexKeyKind::TemplateLiteralString => true,
                IndexKeyKind::Union(members) => members.iter().all(|&member| {
                    let member_kind = tsz_solver::type_queries::classify_index_key(db, member);
                    key_matches_string_index(db, member, &member_kind)
                }),
                IndexKeyKind::Other => {
                    tsz_solver::type_queries::contains_type_parameters_db(db, key_type)
                        || tsz_solver::type_queries::is_keyof_type(db, key_type)
                }
            }
        }

        fn key_matches_number_index(
            db: &dyn tsz_solver::TypeDatabase,
            key_type: TypeId,
            kind: &IndexKeyKind,
        ) -> bool {
            match kind {
                IndexKeyKind::Number
                | IndexKeyKind::NumberLiteral
                | IndexKeyKind::NumericStringLike => true,
                IndexKeyKind::Union(members) => members.iter().all(|&member| {
                    let member_kind = tsz_solver::type_queries::classify_index_key(db, member);
                    key_matches_number_index(db, member, &member_kind)
                }),
                IndexKeyKind::Other => {
                    tsz_solver::type_queries::contains_type_parameters_db(db, key_type)
                        || tsz_solver::type_queries::is_keyof_type(db, key_type)
                }
                _ => false,
            }
        }

        let db = self.ctx.types.as_type_database();
        let object_type = self.evaluate_type_for_assignability(object_type);
        let key_type = self.evaluate_type_for_assignability(index_type);
        let key_kind = tsz_solver::type_queries::classify_index_key(db, key_type);

        if let Some(shape) = common_query::object_shape_for_type(db, object_type) {
            if let Some(index) = &shape.string_index
                && key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        if let Some(shape) = common_query::callable_shape_for_type(db, object_type) {
            if let Some(index) = &shape.string_index
                && key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        None
    }

    /// Check if a type represents the global `Function` interface from lib.d.ts.
    ///
    /// Checks via Lazy(DefId) against the interner's registered boxed `DefIds`,
    /// or by direct TypeId match against the interner's registered boxed type.
    fn is_function_constraint(&self, type_id: TypeId) -> bool {
        use tsz_solver::visitor::lazy_def_id;
        let db = self.ctx.types.as_type_database();
        // Direct match against interner's boxed Function TypeId
        if let Some(boxed_id) = db.get_boxed_type(tsz_solver::IntrinsicKind::Function)
            && type_id == boxed_id
        {
            return true;
        }
        // Check if the type is Lazy(DefId) with a known Function boxed DefId
        if let Some(def_id) = lazy_def_id(db, type_id)
            && db.is_boxed_def_id(def_id, tsz_solver::IntrinsicKind::Function)
        {
            return true;
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
        use tsz_solver::visitor::{application_id, lazy_def_id};
        let db = self.ctx.types.as_type_database();

        // Get the Application base DefId from the constraint.
        // e.g., for AA<BB>, get the DefId of AA.
        let constraint_base_def = application_id(db, constraint).and_then(|app_id| {
            let app = db.type_application(app_id);
            lazy_def_id(db, app.base)
        });
        let Some(constraint_base_def) = constraint_base_def else {
            return false;
        };

        // Get the type_arg's DefId (it must be an interface/class, i.e., Lazy type).
        let type_arg_def = lazy_def_id(db, type_arg);
        let Some(type_arg_def) = type_arg_def else {
            // When the type_arg is an Application (e.g., AA<BB>) with the same base
            // as the constraint (e.g., AA<AA<BB>>), AND the inner type arguments of
            // the Application type_arg extend the constraint base via heritage, the
            // constraint is coinductively satisfied. This handles recursive constraints
            // like `T extends AA<T>` where `interface BB extends AA<AA<BB>>` — checking
            // `AA<BB>` against `AA<AA<BB>>` leads to infinite nesting that tsc resolves
            // via deeply-nested type detection.
            let type_arg_app_info = application_id(db, type_arg).map(|app_id| {
                let app = db.type_application(app_id);
                (lazy_def_id(db, app.base), app.args.clone())
            });
            if let Some((Some(type_arg_base_def), ref type_arg_args)) = type_arg_app_info {
                if type_arg_base_def == constraint_base_def {
                    // Same base type (e.g., both are AA<...>).
                    // Check if any inner type argument extends the constraint base,
                    // which would create the circular recursion pattern.
                    for &inner_arg in type_arg_args.iter() {
                        let inner_def = lazy_def_id(db, inner_arg);
                        if let Some(inner_def) = inner_def {
                            let inner_sym = self.ctx.def_to_symbol_id(inner_def);
                            let constraint_sym = self.ctx.def_to_symbol_id(constraint_base_def);
                            if let (Some(inner_sym_id), Some(constraint_sym_id)) =
                                (inner_sym, constraint_sym)
                            {
                                if self.interface_extends_symbol(inner_sym_id, constraint_sym_id) {
                                    return true;
                                }
                            }
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
        use tsz_solver::type_queries::{self as query, ArrayLikeKind};

        let Some(shape) = query::get_object_shape(self.ctx.types, source) else {
            return false;
        };
        if shape.number_index.is_none() {
            return false;
        }

        for name in ["length", "concat", "slice"] {
            if query::find_property_in_object_by_str(self.ctx.types, source, name).is_none() {
                return false;
            }
        }

        if !matches!(
            query::classify_array_like(self.ctx.types, target),
            ArrayLikeKind::Readonly(_)
        ) && query::find_property_in_object_by_str(self.ctx.types, source, "push").is_none()
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
