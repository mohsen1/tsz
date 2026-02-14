//! Generic Type Argument Checking Module
//!
//! This module contains methods for validating generic type arguments.
//! It handles:
//! - Type argument constraint validation (TS2344)
//! - Call expression type argument validation
//! - New expression type argument validation
//!
//! This module extends CheckerState with generic-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::query_boundaries::generic_checker as query;
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

        let got = type_args_list.nodes.len();
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
                    let matching = shape
                        .call_signatures
                        .iter()
                        .find(|sig| sig.type_params.len() == got)
                        .map(|sig| sig.type_params.clone());
                    if let Some(params) = matching {
                        params
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

        let expected = type_params.len();

        if type_params.is_empty() {
            // TS2558: Expected 0 type arguments, but got N.
            if got > 0 {
                self.error_at_node_msg(
                    call_idx,
                    crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                    &["0", &got.to_string()],
                );
            }
            return;
        }

        if got != expected {
            // TS2558: Expected N type arguments, but got M.
            self.error_at_node_msg(
                call_idx,
                crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
                &[&expected.to_string(), &got.to_string()],
            );
            return;
        }

        // Collect the provided type arguments
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Skip constraint checking when the type argument contains unresolved type parameters
                // (they'll be checked later when fully instantiated)
                if query::contains_type_parameters(self.ctx.types, type_arg) {
                    continue;
                }

                // Resolve the constraint in case it's a Lazy type
                let constraint = self.resolve_lazy_type(constraint);

                // Instantiate the constraint with type arguments up to and including the
                // current parameter so self-referential constraints are validated.
                let mut subst = tsz_solver::TypeSubstitution::new();
                for (j, p) in type_params.iter().take(i + 1).enumerate() {
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

                let is_satisfied = self.is_assignable_to(type_arg, instantiated_constraint);

                if !is_satisfied {
                    // Report TS2344 at the specific type argument node
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
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
            if !has_type_params_in_decl && let Some(&arg_idx) = type_args_list.nodes.first() {
                let lib_binders = self.get_lib_binders();
                let name = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map(|s| s.escaped_name.clone())
                    .unwrap_or_else(|| "<unknown>".to_string());
                self.error_at_node_msg(
                    arg_idx,
                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                    &[name.as_str()],
                );
            }
            return;
        }

        // Collect the provided type arguments
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Skip validation when type arguments contain unresolved type parameters
                // or infer types. TypeScript defers constraint checking when args aren't
                // fully concrete (e.g., indexed access `T[K]`, conditional types, etc.)
                if query::contains_type_parameters(self.ctx.types, type_arg) {
                    continue;
                }

                // Resolve the constraint in case it's a Lazy type
                let constraint = self.resolve_lazy_type(constraint);

                // Instantiate the constraint with type arguments up to and including the
                // current parameter so self-referential constraints are validated.
                let mut subst = tsz_solver::TypeSubstitution::new();
                for (j, p) in type_params.iter().take(i + 1).enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        subst.insert(p.name, arg);
                    }
                }
                let instantiated_constraint = if subst.is_empty() {
                    constraint
                } else {
                    tsz_solver::instantiate_type(self.ctx.types, constraint, &subst)
                };

                // Also skip if the instantiated constraint contains type parameters.
                // This can happen when the constraint references other type params
                // that weren't fully substituted (e.g., `K extends keyof T` where T
                // is itself a type parameter from a different context).
                if query::contains_type_parameters(self.ctx.types, instantiated_constraint) {
                    continue;
                }

                let is_satisfied = self.is_assignable_to(type_arg, instantiated_constraint);

                if !is_satisfied {
                    // Report TS2344 at the specific type argument node
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
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

    /// Validate explicit type arguments against their constraints for new expressions.
    /// Reports TS2344 when a type argument doesn't satisfy its constraint.
    pub(crate) fn validate_new_expression_type_arguments(
        &mut self,
        constructor_type: TypeId,
        type_args_list: &tsz_parser::parser::NodeList,
        _call_idx: NodeIndex,
    ) {
        // Get the type parameters from the constructor type
        let Some(shape) = query::callable_shape_for_type(self.ctx.types, constructor_type) else {
            return;
        };
        // For callable types, use the first construct signature's type params
        let type_params = shape
            .construct_signatures
            .first()
            .map(|sig| sig.type_params.clone())
            .unwrap_or_default();

        if type_params.is_empty() {
            return;
        }

        // Collect the provided type arguments
        let type_args: Vec<TypeId> = type_args_list
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
            if let Some(constraint) = param.constraint {
                // Skip constraint checking when the type argument contains unresolved type parameters
                // (they'll be checked later when fully instantiated)
                if query::contains_type_parameters(self.ctx.types, type_arg) {
                    continue;
                }

                // Resolve the constraint in case it's a Lazy type
                let constraint = self.resolve_lazy_type(constraint);

                // Instantiate the constraint with type arguments up to and including the
                // current parameter so self-referential constraints are validated.
                let mut subst = tsz_solver::TypeSubstitution::new();
                for (j, p) in type_params.iter().take(i + 1).enumerate() {
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

                let is_satisfied = self.is_assignable_to(type_arg, instantiated_constraint);

                if !is_satisfied {
                    // Report TS2344 at the specific type argument node
                    if let Some(&arg_idx) = type_args_list.nodes.get(i) {
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

    /// Check if a symbol's declaration has type parameters, even if they couldn't be
    /// resolved via get_type_params_for_symbol (e.g., cross-arena lib types).
    fn symbol_declaration_has_type_parameters(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };

        // Check the value declaration and all declarations for type parameters
        let decl_indices: Vec<_> = if !symbol.value_declaration.is_none() {
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
            if let Some(decl_arena) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
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
