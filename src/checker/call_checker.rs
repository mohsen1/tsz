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

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::{ContextualTypeContext, TypeId};

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
    ///
    /// # Parameters
    /// - `args`: The argument node indices
    /// - `expected_for_index`: Closure that returns the expected type for a given argument index
    /// - `check_excess_properties`: Whether to check for excess properties on object literals
    ///
    /// # Returns
    /// Vector of resolved argument types
    pub(crate) fn collect_call_argument_types_with_context<F>(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: F,
        check_excess_properties: bool,
    ) -> Vec<TypeId>
    where
        F: FnMut(usize, usize) -> Option<TypeId>,
    {
        use crate::solver::type_queries::{get_array_element_type, get_tuple_elements};

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
                if let Some(elems) = get_tuple_elements(self.ctx.types, spread_type) {
                    expanded_count += elems.len();
                    continue;
                }
                // Check if it's an array literal spread
                if get_array_element_type(self.ctx.types, spread_type).is_some() {
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

        for &arg_idx in args.iter() {
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
                    if let Some(elems) = get_tuple_elements(self.ctx.types, spread_type) {
                        for elem in elems.iter() {
                            arg_types.push(elem.type_id);
                            effective_index += 1;
                        }
                        continue;
                    }

                    // If it's an array type, check if it's an array literal spread
                    // For array literals, we want to check each element individually
                    // For non-literal arrays, treat as variadic (check element type against remaining params)
                    if get_array_element_type(self.ctx.types, spread_type).is_some() {
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
                                            if let Some(elem_type) =
                                                get_array_element_type(self.ctx.types, spread_type)
                                            {
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
                        if get_array_element_type(self.ctx.types, spread_type).is_some() {
                            // This is a spread of a non-tuple array type
                            // TypeScript emits TS2556: "A spread argument must either have a tuple type or be passed to a rest parameter."
                            // We'll emit the error here on the spread expression
                            self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                            // Continue processing - push the element type for assignability checking
                            if let Some(elem_type) =
                                get_array_element_type(self.ctx.types, spread_type)
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
        signatures: &[crate::solver::CallSignature],
        force_bivariant_callbacks: bool,
    ) -> Option<TypeId> {
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, FunctionShape};

        if signatures.is_empty() {
            return None;
        }

        // Phase 6 Task 4: Overload Contextual Typing
        // Instead of re-collecting argument types for each signature (incorrect),
        // we collect argument types ONCE using a union of all overload signatures.
        // This matches TypeScript behavior where arguments get the union of parameter types
        // from all candidate signatures as their contextual type.

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
                self.ctx.types.function(func_shape)
            })
            .collect();

        // Union of all signatures provides contextual typing
        let union_contextual = if signature_types.len() == 1 {
            signature_types[0]
        } else {
            self.ctx.types.union(signature_types.clone())
        };

        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, union_contextual);

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        // Collect argument types ONCE with union contextual type
        self.ctx.node_types = Default::default();
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            false,
        );
        let temp_node_types = std::mem::take(&mut self.ctx.node_types);

        self.ctx.node_types = std::mem::take(&mut original_node_types);

        // Now try each signature with the pre-collected argument types
        for (_sig, &func_type) in signatures.iter().zip(signature_types.iter()) {
            self.ensure_application_symbols_resolved(func_type);
            for &arg_type in &arg_types {
                self.ensure_application_symbols_resolved(arg_type);
            }

            // Ensure all Ref types are resolved into type_env for assignability.
            self.ensure_refs_resolved(func_type);
            for &arg_type in &arg_types {
                self.ensure_refs_resolved(arg_type);
            }
            let result = {
                let env = self.ctx.type_env.borrow();
                // Resolve Lazy func_type via type_env before passing to solver.
                // The solver's resolve_call doesn't handle Lazy types, so we must
                // resolve to the concrete Function/Callable type here.
                let resolved_func_type = {
                    use crate::solver::TypeKey;
                    if let Some(TypeKey::Lazy(def_id)) = self.ctx.types.lookup(func_type) {
                        env.get_def(def_id).unwrap_or(func_type)
                    } else {
                        func_type
                    }
                };
                let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
                self.ctx.configure_compat_checker(&mut checker);
                let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
                evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
                // Pass contextual type for generic type inference
                // Example: `let x: string = id(42)` should infer T = string from context
                evaluator.set_contextual_type(self.ctx.contextual_type);
                evaluator.resolve_call(resolved_func_type, &arg_types)
            };

            if let CallResult::Success(return_type) = result {
                // Phase 6 Task 4: Merge the node types inferred during argument collection
                self.ctx.node_types.extend(temp_node_types);

                // Phase 6 Task 4: CRITICAL FIX - Check excess properties against the MATCHED signature,
                // not the union. Using the union would allow properties that exist in other overloads
                // but not in the selected one, causing false negatives.
                let matched_sig_helper =
                    ContextualTypeContext::with_expected(self.ctx.types, func_type);
                self.check_call_argument_excess_properties(args, &arg_types, |i, arg_count| {
                    matched_sig_helper.get_parameter_type_for_call(i, arg_count)
                });

                return Some(return_type);
            }

            // Phase 6 Task 4: REMOVED erroneous std::mem::take line that was corrupting state.
            // We don't need to save state on failure - just continue to the next signature.
        }

        // Restore original state if no overload matched
        self.ctx.node_types = original_node_types;
        None
    }
}
