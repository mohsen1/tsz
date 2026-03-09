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
    // =========================================================================
    // Type Argument Validation
    // =========================================================================

    /// Validate explicit type arguments against their constraints for call expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    /// Reports TS2558 when a non-generic function is called with type arguments.
    pub(crate) fn validate_call_type_arguments(
        &mut self,
        callee_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        call_idx: NodeIndex,
    ) {
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
            return;
        }

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
        // Get the type parameters from the callee type. For callables with overloads,
        // prefer a signature whose type parameter arity matches the provided type args.
        let type_params =
            match query::classify_for_type_argument_extraction(self.ctx.types, callee_type) {
                query::TypeArgumentExtractionKind::Function(shape_id) => {
                    let shape = self.ctx.types.function_shape(shape_id);
                    shape.type_params.clone()
                }
                query::TypeArgumentExtractionKind::Callable(shape_id) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    let matching: Vec<_> = shape
                        .call_signatures
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
                    // TSC only emits TS2344 when NO overload's constraints are satisfied;
                    // the overload resolution loop handles per-signature constraint checking.
                    if matching.len() > 1 {
                        return;
                    }
                    if let Some(sig) = matching.first() {
                        sig.type_params.clone()
                    } else {
                        // Fall back to first signature for diagnostics when no arity match exists.
                        shape
                            .call_signatures
                            .first()
                            .map(|sig| sig.type_params.clone())
                            .unwrap_or_default()
                    }
                }
                query::TypeArgumentExtractionKind::Other => return,
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
            }
            return;
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
            return;
        }

        self.validate_type_args_against_params(&type_params, type_args_list);
    }

    /// Validate type arguments against their constraints for type references (e.g., `A<X, Y>`).
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    ///
    /// This handles cases like `class A<T, U extends T>` where `A<{a: string}, {b: string}>`
    /// should error because `{b: string}` doesn't extend `{a: string}`.
    pub(crate) fn validate_type_reference_type_arguments(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        type_args_list: &tsz_parser::parser::NodeList,
        type_ref_idx: NodeIndex,
    ) {
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
            return;
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
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Skip constraint checking when the type argument is an error type
                // (avoids cascading errors from unresolved references)
                if type_arg == TypeId::ERROR {
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
                if type_arg_contains_type_parameters {
                    let db = self.ctx.types.as_type_database();
                    let base = tsz_solver::type_queries::get_base_constraint_of_type(db, type_arg);
                    if base != type_arg {
                        // Bare type parameter — check its base constraint instead of
                        // eagerly validating the unresolved type parameter itself.
                        // Composite generic arguments like `T[K]` or `GetProps<C>`
                        // must still flow through the full relation check below; tsc
                        // reports TS2344 for those when the generic structure itself
                        // fails the target constraint.
                        let base = self.resolve_lazy_type(base);
                        // If the base constraint is `unknown`, the type parameter has no
                        // usable constraint (e.g., unconstrained params or call-signature
                        // type params whose constraints aren't populated). Skip.
                        if base == TypeId::UNKNOWN {
                            continue;
                        }
                        // If the base constraint is a union, skip. Union-constrained type
                        // params often appear in conditional types where the true branch
                        // narrows to a specific union member. Checking the full union
                        // against the narrowed constraint would produce false positives.
                        if tsz_solver::type_queries::get_union_members(db, base).is_some() {
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
                        let mut subst = tsz_solver::TypeSubstitution::new();
                        for (j, p) in type_params.iter().enumerate() {
                            if let Some(&arg) = type_args.get(j) {
                                subst.insert(p.name, arg);
                            }
                        }
                        let inst_constraint = if subst.is_empty() {
                            constraint_resolved
                        } else {
                            tsz_solver::instantiate_type(
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

                // Instantiate the constraint with all provided type arguments so that
                // forward-referencing constraints (e.g., `T extends U` where U comes
                // after T) are fully resolved before validation.
                let mut subst = tsz_solver::TypeSubstitution::new();
                for (j, p) in type_params.iter().enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        subst.insert(p.name, arg);
                    }
                }
                let instantiated_constraint = if subst.is_empty() {
                    constraint
                } else {
                    tsz_solver::instantiate_type(self.ctx.types, constraint, &subst)
                };

                let mut is_satisfied = self.is_assignable_to(type_arg, instantiated_constraint);

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

                if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    // Check if the failure is due to a weak type violation (TS2559).
                    // In tsc, when the constraint is a "weak type" (all-optional properties)
                    // and the type argument shares no common properties, tsc emits TS2559
                    // instead of TS2344.
                    let analysis =
                        self.analyze_assignability_failure(type_arg, instantiated_constraint);
                    if matches!(
                        analysis.failure_reason,
                        Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                    ) {
                        self.error_no_common_properties_constraint(
                            type_arg,
                            instantiated_constraint,
                            arg_idx,
                        );
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

        if !self.is_array_like_type(source)
            && !self.has_structural_array_surface(source, target)
        {
            return false;
        }

        if target_elem == TypeId::ANY {
            return true;
        }

        let source_elem = self.get_element_access_type(source, TypeId::NUMBER, Some(0));
        source_elem != TypeId::ERROR && self.is_assignable_to(source_elem, target_elem)
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
