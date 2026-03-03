//! Call expression checking (overload resolution, contextual typing, signature instantiation).

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, is_type_parameter_type, lazy_def_id_for_type, resolve_call,
    resolve_new, tuple_elements_for_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{AssignabilityChecker, CallResult, ContextualTypeContext, TypeId};

struct CheckerCallAssignabilityAdapter<'s, 'ctx> {
    state: &'s mut CheckerState<'ctx>,
}

impl AssignabilityChecker for CheckerCallAssignabilityAdapter<'_, '_> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to(source, target)
    }
    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to_strict(source, target)
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        self.state.is_assignable_to_bivariant(source, target)
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_type_for_assignability(type_id)
    }
}

// =============================================================================
// Call Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Whether an argument node needs contextual typing from the callee signature.
    ///
    /// Literal expressions need contextual typing to preserve literal types when
    /// the expected parameter type is a literal union (e.g., `"A"` should remain
    /// `"A"` when passed to a parameter of type `"A" | "B"`).
    ///
    /// Other expressions like arrow functions, object literals, etc. also need
    /// contextual typing for their internal structure.
    fn argument_needs_contextual_type(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Literal expressions need contextual typing to preserve literal types
        // when the expected type is a literal union or specific literal type.
        let is_literal = matches!(
            node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        );

        if is_literal {
            return true;
        }

        matches!(
            node.kind,
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
        )
    }

    /// Const object/array literal bindings do not benefit from flow narrowing at
    /// call sites. Skipping flow narrowing for these stable identifiers avoids
    /// repeated CFG traversals on large argument objects.
    fn can_skip_flow_narrowing_for_argument(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() || !self.is_const_variable_declaration(value_decl) {
            return false;
        }

        let Some(decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return false;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
    }

    pub(crate) fn resolve_call_with_checker_adapter(
        &mut self,
        func_type: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> (
        CallResult,
        Option<(tsz_solver::TypePredicate, Vec<tsz_solver::ParamInfo>)>,
    ) {
        self.ensure_relation_input_ready(func_type);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter { state: self };
        resolve_call(
            db,
            &mut checker,
            func_type,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
        )
    }

    pub(crate) fn resolve_new_with_checker_adapter(
        &mut self,
        type_id: TypeId,
        arg_types: &[TypeId],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
    ) -> CallResult {
        self.ensure_relation_input_ready(type_id);
        self.ensure_relation_inputs_ready(arg_types);

        let db = self.ctx.types;
        let mut checker = CheckerCallAssignabilityAdapter { state: self };
        resolve_new(
            db,
            &mut checker,
            type_id,
            arg_types,
            force_bivariant_callbacks,
            contextual_type,
        )
    }

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
        // CRITICAL: The placeholder must have at least one parameter so that
        // `is_contextually_sensitive` returns `true`, which causes
        // `contextual_round1_arg_types` to skip it (return None) during Round 1
        // type inference. A zero-parameter placeholder would have
        // `is_contextually_sensitive = false`, causing it to be included in inference
        // and incorrectly constraining type parameters (e.g., `T = () => any`).
        let sensitive_placeholder = skip_sensitive_indices.map(|_| {
            let placeholder_param_name = self.ctx.types.intern_string("__sensitive_arg__");
            let shape = FunctionShape {
                params: vec![tsz_solver::ParamInfo {
                    name: Some(placeholder_param_name),
                    type_id: TypeId::ANY,
                    optional: true,
                    rest: false,
                }],
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
        for &arg_idx in args {
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
                if array_element_type_for_type(self.ctx.types, spread_type).is_some()
                    && let Some(expr_node) = self.ctx.arena.get(spread_data.expression)
                    && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                {
                    expanded_count += literal.elements.nodes.len();
                    continue;
                }
            }
            expanded_count += 1;
        }

        let mut arg_types = Vec::with_capacity(expanded_count);
        let mut effective_index = 0usize;
        // Track whether TS2556 was already emitted in this call.
        // tsc only reports TS2556 on the first non-tuple spread, not subsequent ones.
        let mut emitted_ts2556 = false;

        for (i, &arg_idx) in args.iter().enumerate() {
            // Skip sensitive arguments in Round 1 of two-pass generic inference.
            // Push a Function-typed placeholder so the solver's is_contextually_sensitive
            // recognizes it and skips inference for this slot.
            if let Some(skip_mask) = skip_sensitive_indices
                && let Some(sensitive_placeholder) = sensitive_placeholder
                && i < skip_mask.len()
                && skip_mask[i]
            {
                arg_types.push(sensitive_placeholder);
                effective_index += 1;
                continue;
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
                        for elem in &elems {
                            if elem.rest {
                                // Rest element (e.g., `...boolean[]` in `[number, string, ...boolean[]]`).
                                // Extract the array element type and push one representative
                                // argument so the solver matches it against the rest parameter's
                                // element type rather than the whole array type.
                                if let Some(inner) =
                                    array_element_type_for_type(self.ctx.types, elem.type_id)
                                {
                                    arg_types.push(inner);
                                    effective_index += 1;
                                } else if let Some(sub_elems) =
                                    tuple_elements_for_type(self.ctx.types, elem.type_id)
                                {
                                    // Rest element is a nested tuple (variadic tuple spread).
                                    // Expand its fixed elements; for nested rest elements,
                                    // extract the array element type.
                                    for sub in &sub_elems {
                                        if sub.rest {
                                            if let Some(inner) = array_element_type_for_type(
                                                self.ctx.types,
                                                sub.type_id,
                                            ) {
                                                arg_types.push(inner);
                                                effective_index += 1;
                                            }
                                        } else {
                                            let sub_type = if sub.optional {
                                                self.ctx
                                                    .types
                                                    .factory()
                                                    .union(vec![sub.type_id, TypeId::UNDEFINED])
                                            } else {
                                                sub.type_id
                                            };
                                            arg_types.push(sub_type);
                                            effective_index += 1;
                                        }
                                    }
                                }
                                // else: unknown rest type — skip (no args pushed)
                            } else {
                                let elem_type = if elem.optional {
                                    self.ctx
                                        .types
                                        .factory()
                                        .union(vec![elem.type_id, TypeId::UNDEFINED])
                                } else {
                                    elem.type_id
                                };
                                arg_types.push(elem_type);
                                effective_index += 1;
                            }
                        }
                        continue;
                    }

                    // If the spread type is a generic type parameter constrained to an
                    // array type (e.g., A extends any[]), treat it like a rest parameter
                    // spread. TypeScript does NOT emit TS2556 for such spreads because
                    // the runtime value is guaranteed to be array-like.
                    if is_type_parameter_type(self.ctx.types, spread_type)
                        && let Some(constraint) =
                            tsz_solver::type_queries::get_type_parameter_constraint(
                                self.ctx.types,
                                spread_type,
                            )
                        && (array_element_type_for_type(self.ctx.types, constraint).is_some()
                            || tuple_elements_for_type(self.ctx.types, constraint).is_some())
                    {
                        // Push the spread type as-is (the solver will handle it)
                        arg_types.push(spread_type);
                        effective_index += 1;
                        continue;
                    }

                    // If it's an array type, check if it's an array literal spread
                    // For array literals, we want to check each element individually
                    // For non-literal arrays, treat as variadic (check element type against remaining params)
                    if array_element_type_for_type(self.ctx.types, spread_type).is_some() {
                        // Check if the spread expression is an array literal
                        if let Some(expr_node) = self.ctx.arena.get(spread_data.expression)
                            && let Some(literal) = self.ctx.arena.get_literal_expr(expr_node)
                        {
                            // It's an array literal - get each element's type individually
                            for &elem_idx in &literal.elements.nodes {
                                if elem_idx.is_none() {
                                    continue;
                                }
                                // Skip spread elements within the spread (unlikely but handle it)
                                if let Some(elem_node) = self.ctx.arena.get(elem_idx)
                                    && elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                                {
                                    // For nested spreads in array literals, use the element type
                                    if let Some(elem_type) =
                                        array_element_type_for_type(self.ctx.types, spread_type)
                                    {
                                        arg_types.push(elem_type);
                                        effective_index += 1;
                                    }
                                    continue;
                                }
                                // Get the type of this specific element
                                let elem_type = self.get_type_of_node(elem_idx);
                                arg_types.push(elem_type);
                                effective_index += 1;
                            }
                            continue;
                        }

                        // Not an array literal - treat as variadic (element type applies to all remaining params)
                        // But first, emit TS2556 error: spread must be tuple or rest parameter.
                        //
                        // TS2556 fires when a non-tuple array spread covers a non-rest parameter.
                        // A spread is valid only if it lands exclusively on a rest parameter position.
                        // We check this via `is_rest_parameter_position` on the callable type,
                        // falling back to the large-index probe when the callable type isn't available.
                        if array_element_type_for_type(self.ctx.types, spread_type).is_some() {
                            let current_expected =
                                expected_for_index(effective_index, expanded_count);

                            // Check if this spread position is a rest parameter position.
                            // Use the callable type context if available for precise check;
                            // when no callable type is set (callee is any/error/unknown),
                            // fall back to the large-index probe heuristic.
                            let at_rest_position =
                                if let Some(callable_type) = self.ctx.current_callable_type {
                                    let ctx = tsz_solver::ContextualTypeContext::with_expected(
                                        self.ctx.types,
                                        callable_type,
                                    );
                                    ctx.is_rest_parameter_position(effective_index, expanded_count)
                                } else {
                                    // No callable type means callee is any/error/unknown.
                                    // Use the probe heuristic: if a large-index probe returns
                                    // Some, a rest param exists. We accept the spread if there's
                                    // no param at this position (past all non-rest params) or
                                    // if the callee is any (all positions return Some(ANY)).

                                    expected_for_index(usize::MAX / 2, expanded_count).is_some()
                                };

                            // A non-tuple array spread is only valid at a rest parameter
                            // position. Even if the param type is `any`, TS2556 fires
                            // when the spread covers a non-rest position.
                            if !at_rest_position {
                                if current_expected.is_none() {
                                    // No parameter at this position and not at rest:
                                    // the spread exceeds all declared params → TS2556.
                                    if !emitted_ts2556 {
                                        self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                                        emitted_ts2556 = true;
                                    }
                                    continue;
                                }
                                // Non-tuple array spread at a non-rest parameter → TS2556
                                if !emitted_ts2556 {
                                    self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                                    emitted_ts2556 = true;
                                }
                                // Push ANY to suppress subsequent TS2345 — tsc
                                // only reports TS2556 here.
                                arg_types.push(TypeId::ANY);
                                effective_index += 1;
                                continue;
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

                    // Handle non-array, non-tuple iterables (custom iterator classes).
                    // Resolve the iterated element type via the iterator protocol:
                    // type[Symbol.iterator]().next().value
                    if self.is_iterable_type(spread_type) {
                        let element_type = self.for_of_element_type(spread_type);

                        // TS2556 check: A non-tuple iterable spread is only valid at
                        // a rest parameter position (same logic as array spread above).
                        let current_expected = expected_for_index(effective_index, expanded_count);

                        let at_rest_position =
                            if let Some(callable_type) = self.ctx.current_callable_type {
                                let ctx = tsz_solver::ContextualTypeContext::with_expected(
                                    self.ctx.types,
                                    callable_type,
                                );
                                ctx.is_rest_parameter_position(effective_index, expanded_count)
                            } else {
                                // No callable type → callee is any/error/unknown; accept spread

                                expected_for_index(usize::MAX / 2, expanded_count).is_some()
                            };

                        if !at_rest_position {
                            if current_expected.is_none() {
                                // No parameter at this position and not at rest → TS2556.
                                if !emitted_ts2556 {
                                    self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                                    emitted_ts2556 = true;
                                }
                                continue;
                            }
                            if !emitted_ts2556 {
                                self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                                emitted_ts2556 = true;
                            }
                            // When TS2556 is emitted, push ANY to suppress a
                            // subsequent TS2345 — tsc only reports TS2556 here.
                            arg_types.push(TypeId::ANY);
                            effective_index += 1;
                            continue;
                        }

                        // Push the iterated element type, not the raw iterator class type
                        arg_types.push(element_type);
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
            let apply_contextual = self.argument_needs_contextual_type(arg_idx);

            let prev_context = self.ctx.contextual_type;
            if apply_contextual {
                self.ctx.contextual_type = expected_type;
            } else {
                // Non-sensitive argument expressions should not inherit an outer
                // contextual type (e.g. variable-initializer context) because that
                // can trigger unnecessary contextual resolution work.
                self.ctx.contextual_type = None;
            }
            let skip_flow = !apply_contextual && self.can_skip_flow_narrowing_for_argument(arg_idx);
            let prev_skip_flow = self.ctx.skip_flow_narrowing;
            if skip_flow {
                self.ctx.skip_flow_narrowing = true;
            }

            let arg_type = self.get_type_of_node(arg_idx);
            if skip_flow {
                self.ctx.skip_flow_narrowing = prev_skip_flow;
            }
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
                // Skip excess property checking when the original (pre-instantiation)
                // parameter type contains a type parameter. For generic calls like
                // `parrot<T extends Named>({name, sayHello() {}})`, the instantiated
                // contextual type is the constraint `Named`, but tsc does not fire
                // excess property checks because `T` captures the full object type.
                && !self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                    effective_index < skip.len() && skip[effective_index]
                })
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
                // Skip excess property checking for type parameters - the type parameter
                // captures the full object type, so extra properties are allowed.
                && !is_type_parameter_type(self.ctx.types, expected)
                // Also skip when the original parameter type contains a type parameter
                // (set via generic_excess_skip for generic call paths).
                && !self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                    i < skip.len() && skip[i]
                })
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
        actual_this_type: Option<TypeId>,
    ) -> Option<TypeId> {
        use tsz_solver::FunctionShape;
        use tsz_solver::operations::CallResult;

        tracing::debug!(
            "resolve_overloaded_call_with_signatures: signatures = {:?}, args = {:?}",
            signatures,
            args
        );
        if signatures.is_empty() {
            return None;
        }

        // Overload contextual typing baseline.
        // First pass collects argument types once using a union of overload signatures.
        // If that fails to find a match, we run a second pass that re-collects arguments
        // per candidate signature with signature-specific contextual types. This helps
        // avoid false TS2345/TS2322 when the union contextual type is too lossy.
        let factory = self.ctx.types.factory();

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
        let union_contextual =
            tsz_solver::utils::union_or_single(self.ctx.types, signature_types.clone());

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
        let prev_callable_type = self.ctx.current_callable_type;
        self.ctx.current_callable_type = Some(union_contextual);
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            false,
            None, // No skipping needed for overload resolution
        );
        self.ctx.current_callable_type = prev_callable_type;
        let temp_node_types = std::mem::take(&mut self.ctx.node_types);

        self.ctx.node_types = std::mem::take(&mut original_node_types);

        // First pass: try each signature with union-contextual argument types.
        for (idx, (_sig, &func_type)) in signatures.iter().zip(signature_types.iter()).enumerate() {
            tracing::debug!("Trying overload {} with {} args", idx, arg_types.len());
            self.ensure_relation_input_ready(func_type);
            let resolved_func_type =
                if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                    self.ctx
                        .type_env
                        .borrow()
                        .get_def(def_id)
                        .unwrap_or(func_type)
                } else {
                    func_type
                };
            let (result, _instantiated_predicate) = self.resolve_call_with_checker_adapter(
                resolved_func_type,
                &arg_types,
                force_bivariant_callbacks,
                self.ctx.contextual_type,
                None,
            );

            match &result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
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
                    // Merge the node types inferred during argument collection
                    self.ctx.node_types.extend(temp_node_types);

                    // CRITICAL FIX - Check excess properties against the MATCHED signature,
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

            let prev_callable_type = self.ctx.current_callable_type;
            self.ctx.current_callable_type = Some(func_type);
            let sig_arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                false,
                None,
            );
            self.ctx.current_callable_type = prev_callable_type;

            self.ensure_relation_input_ready(func_type);

            let resolved_func_type =
                if let Some(def_id) = lazy_def_id_for_type(self.ctx.types, func_type) {
                    self.ctx
                        .type_env
                        .borrow()
                        .get_def(def_id)
                        .unwrap_or(func_type)
                } else {
                    func_type
                };
            let (result, _instantiated_predicate) = self.resolve_call_with_checker_adapter(
                resolved_func_type,
                &sig_arg_types,
                force_bivariant_callbacks,
                self.ctx.contextual_type,
                actual_this_type,
            );

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
