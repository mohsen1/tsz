//! Generic type argument validation (TS2344 constraint checking).

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
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
            self.error_at_node(
                call_idx,
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
                && let Some(&arg_idx) = type_args_list.nodes.first()
            {
                let lib_binders = self.get_lib_binders();
                let name = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
                self.error_at_node_msg(
                    arg_idx,
                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                    &[name.as_str()],
                );
            }
            return;
        }

        // Collect the provided type arguments for circular reference check
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        let symbol_type = self.get_type_of_symbol(sym_id);

        if type_args.contains(&symbol_type) {
            // Report TS4109 at the specific type argument node
            if let Some(&arg_idx) = type_args_list.nodes.first() {
                let lib_binders = self.get_lib_binders();
                let name = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());

                self.error_at_node_msg(
                    arg_idx,
                    crate::diagnostics::diagnostic_codes::TYPE_ARGUMENTS_FOR_CIRCULARLY_REFERENCE_THEMSELVES,
                    &[name.as_str()],
                );
            }
        }

        // Validate type arguments against their constraints
        self.validate_type_args_against_params(&type_params, type_args_list);
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

                // Skip constraint checking when the type argument contains unresolved type parameters
                // (they'll be checked later when fully instantiated)
                if query::contains_type_parameters(self.ctx.types, type_arg) {
                    continue;
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

                // Skip if the instantiated constraint contains type parameters
                if query::contains_type_parameters(self.ctx.types, instantiated_constraint) {
                    continue;
                }

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

                if !is_satisfied && let Some(&arg_idx) = type_args_list.nodes.get(i) {
                    self.error_type_constraint_not_satisfied(
                        type_arg,
                        instantiated_constraint,
                        arg_idx,
                    );
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
