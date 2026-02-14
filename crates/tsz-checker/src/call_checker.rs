//! Call Checking Module
//!
//! This module contains methods for checking function and method calls.
//! It handles:
//! - Argument type collection with contextual typing
//! - Overload resolution
//! - Type argument validation (TS2344)
//! - Call signature instantiation
//! - This-type substitution in call returns
//!
//! This module extends CheckerState with call-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::query_boundaries::call_checker::{
    array_element_type_for_type, is_type_parameter_type, lazy_def_id_for_type,
    resolve_call_with_context, tuple_elements_for_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

// =============================================================================
// Call Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Argument Type Collection
    // =========================================================================

    /// Collect argument types with contextual typing from expected parameter types.
    ///
    /// This method handles:
    /// - Regular arguments: applies contextual type from parameter
    /// - Spread arguments: expands tuple types to multiple arguments
    /// - Excess property checking for object literal arguments
    /// - Skipping sensitive arguments in Round 1 of two-pass inference
    ///
    /// # Parameters
    /// - `args`: The argument node indices
    /// - `expected_for_index`: Closure that returns the expected type for a given argument index
    /// - `check_excess_properties`: Whether to check for excess properties on object literals
    /// - `skip_sensitive_indices`: Optional mask indicating which arguments to skip (for Round 1)
    ///
    /// # Returns
    /// Vector of resolved argument types
    pub(crate) fn collect_call_argument_types_with_context<F>(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: F,
        check_excess_properties: bool,
        skip_sensitive_indices: Option<&[bool]>,
    ) -> Vec<TypeId>
    where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        use tsz_solver::FunctionShape;
        let factory = self.ctx.types.factory();

        // Pre-create a single placeholder for skipped sensitive arguments.
        // The solver's is_contextually_sensitive recognizes Function types and skips them
        // during Round 1 inference. We create one and reuse its TypeId for all skipped args.
        let sensitive_placeholder = skip_sensitive_indices.map(|_| {
            let shape = FunctionShape {
                params: vec![],
                return_type: TypeId::ANY,
                this_type: None,
                type_params: vec![],
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            };
            factory.function(shape)
        });

        // First pass: count expanded arguments (spreads of tuple/array literals expand to multiple args)
        let mut expanded_count = 0usize;
        for &arg_idx in args.iter() {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
            {
                let spread_type = self.get_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                    expanded_count += elems.len();
                    continue;
                }
                // Check if it's an array literal spread
                if array_element_type_for_type(self.ctx.types, spread_type).is_some() {
                    if let Some(expr_node) = self.ctx.arena.get(spread_data.expression) {
                        if let Some(literal) = self.ctx.arena.get_literal_expr(expr_node) {
                            expanded_count += literal.elements.nodes.len();
                            continue;
                        }
                    }
                }
            }
            expanded_count += 1;
        }

        let mut arg_types = Vec::with_capacity(expanded_count);
        let mut effective_index = 0usize;

        for (i, &arg_idx) in args.iter().enumerate() {
            // Skip sensitive arguments in Round 1 of two-pass generic inference.
            // Push a Function-typed placeholder so the solver's is_contextually_sensitive
            // recognizes it and skips inference for this slot.
            if let Some(skip_mask) = skip_sensitive_indices {
                if i < skip_mask.len() && skip_mask[i] {
                    arg_types.push(sensitive_placeholder.unwrap());
                    effective_index += 1;
                    continue;
                }
            }

            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                // Handle spread elements specially - expand tuple types
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
                {
                    let spread_type = self.get_type_of_node(spread_data.expression);
                    let spread_type = self.resolve_type_for_property_access(spread_type);
                    let spread_type = self.resolve_lazy_type(spread_type);

                    // Check if spread argument is iterable, emit TS2488 if not
                    self.check_spread_iterability(spread_type, spread_data.expression);

                    // If it's a tuple type, expand its elements
                    if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                        for elem in elems.iter() {
                            arg_types.push(elem.type_id);
                            effective_index += 1;
                        }
                        continue;
                    }

                    // If it's an array type, check if it's an array literal spread
                    // For array literals, we want to check each element individually
                    // For non-literal arrays, treat as variadic (check element type against remaining params)
                    if array_element_type_for_type(self.ctx.types, spread_type).is_some() {
                        // Check if the spread expression is an array literal
                        if let Some(expr_node) = self.ctx.arena.get(spread_data.expression) {
                            if let Some(literal) = self.ctx.arena.get_literal_expr(expr_node) {
                                // It's an array literal - get each element's type individually
                                for &elem_idx in literal.elements.nodes.iter() {
                                    if elem_idx.is_none() {
                                        continue;
                                    }
                                    // Skip spread elements within the spread (unlikely but handle it)
                                    if let Some(elem_node) = self.ctx.arena.get(elem_idx) {
                                        if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                                            // For nested spreads in array literals, use the element type
                                            if let Some(elem_type) = array_element_type_for_type(
                                                self.ctx.types,
                                                spread_type,
                                            ) {
                                                arg_types.push(elem_type);
                                                effective_index += 1;
                                            }
                                            continue;
                                        }
                                    }
                                    // Get the type of this specific element
                                    let elem_type = self.get_type_of_node(elem_idx);
                                    arg_types.push(elem_type);
                                    effective_index += 1;
                                }
                                continue;
                            }
                        }

                        // Not an array literal - treat as variadic (element type applies to all remaining params)
                        // But first, emit TS2556 error: spread must be tuple or rest parameter
                        // Only emit when the target function does NOT have a rest parameter.
                        //
                        // NOTE: We can't check is_array_like on the expected type because
                        // extract_param_type_at unwraps rest parameter arrays, returning
                        // the element type (e.g. `string` for `...z: string[]`). Instead,
                        // we probe at a very large index: rest parameters accept unlimited
                        // args, so a probe returns Some only when a rest param exists.
                        if array_element_type_for_type(self.ctx.types, spread_type).is_some() {
                            let current_expected =
                                expected_for_index(effective_index, expanded_count);
                            // Determine if the target accepts this spread:
                            // 1. No expected type → unresolved, don't emit error
                            // 2. Expected type is `any` → accepts all spreads
                            // 3. Probe at large index returns Some → rest param exists
                            let target_accepts_spread = current_expected.is_none()
                                || current_expected.is_some_and(|t| t == TypeId::ANY)
                                || expected_for_index(usize::MAX / 2, expanded_count).is_some();
                            if !target_accepts_spread {
                                // This is a spread of a non-tuple array type
                                // TypeScript emits TS2556: "A spread argument must either have a tuple type or be passed to a rest parameter."
                                self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                            }
                            // Continue processing - push the element type for assignability checking
                            if let Some(elem_type) =
                                array_element_type_for_type(self.ctx.types, spread_type)
                            {
                                arg_types.push(elem_type);
                                effective_index += 1;
                                continue;
                            }
                        }
                    }

                    // Otherwise just push the spread type as-is
                    arg_types.push(spread_type);
                    effective_index += 1;
                    continue;
                }
            }

            // Regular (non-spread) argument
            let expected_type = expected_for_index(effective_index, expanded_count);

            let prev_context = self.ctx.contextual_type;
            self.ctx.contextual_type = expected_type;

            let arg_type = self.get_type_of_node(arg_idx);
            arg_types.push(arg_type);

            if check_excess_properties
                && let Some(expected) = expected_type
                && expected != TypeId::ANY
                && expected != TypeId::UNKNOWN
                && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                // Skip excess property checking for type parameters - the type parameter
                // captures the full object type, so extra properties are allowed.
                && !is_type_parameter_type(self.ctx.types, expected)
            {
                self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
            }

            self.ctx.contextual_type = prev_context;
            effective_index += 1;
        }

        arg_types
    }

    /// Check excess properties on call arguments that are object literals.
    fn check_call_argument_excess_properties<F>(
        &mut self,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        mut expected_for_index: F,
    ) where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        let arg_count = args.len();
        for (i, &arg_idx) in args.iter().enumerate() {
            let expected = expected_for_index(i, arg_count);
            if let Some(expected) = expected
                && expected != TypeId::ANY
                && expected != TypeId::UNKNOWN
                && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                let arg_type = arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN);
                self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
            }
        }
    }

    // =========================================================================
    // Overload Resolution
    // =========================================================================

    /// Resolve an overloaded call by trying each signature.
    ///
    /// This method iterates through overload signatures and returns the first
    /// one that successfully matches the provided arguments.
    ///
    /// # Parameters
    /// - `args`: The argument node indices
    /// - `signatures`: The overload signatures to try
    ///
    /// # Returns
    /// - `Some(return_type)` if a matching overload was found
    /// - `None` if no overload matched
    pub(crate) fn resolve_overloaded_call_with_signatures(
        &mut self,
        args: &[NodeIndex],
        signatures: &[tsz_solver::CallSignature],
        force_bivariant_callbacks: bool,
    ) -> Option<TypeId> {
        use tsz_solver::{CallResult, FunctionShape};

        if signatures.is_empty() {
            return None;
        }

        // Phase 6 Task 4: Overload contextual typing baseline.
        // First pass collects argument types once using a union of overload signatures.
        // If that fails to find a match, we run a second pass that re-collects arguments
        // per candidate signature with signature-specific contextual types. This helps
        // avoid false TS2345/TS2322 when the union contextual type is too lossy.

        // Create a union of all overload signatures for contextual typing
        let signature_types: Vec<TypeId> = signatures
            .iter()
            .map(|sig| {
                let func_shape = FunctionShape {
                    params: sig.params.clone(),
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_params: sig.type_params.clone(),
                    type_predicate: sig.type_predicate.clone(),
                    is_constructor: false,
                    is_method: sig.is_method,
                };
                factory.function(func_shape)
            })
            .collect();

        // Union of all signatures provides contextual typing
        let union_contextual = if signature_types.len() == 1 {
            signature_types[0]
        } else {
            factory.union(signature_types)
        };

        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            union_contextual,
            self.ctx.compiler_options.no_implicit_any,
        );

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        // Collect argument types ONCE with union contextual type.
        // Diagnostics produced during this pass are speculative: if no overload
        // matches, TypeScript reports the overload failure and suppresses these
        // nested callback/body diagnostics.
        let first_pass_diagnostics_checkpoint = self.ctx.diagnostics.len();
        self.ctx.node_types = Default::default();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            false,
            None, // No skipping needed for overload resolution
        );
        let temp_node_types = std::mem::take(&mut self.ctx.node_types);

        self.ctx.node_types = std::mem::take(&mut original_node_types);

        // First pass: try each signature with union-contextual argument types.
        for (idx, (_sig, &func_type)) in signatures.iter().zip(signature_types.iter()).enumerate() {
            tracing::debug!("Trying overload {} with {} args", idx, arg_types.len());
            self.ensure_relation_input_ready(func_type);
            self.ensure_relation_inputs_ready(&arg_types);
            let result = {
                let env = self.ctx.type_env.borrow();
                // Resolve Lazy func_type via type_env before passing to solver.
                // The solver's resolve_call doesn't handle Lazy types, so we must
                // resolve to the concrete Function/Callable type here.
                let resolved_func_type = {
                    if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                        env.get_def(def_id).unwrap_or(func_type)
                    } else {
                        func_type
                    }
                };
                resolve_call_with_context(
                    self.ctx.types,
                    &self.ctx,
                    &*env,
                    resolved_func_type,
                    &arg_types,
                    force_bivariant_callbacks,
                    self.ctx.contextual_type,
                )
            };

            match &result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                } => {
                    tracing::debug!("Overload {} failed: arg {} type mismatch", idx, index);
                    tracing::debug!("  Expected TypeId: {:?}", expected);
                    tracing::debug!("  Actual TypeId: {:?}", actual);
                }
                _ => {
                    tracing::debug!("Overload {} result: {:?}", idx, result);
                }
            }
            match result {
                CallResult::Success(return_type) => {
                    // Phase 6 Task 4: Merge the node types inferred during argument collection
                    self.ctx.node_types.extend(temp_node_types);

                    // Phase 6 Task 4: CRITICAL FIX - Check excess properties against the MATCHED signature,
                    // not the union. Using the union would allow properties that exist in other overloads
                    // but not in the selected one, causing false negatives.
                    let matched_sig_helper = ContextualTypeContext::with_expected_and_options(
                        self.ctx.types,
                        func_type,
                        self.ctx.compiler_options.no_implicit_any,
                    );
                    self.check_call_argument_excess_properties(args, &arg_types, |i, arg_count| {
                        matched_sig_helper.get_parameter_type_for_call(i, arg_count)
                    });

                    return Some(return_type);
                }
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // Constraint violation from callback return - overload matched
                    // but with constraint error. Treat as match for overload resolution.
                    self.ctx.node_types.extend(temp_node_types);
                    return Some(return_type);
                }
                _ => {}
            }
        }

        // Second pass: signature-specific contextual typing.
        // Some overload sets require contextual typing from a specific candidate to
        // type callback/object-literal arguments correctly. The union pass above can
        // miss those, producing false negatives and downstream false TS2345/TS2322.
        for (_sig, &func_type) in signatures.iter().zip(signature_types.iter()) {
            let sig_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                func_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            let diagnostics_checkpoint = self.ctx.diagnostics.len();
            self.ctx.node_types = Default::default();

            let sig_arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                false,
                None,
            );

            self.ensure_relation_input_ready(func_type);
            self.ensure_relation_inputs_ready(&sig_arg_types);

            let result = {
                let env = self.ctx.type_env.borrow();
                let resolved_func_type = {
                    if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                        env.get_def(def_id).unwrap_or(func_type)
                    } else {
                        func_type
                    }
                };
                resolve_call_with_context(
                    self.ctx.types,
                    &self.ctx,
                    &*env,
                    resolved_func_type,
                    &sig_arg_types,
                    force_bivariant_callbacks,
                    self.ctx.contextual_type,
                )
            };

            if let CallResult::Success(return_type) = result {
                let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                self.ctx.node_types = std::mem::take(&mut original_node_types);
                self.ctx.node_types.extend(sig_node_types);

                self.check_call_argument_excess_properties(args, &sig_arg_types, |i, arg_count| {
                    sig_helper.get_parameter_type_for_call(i, arg_count)
                });

                return Some(return_type);
            }

            self.ctx.diagnostics.truncate(diagnostics_checkpoint);
        }

        // No overload matched: drop speculative diagnostics from overload argument
        // collection and keep only overload-level diagnostics.
        self.ctx
            .diagnostics
            .truncate(first_pass_diagnostics_checkpoint);

        // Restore original state if no overload matched
        self.ctx.node_types = original_node_types;
        None
    }
}
