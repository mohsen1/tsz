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

        // First pass: count expanded arguments (spreads of tuple types expand to multiple args)
        let mut expanded_count = 0usize;
        for &arg_idx in args.iter() {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
            {
                let spread_type = self.get_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                if let Some(elems) = get_tuple_elements(self.ctx.types, spread_type) {
                    expanded_count += elems.len();
                    continue;
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

                    // If it's an array type, push the element type (variadic handling)
                    if let Some(elem_type) = get_array_element_type(self.ctx.types, spread_type) {
                        arg_types.push(elem_type);
                        effective_index += 1;
                        continue;
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
    ) -> Option<TypeId> {
        use crate::solver::{CallEvaluator, CallResult, CompatChecker, FunctionShape};

        if signatures.is_empty() {
            return None;
        }

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        for sig in signatures {
            let func_shape = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate.clone(),
                is_constructor: false,
                is_method: false,
            };
            let func_type = self.ctx.types.function(func_shape);
            let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, func_type);

            self.ctx.node_types = Default::default();
            let arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                false,
            );
            let temp_node_types = std::mem::take(&mut self.ctx.node_types);

            self.ctx.node_types = std::mem::take(&mut original_node_types);
            self.ensure_application_symbols_resolved(func_type);
            for &arg_type in &arg_types {
                self.ensure_application_symbols_resolved(arg_type);
            }
            let result = {
                let env = self.ctx.type_env.borrow();
                let mut checker = CompatChecker::with_resolver(self.ctx.types, &*env);
                self.ctx.configure_compat_checker(&mut checker);
                let mut evaluator = CallEvaluator::new(self.ctx.types, &mut checker);
                evaluator.resolve_call(func_type, &arg_types)
            };

            if let CallResult::Success(return_type) = result {
                self.ctx.node_types.extend(temp_node_types);
                self.check_call_argument_excess_properties(args, &arg_types, |i, arg_count| {
                    ctx_helper.get_parameter_type_for_call(i, arg_count)
                });
                return Some(return_type);
            }

            original_node_types = std::mem::take(&mut self.ctx.node_types);
        }

        self.ctx.node_types = original_node_types;
        None
    }
}
