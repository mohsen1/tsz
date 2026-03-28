//! Call expression checking (overload resolution, argument collection, signature instantiation).
//!
//! Contextual typing analysis helpers live in the sibling `call_context` module.

use crate::computation::complex::is_contextually_sensitive;
use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, is_type_parameter_type, lazy_def_id_for_type, resolve_call,
    resolve_new, stable_call_recovery_return_type, tuple_elements_for_type,
};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{
    AssignabilityChecker, CallResult, ContextualTypeContext, PendingDiagnosticBuilder, TypeId,
};

/// Call-local context carrying the callable type during argument collection.
///
/// Replaces the ambient `ctx.current_callable_type` field. Threaded explicitly
/// through `collect_call_argument_types_with_context` and its transitive callees
/// so that rest-parameter position checks (TS2556) and generic excess-property
/// skip decisions can query the callable shape without ambient mutable state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallableContext {
    /// The callable type of the call expression being processed.
    pub callable_type: Option<TypeId>,
}

impl CallableContext {
    pub const fn new(callable_type: TypeId) -> Self {
        Self {
            callable_type: Some(callable_type),
        }
    }

    pub const fn none() -> Self {
        Self {
            callable_type: None,
        }
    }
}

pub(crate) struct OverloadResolution {
    pub(crate) arg_types: Vec<TypeId>,
    pub(crate) result: CallResult,
}

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

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use crate::query_boundaries::state::type_environment::application_info;

        let (base, args) = application_info(self.state.ctx.types, type_id)?;
        let sym_id = self.state.ctx.resolve_type_to_symbol_id(base)?;
        let (body, type_params) = self.state.type_reference_symbol_type_with_params(sym_id);
        if body == TypeId::ANY || body == TypeId::ERROR || type_params.is_empty() {
            return None;
        }
        let subst = TypeSubstitution::from_args(self.state.ctx.types, &type_params, &args);
        let instantiated = instantiate_type(self.state.ctx.types, body, &subst);
        if instantiated == type_id {
            None
        } else {
            Some(instantiated)
        }
    }

    fn promise_like_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.state
            .promise_like_return_type_argument(type_id)
            .or_else(|| {
                let resolved = self.state.resolve_lazy_type(type_id);
                (resolved != type_id)
                    .then(|| self.state.promise_like_return_type_argument(resolved))
                    .flatten()
            })
    }

    fn type_resolver(&self) -> Option<&dyn tsz_solver::TypeResolver> {
        Some(&self.state.ctx)
    }
}

// =============================================================================
// Call Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn callable_context_can_type_function_argument_despite_unresolved(
        &self,
        arg_idx: NodeIndex,
        expected_context_type: Option<TypeId>,
    ) -> bool {
        let Some(expected_context_type) = expected_context_type else {
            return false;
        };
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape
                .params
                .iter()
                .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR);
        }

        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            expected_context_type,
        ) {
            return shape.call_signatures.iter().all(|sig| {
                sig.params
                    .iter()
                    .all(|param| param.type_id != TypeId::UNKNOWN && param.type_id != TypeId::ERROR)
            });
        }

        false
    }

    pub(crate) const fn should_preserve_speculative_call_diagnostic(
        diag: &crate::diagnostics::Diagnostic,
    ) -> bool {
        diag.code == diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS
            || diag.code == diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE
            // TS2454 (variable used before being assigned) is a semantic fact
            // about the variable's definite-assignment status, not a speculative
            // inference result from the call. It must survive call-expression
            // diagnostic rollbacks (round 1 → round 2, return-context re-check).
            || diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
            // TS2304/TS2552/TS2662/TS2663 (cannot find name variants) are semantic
            // facts about name resolution, not speculative inference results.
            // If a name is undefined, it is undefined regardless of which
            // overload is tried.
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
            // TS2872/TS2873 (always truthy/falsy) are purely syntactic facts
            // about expression truthiness, not speculative inference results.
            // They must survive call-expression diagnostic rollbacks.
            || diag.code == diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY
            || diag.code == diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY
            // TS2304/TS2552 (Cannot find name / did you mean?) are name-resolution
            // facts that do not depend on the overload candidate being tried.
            // They must survive speculative rollbacks so undeclared identifiers
            // in argument expressions always produce an error, matching tsc.
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME
            || diag.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
    }

    fn normalized_spread_argument_type(&mut self, expr: NodeIndex) -> TypeId {
        let spread_type = self.get_type_of_node(expr);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        let spread_type = self.evaluate_type_with_env(spread_type);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        self.evaluate_application_type(spread_type)
    }

    fn overload_candidate_has_hard_non_callback_arg_errors(
        &self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        self.ctx
            .speculative_diagnostics_since(snap)
            .iter()
            .any(|diag| {
                args.iter().any(|&arg_idx| {
                    let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                        return false;
                    };
                    let is_callback_arg = arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;
                    !is_callback_arg && diag.start >= arg_node.pos && diag.start < arg_node.end
                })
            })
    }

    fn prune_callback_body_diagnostics(
        &mut self,
        args: &[NodeIndex],
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) {
        // Pre-compute callback body spans before entering the mutable borrow
        // for rollback_diagnostics_filtered.
        let callback_spans: Vec<Option<(u32, u32)>> = args
            .iter()
            .map(|&arg_idx| {
                let arg_node = self.ctx.arena.get(arg_idx)?;
                let is_callback_arg = arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;
                if !is_callback_arg {
                    return None;
                }
                self.callback_body_span(arg_idx)
            })
            .collect();
        self.ctx.rollback_diagnostics_filtered(snap, |diag| {
            !callback_spans.iter().any(|span| {
                span.is_some_and(|(start, end)| diag.start >= start && diag.start < end)
            })
        });
    }

    fn raw_block_body_callback_mismatch(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        args.iter().enumerate().find_map(|(index, &arg_idx)| {
            let arg_node = self.ctx.arena.get(arg_idx)?;
            if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return None;
            }
            let func = self.ctx.arena.get_function(arg_node)?;
            let body = self.ctx.arena.get(func.body)?;
            if body.kind != syntax_kind_ext::BLOCK {
                return None;
            }
            let expected = expected_for_index(self, index)?;
            if expected == TypeId::ERROR || expected == TypeId::UNKNOWN || expected == TypeId::ANY {
                return None;
            }
            let expected_is_concrete = expected != TypeId::ANY
                && expected != TypeId::UNKNOWN
                && expected != TypeId::ERROR
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    expected,
                )
                && !crate::query_boundaries::common::contains_infer_types(self.ctx.types, expected);
            let snap = self.ctx.snapshot_full();
            self.invalidate_expression_for_contextual_retry(arg_idx);
            self.ctx.daa_error_nodes.remove(&arg_idx.0);
            self.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
            let diag_snap = self.ctx.snapshot_diagnostics();
            let actual = self.get_type_of_node_with_request(arg_idx, &TypingRequest::NONE);
            let refined_actual = if self
                .target_has_concrete_return_context_for_generic_refinement(expected)
            {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    actual, expected,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(actual, expected)
            };
            let has_callback_body_diagnostic =
                self.callback_body_span(arg_idx)
                    .is_some_and(|(start, end)| {
                        self.ctx
                            .speculative_diagnostics_since(&diag_snap)
                            .iter()
                            .any(|diag| diag.start >= start && diag.start < end)
                    });
            self.ctx.rollback_full(&snap);
            let is_generator_callback = func.asterisk_token;
            let (has_return_type_mismatch, has_generator_component_mismatch) =
                stable_call_recovery_return_type(self.ctx.types, refined_actual)
                    .zip(stable_call_recovery_return_type(self.ctx.types, expected))
                    .map(|(actual_return, expected_return)| {
                        let generator_component_mismatch = self
                            .get_generator_yield_type_argument(actual_return)
                            .zip(self.get_generator_yield_type_argument(expected_return))
                            .is_some_and(|(actual_yield, expected_yield)| {
                                !self.is_assignable_to(actual_yield, expected_yield)
                            })
                            || self
                                .get_generator_return_type_argument(actual_return)
                                .zip(self.get_generator_return_type_argument(expected_return))
                                .is_some_and(|(actual_gen_return, expected_gen_return)| {
                                    !self.is_assignable_to(actual_gen_return, expected_gen_return)
                                })
                            || self
                                .get_generator_next_type_argument(actual_return)
                                .zip(self.get_generator_next_type_argument(expected_return))
                                .is_some_and(|(actual_next, expected_next)| {
                                    !self.is_assignable_to(expected_next, actual_next)
                                });

                        (
                            generator_component_mismatch
                                || !self.is_assignable_to(actual_return, expected_return),
                            generator_component_mismatch,
                        )
                    })
                    .unwrap_or((false, false));
            ((has_callback_body_diagnostic
                || (is_generator_callback
                    && expected_is_concrete
                    && has_generator_component_mismatch))
                && has_return_type_mismatch)
                .then_some((index, refined_actual, expected))
        })
    }

    pub(crate) fn current_block_body_callback_return_mismatch_arg(
        &mut self,
        args: &[NodeIndex],
        expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        self.raw_block_body_callback_mismatch(args, expected_for_index)
    }

    pub(crate) fn current_binding_pattern_callback_unknown_context_arg(
        &mut self,
        args: &[NodeIndex],
        mut expected_for_index: impl FnMut(&mut Self, usize) -> Option<TypeId>,
    ) -> Option<(usize, TypeId, TypeId)> {
        args.iter().enumerate().find_map(|(index, &arg_idx)| {
            let arg_node = self.ctx.arena.get(arg_idx)?;
            if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return None;
            }
            let func = self.ctx.arena.get_function(arg_node)?;
            let expected = expected_for_index(self, index)?;
            let expected_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected,
            )?;

            let has_unknown_binding_pattern =
                func.parameters
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(param_index, &param_idx)| {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            return false;
                        };
                        if param.type_annotation.is_some() {
                            return false;
                        }
                        let has_binding_pattern =
                            self.ctx.arena.get(param.name).is_some_and(|name_node| {
                                name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            });
                        if !has_binding_pattern {
                            return false;
                        }
                        expected_shape
                            .params
                            .get(param_index)
                            .map(|param| param.type_id)
                            .or_else(|| {
                                let last = expected_shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                            .is_some_and(|param_type| {
                                let unconstrained_type_param =
                                    crate::query_boundaries::common::type_parameter_constraint(
                                        self.ctx.types,
                                        param_type,
                                    )
                                    .is_none_or(|constraint| {
                                        constraint == TypeId::UNKNOWN
                                    || constraint == TypeId::ERROR
                                    || crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        constraint,
                                    )
                                    || crate::query_boundaries::common::contains_infer_types(
                                        self.ctx.types,
                                        constraint,
                                    )
                                    });
                                param_type == TypeId::UNKNOWN
                                    || param_type == TypeId::ERROR
                                    || crate::query_boundaries::common::contains_infer_types(
                                        self.ctx.types,
                                        param_type,
                                    )
                                    || (crate::query_boundaries::common::is_type_parameter_like(
                                        self.ctx.types,
                                        param_type,
                                    ) && unconstrained_type_param)
                            })
                    });

            if !has_unknown_binding_pattern {
                return None;
            }

            let actual = self.get_type_of_node_with_request(arg_idx, &TypingRequest::NONE);
            Some((index, actual, expected))
        })
    }

    pub(crate) fn collect_non_callback_diagnostics_between(
        &self,
        args: &[NodeIndex],
        from_snap: &crate::context::speculation::DiagnosticSnapshot,
        to_snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        self.ctx
            .diagnostics_between(from_snap, to_snap)
            .iter()
            .filter(|diag| {
                !args.iter().any(|&arg_idx| {
                    let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                        return false;
                    };
                    // Object literal arguments with methods have body diagnostics
                    // that depend on contextual typing (e.g., ThisType<T> markers).
                    // Filter them like callback body diagnostics so they don't
                    // persist when a different overload resolves successfully.
                    let is_context_sensitive_arg = arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                        || arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
                    if !is_context_sensitive_arg {
                        return false;
                    }
                    if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        // For object literals, filter diagnostics within the entire span
                        diag.start >= arg_node.pos && diag.start < arg_node.end
                    } else {
                        self.callback_body_span(arg_idx)
                            .is_some_and(|(start, end)| diag.start >= start && diag.start < end)
                    }
                })
            })
            .cloned()
            .collect()
    }

    pub(crate) fn preserved_speculative_call_diagnostics(
        &self,
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> Vec<crate::diagnostics::Diagnostic> {
        self.ctx
            .speculative_diagnostics_since(snap)
            .iter()
            .filter(|diag| Self::should_preserve_speculative_call_diagnostic(diag))
            .cloned()
            .collect()
    }

    pub(crate) fn extend_unique_diagnostics(
        &self,
        dest: &mut Vec<crate::diagnostics::Diagnostic>,
        source: impl IntoIterator<Item = crate::diagnostics::Diagnostic>,
    ) {
        let mut seen = rustc_hash::FxHashSet::default();
        for diag in dest.iter() {
            seen.insert(self.ctx.diagnostic_dedup_key(diag));
        }
        for diag in source {
            let key = self.ctx.diagnostic_dedup_key(&diag);
            if seen.insert(key) {
                dest.push(diag);
            }
        }
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
    ) -> tsz_solver::operations::CallWithCheckerResult {
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
        callable_ctx: CallableContext,
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
                let spread_type = self.normalized_spread_argument_type(spread_data.expression);
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
                    let spread_type = self.normalized_spread_argument_type(spread_data.expression);

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
                                                    .union2(sub.type_id, TypeId::UNDEFINED)
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
                                        .union2(elem.type_id, TypeId::UNDEFINED)
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
                            crate::query_boundaries::common::type_parameter_constraint(
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
                                if let Some(callable_type) = callable_ctx.callable_type {
                                    let ctx = tsz_solver::ContextualTypeContext::with_expected(
                                        self.ctx.types,
                                        callable_type,
                                    );
                                    ctx.allows_non_tuple_spread_position(
                                        effective_index,
                                        expanded_count,
                                    )
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
                        let element_type = self.for_of_element_type(spread_type, false);

                        // TS2556 check: A non-tuple iterable spread is only valid at
                        // a rest parameter position (same logic as array spread above).
                        let current_expected = expected_for_index(effective_index, expanded_count);

                        let at_rest_position = if let Some(callable_type) =
                            callable_ctx.callable_type
                        {
                            let ctx = tsz_solver::ContextualTypeContext::with_expected(
                                self.ctx.types,
                                callable_type,
                            );
                            ctx.allows_non_tuple_spread_position(effective_index, expanded_count)
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
            let unresolved_refresh_context = expected_type.is_some_and(|ty| {
                ty == TypeId::UNKNOWN
                    || ty == TypeId::ERROR
                    || tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, ty)
            });
            let expected_context_type = self.contextual_type_option_for_call_argument_at(
                expected_type,
                arg_idx,
                Some(effective_index),
                Some(expanded_count),
                callable_ctx,
            );
            let can_apply_contextual_despite_unresolved = unresolved_refresh_context
                && self.callable_context_can_type_function_argument_despite_unresolved(
                    arg_idx,
                    expected_context_type,
                );
            let apply_contextual = self.argument_needs_contextual_type(arg_idx)
                && (!unresolved_refresh_context || can_apply_contextual_despite_unresolved);
            let raw_context_requires_generic_epc_skip = expected_context_type.is_some_and(|ty| {
                crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                    || crate::computation::call_inference::should_preserve_contextual_application_shape(
                        self.ctx.types,
                        ty,
                    )
            });
            let callable_context_requires_generic_epc_skip =
                callable_ctx.callable_type.is_some_and(|callable_type| {
                    let ctx =
                        tsz_solver::ContextualTypeContext::with_expected(self.ctx.types, callable_type);
                    ctx.get_parameter_type_for_call(effective_index, expanded_count)
                        .is_some_and(|param_type| {
                            crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                param_type,
                            ) || crate::computation::call_inference::should_preserve_contextual_application_shape(
                                self.ctx.types,
                                param_type,
                            )
                        })
                });

            // Extract ThisType<T> marker from the unevaluated expected type BEFORE
            // contextual_type_for_expression evaluates it away. ThisType<T> is an empty
            // interface marker, so intersection simplification removes it. We need to
            // preserve it for object literal methods' `this` type.
            let pushed_this_type = if let Some(et) = expected_type {
                use tsz_solver::ContextualTypeContext;
                let ctx_helper = ContextualTypeContext::with_expected_and_options(
                    self.ctx.types,
                    et,
                    self.ctx.compiler_options.no_implicit_any,
                );
                // First try simple extraction (no alias expansion needed).
                // If that fails, use the resolver to expand type aliases
                // (e.g., ConstructorOptions<Data> → ... & ThisType<Instance<Data>>).
                let this_type = ctx_helper.get_this_type_from_marker().or_else(|| {
                    let env = self.ctx.type_env.borrow();
                    ctx_helper.get_this_type_from_marker_with_resolver(&*env)
                });
                // If the expected type (which may be an already-evaluated/instantiated
                // parameter type) doesn't contain ThisType, try the callable's original
                // parameter type. During generic argument refresh (second pass), the
                // refreshed contextual types lose ThisType<T> because evaluation strips
                // empty marker interfaces. The callable's original parameter type still
                // has it.
                let this_type = this_type.or_else(|| {
                    let callable_type = callable_ctx.callable_type?;
                    let callable_ctx_helper =
                        ContextualTypeContext::with_expected(self.ctx.types, callable_type);
                    let param_type = callable_ctx_helper
                        .get_parameter_type_for_call(effective_index, expanded_count)?;
                    let param_ctx_helper = ContextualTypeContext::with_expected_and_options(
                        self.ctx.types,
                        param_type,
                        self.ctx.compiler_options.no_implicit_any,
                    );
                    param_ctx_helper.get_this_type_from_marker().or_else(|| {
                        let env = self.ctx.type_env.borrow();
                        param_ctx_helper.get_this_type_from_marker_with_resolver(&*env)
                    })
                });
                if let Some(this_type) = this_type {
                    self.ctx.this_type_stack.push(this_type);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            let skip_flow = !apply_contextual && self.can_skip_flow_narrowing_for_argument(arg_idx);
            let request = if apply_contextual {
                match expected_context_type {
                    Some(ty) => TypingRequest::with_contextual_type(ty),
                    None => TypingRequest::NONE,
                }
            } else if skip_flow {
                TypingRequest::for_write_context()
            } else {
                TypingRequest::NONE
            };
            let arg_snap = self.ctx.snapshot_diagnostics();
            let arg_type = self.get_type_of_node_with_request(arg_idx, &request);

            let is_direct_function_arg = self.ctx.arena.get(arg_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            });
            let arg_node = self.ctx.arena.get(arg_idx);
            let is_sensitive_contextual_arg = apply_contextual
                && expected_type.is_some()
                && arg_node.is_some_and(|arg_node| {
                    is_contextually_sensitive(self, arg_idx)
                        || (arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            && self.ctx.generic_excess_skip.is_some())
                });
            if is_sensitive_contextual_arg {
                let arg_node = arg_node.expect("sensitive contextual arg should exist");
                let object_literal_function_param_spans =
                    if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        self.object_literal_function_like_param_spans(arg_idx)
                    } else {
                        Vec::new()
                    };
                let callback_body_start = self
                    .ctx
                    .arena
                    .get_function(arg_node)
                    .and_then(|func| self.ctx.arena.get(func.body))
                    .filter(|body_node| body_node.kind != syntax_kind_ext::BLOCK)
                    .map(|body_node| body_node.pos);
                let object_literal_has_excess_property_diag = arg_node.kind
                    == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && self
                        .ctx
                        .speculative_diagnostics_since(&arg_snap)
                        .iter()
                        .any(|diag| {
                            diag.code
                                == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                                || diag.code
                                    == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                        });
                // Build pre-existing diagnostic keys for exact dedup.
                let existing_diag_keys: Vec<_> = self
                    .ctx
                    .diagnostics
                    .iter()
                    .take(arg_snap.diagnostics_len)
                    .map(|d| (d.code, d.start, d.length, d.message_text.clone()))
                    .collect();
                let mut seen_diag_keys = existing_diag_keys;
                self.ctx.rollback_diagnostics_filtered(&arg_snap, |diag| {
                    if Self::should_preserve_speculative_call_diagnostic(diag) {
                        return true;
                    }
                    let is_provisional_assignability = diag.code
                        == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        || diag.code
                            == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;
                    let is_provisional_implicit_any = matches!(
                        diag.code,
                        diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                            | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                            | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                            | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                    );
                    let is_callback_body_diag =
                        callback_body_start.is_some_and(|start| diag.start >= start);
                    let is_object_literal_diag = arg_node.kind
                        == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && diag.start >= arg_node.pos
                        && diag.start < arg_node.end;
                    let is_object_literal_function_param_implicit_any = unresolved_refresh_context
                        && is_provisional_implicit_any
                        && object_literal_function_param_spans
                            .iter()
                            .any(|(start, end)| diag.start >= *start && diag.start < *end);
                    let is_function_arg_implicit_any_diag =
                        (arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                            || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                            && is_provisional_implicit_any
                            && diag.start >= arg_node.pos
                            && diag.start < arg_node.end;
                    let is_direct_callback_body_assignability =
                        callback_body_start.is_some_and(|start| diag.start == start)
                            && is_provisional_assignability;
                    let keep = if !is_provisional_assignability && !is_provisional_implicit_any {
                        true
                    } else if is_direct_function_arg {
                        is_direct_callback_body_assignability
                            || !(is_callback_body_diag || is_function_arg_implicit_any_diag)
                    } else if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        // Generic contextual refresh re-checks object literal members with
                        // instantiated parameter types. Drop provisional TS7006/TS7031
                        // from function-like members while the expected type still contains
                        // unresolved type parameters/infer placeholders; keep other
                        // object-literal implicit-any diagnostics and all definitive errors.
                        // If the same pass has already established TS2353 for an excess key,
                        // preserve the callback's implicit-any diagnostics because there is
                        // no later contextual refresh that can make that member valid.
                        !is_object_literal_diag
                            || (is_provisional_implicit_any
                                && !is_provisional_assignability
                                && (!is_object_literal_function_param_implicit_any
                                    || object_literal_has_excess_property_diag))
                    } else {
                        // For array literals and other contextually-sensitive args,
                        // keep implicit-any diagnostics (TS7006/TS7019).
                        is_provisional_implicit_any && !is_provisional_assignability
                    };
                    // Exact-message dedup against pre-existing diagnostics.
                    if keep {
                        let full_key = (
                            diag.code,
                            diag.start,
                            diag.length,
                            diag.message_text.clone(),
                        );
                        if seen_diag_keys.iter().any(|existing| existing == &full_key) {
                            return false;
                        }
                        seen_diag_keys.push(full_key);
                    }
                    keep
                });
            }
            // Unresolved infer types in expected type → callback was processed without
            // contextual types. Drop provisional implicit-any diagnostics (TS7006/TS7031).
            if unresolved_refresh_context
                && is_direct_function_arg
                && let Some(n) = self.ctx.arena.get(arg_idx)
            {
                let (s, e) = (n.pos, n.end);
                let count_before = self.ctx.diagnostics.len();
                self.ctx.rollback_diagnostics_filtered(&arg_snap, |d| {
                    !(matches!(d.code, 7006 | 7019 | 7031 | 7051) && d.start >= s && d.start < e)
                });
                if self.ctx.diagnostics.len() < count_before {
                    self.ctx.implicit_any_checked_closures.remove(&arg_idx);
                }
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
                // Note: we only check skip[i], not whether `expected` still contains
                // type parameters — after inference, expected is fully instantiated
                // but tsc still skips EPC based on the original parameter type.
                && !self.ctx.generic_excess_skip.as_ref().is_some_and(|skip| {
                    effective_index < skip.len() && skip[effective_index]
                })
                && !raw_context_requires_generic_epc_skip
                && !callable_context_requires_generic_epc_skip
                && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
            {
                self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
            }

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }
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
                && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
            {
                let arg_type = arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN);
                self.check_object_literal_excess_properties(arg_type, expected, arg_idx);
            }
        }
    }

    fn validate_non_tuple_spreads_for_signature(&mut self, args: &[NodeIndex], func_type: TypeId) {
        let ctx = ContextualTypeContext::with_expected(self.ctx.types, func_type);
        let mut expanded_count = 0usize;
        for &arg_idx in args {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                && arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
            {
                let spread_type = self.normalized_spread_argument_type(spread_data.expression);
                if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                    expanded_count += elems.len();
                    continue;
                }
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

        let mut effective_index = 0usize;
        for &arg_idx in args {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                effective_index += elems.len();
                continue;
            }
            if is_type_parameter_type(self.ctx.types, spread_type)
                && let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    spread_type,
                )
                && (array_element_type_for_type(self.ctx.types, constraint).is_some()
                    || tuple_elements_for_type(self.ctx.types, constraint).is_some())
            {
                effective_index += 1;
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if is_non_tuple_spread
                && !ctx.allows_non_tuple_spread_position(effective_index, expanded_count)
            {
                self.error_spread_must_be_tuple_or_rest_at(arg_idx);
                return;
            }
            effective_index += 1;
        }
    }

    fn find_prior_non_tuple_spread_for_mismatch(
        &mut self,
        args: &[NodeIndex],
        mismatch_index: usize,
    ) -> Option<NodeIndex> {
        let mut effective_index = 0usize;
        let mut prior_non_tuple_spread = None;

        for &arg_idx in args {
            if effective_index > mismatch_index {
                break;
            }
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                effective_index += 1;
                continue;
            };
            if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                if effective_index == mismatch_index {
                    return prior_non_tuple_spread;
                }
                effective_index += 1;
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_spread(arg_node) else {
                effective_index += 1;
                continue;
            };
            let spread_type = self.normalized_spread_argument_type(spread_data.expression);
            if let Some(elems) = tuple_elements_for_type(self.ctx.types, spread_type) {
                if mismatch_index < effective_index + elems.len() {
                    return prior_non_tuple_spread;
                }
                effective_index += elems.len();
                continue;
            }
            let is_non_tuple_spread = array_element_type_for_type(self.ctx.types, spread_type)
                .is_some()
                || self.is_iterable_type(spread_type);
            if effective_index == mismatch_index {
                return prior_non_tuple_spread;
            }
            if is_non_tuple_spread {
                prior_non_tuple_spread = Some(arg_idx);
            }
            effective_index += 1;
        }

        prior_non_tuple_spread
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
    /// - `Some(OverloadResolution)` if overload resolution was attempted
    /// - `None` if there were no overload signatures to resolve
    pub(crate) fn resolve_overloaded_call_with_signatures(
        &mut self,
        args: &[NodeIndex],
        signatures: &[tsz_solver::CallSignature],
        force_bivariant_callbacks: bool,
        contextual_type: Option<TypeId>,
        actual_this_type: Option<TypeId>,
    ) -> Option<OverloadResolution> {
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
                    type_predicate: sig.type_predicate,
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
        let contextual_refresh_args: Vec<_> = args
            .iter()
            .copied()
            .filter(|&arg_idx| self.argument_needs_contextual_type(arg_idx))
            .collect();
        let refresh_all_args = |this: &mut Self| {
            for &arg_idx in args {
                this.invalidate_expression_for_contextual_retry(arg_idx);
                this.ctx.daa_error_nodes.remove(&arg_idx.0);
                this.ctx.flow_narrowed_nodes.remove(&arg_idx.0);
            }
        };

        let mut original_node_types = std::mem::take(&mut self.ctx.node_types);

        // Snapshot all speculative state before overload resolution begins.
        // This captures diagnostics, emitted_diagnostics dedup set, TS2454
        // dedup state, TS2307 module dedup, and implicit-any-checked-closures.
        // On failure paths we roll back to this snapshot; on success paths
        // we selectively keep diagnostics via the transaction API.
        let overload_snap = self.ctx.snapshot_full();

        // Collect argument types ONCE with union contextual type.
        // Diagnostics produced during this pass are speculative: if no overload
        // matches, TypeScript reports the overload failure and suppresses these
        // nested callback/body diagnostics.
        self.ctx.node_types = Default::default();
        // Clear the contextual resolution cache once before the loop — the cache
        // is shared and needs clearing before any arg is re-evaluated, but clearing
        // it per-arg was redundant (empty after the first iteration).
        self.clear_contextual_resolution_cache();
        for &arg_idx in &contextual_refresh_args {
            self.invalidate_expression_for_contextual_retry(arg_idx);
        }
        let union_callable_ctx = CallableContext::new(union_contextual);
        // Preserve literal types during overload argument collection so that
        // string/number literal arguments keep their literal types (e.g., "canvas"
        // stays as literal "canvas" instead of widening to string).  This is
        // critical for overload resolution: without it, the union contextual type
        // (which collapses literal | string → string) causes all literal overloads
        // to fail matching.
        let prev_preserve_literals = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let arg_types = self.collect_call_argument_types_with_context(
            args,
            |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
            false,
            None, // No skipping needed for overload resolution
            union_callable_ctx,
        );
        self.ctx.preserve_literal_types = prev_preserve_literals;
        let temp_node_types = std::mem::take(&mut self.ctx.node_types);

        self.ctx.node_types = std::mem::take(&mut original_node_types);

        // First pass: try each signature with union-contextual argument types.
        for (idx, (sig, &func_type)) in signatures.iter().zip(signature_types.iter()).enumerate() {
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
            let (result, _instantiated_predicate, instantiated_params) = self
                .resolve_call_with_checker_adapter(
                    resolved_func_type,
                    &arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
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
                    if self
                        .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                            ContextualTypeContext::with_expected_and_options(
                                checker.ctx.types,
                                func_type,
                                checker.ctx.compiler_options.no_implicit_any,
                            )
                            .get_parameter_type_for_call(index, args.len())
                        })
                        .is_some()
                    {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }
                    if self.overload_candidate_has_hard_non_callback_arg_errors(
                        args,
                        &overload_snap.diag,
                    ) {
                        self.prune_callback_body_diagnostics(args, &overload_snap.diag);
                        continue;
                    }

                    // When the matched overload is generic and has contextual refresh args,
                    // re-collect argument types with instantiated parameter types. The first
                    // pass used the union-contextual type which has unresolved type parameters,
                    // causing false diagnostics in callback bodies (e.g., TS2339 for `this.b`
                    // when `this` has type `TContext` instead of the inferred `{b: string}`).
                    let mut did_instantiated_retry = false;
                    let final_arg_types = if !sig.type_params.is_empty()
                        && !contextual_refresh_args.is_empty()
                        && let Some(instantiated_params) = instantiated_params.as_ref()
                    {
                        let contextual_closures: Vec<_> = self
                            .ctx
                            .implicit_any_contextual_closures
                            .iter()
                            .copied()
                            .collect();
                        self.ctx.rollback_full(&overload_snap);
                        self.ctx
                            .implicit_any_checked_closures
                            .extend(contextual_closures);
                        self.ctx.node_types = Default::default();
                        refresh_all_args(self);

                        let sig_callable_ctx = CallableContext::new(func_type);
                        let refreshed_contextual_types = self
                            .contextual_param_types_from_instantiated_params(
                                instantiated_params,
                                args.len(),
                            );
                        let refreshed_arg_types = self.collect_call_argument_types_with_context(
                            args,
                            |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                            false,
                            None,
                            sig_callable_ctx,
                        );
                        did_instantiated_retry = true;
                        refreshed_arg_types
                    } else {
                        arg_types.clone()
                    };

                    // Merge the node types inferred during argument collection.
                    // If we did the instantiated retry, node_types already contains the
                    // refreshed entries; otherwise merge the first-pass temp_node_types.
                    if !did_instantiated_retry {
                        self.ctx.node_types.extend(temp_node_types);
                    }
                    self.validate_non_tuple_spreads_for_signature(args, func_type);

                    // CRITICAL FIX - Check excess properties against the MATCHED signature,
                    // not the union. Using the union would allow properties that exist in other overloads
                    // but not in the selected one, causing false negatives.
                    let matched_sig_helper = ContextualTypeContext::with_expected_and_options(
                        self.ctx.types,
                        func_type,
                        self.ctx.compiler_options.no_implicit_any,
                    );
                    self.check_call_argument_excess_properties(
                        args,
                        &final_arg_types,
                        |i, arg_count| matched_sig_helper.get_parameter_type_for_call(i, arg_count),
                    );

                    return Some(OverloadResolution {
                        arg_types: final_arg_types,
                        result: CallResult::Success(return_type),
                    });
                }
                CallResult::ArgumentTypeMismatch { index, .. } => {
                    if let Some(spread_idx) =
                        self.find_prior_non_tuple_spread_for_mismatch(args, index)
                    {
                        self.error_spread_must_be_tuple_or_rest_at(spread_idx);
                        self.ctx.node_types.extend(temp_node_types);
                        return Some(OverloadResolution {
                            arg_types: arg_types.clone(),
                            result: CallResult::Success(sig.return_type),
                        });
                    }
                }
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // Constraint violation from callback return - overload matched
                    // but with constraint error. If there are more overloads to try,
                    // continue to the next one (e.g., Object.freeze overload 0 is
                    // `T extends Function` which is violated for object args — we
                    // must try overload 1 `T extends {[idx:string]:U}`).
                    if signatures.len() > 1 {
                        continue;
                    }
                    self.ctx.node_types.extend(temp_node_types);
                    return Some(OverloadResolution {
                        arg_types: arg_types.clone(),
                        result: CallResult::Success(return_type),
                    });
                }
                _ => {}
            }
        }

        // Second pass: signature-specific contextual typing.
        // Some overload sets require contextual typing from a specific candidate to
        // type callback/object-literal arguments correctly. The union pass above can
        // miss those, producing false negatives and downstream false TS2345/TS2322.
        let mut failures = Vec::new();
        // When a signature returns TypeParameterConstraintViolation and there are more
        // overloads to try, store it as a fallback and continue. Used for Object.freeze
        // where overload 0 (T extends Function) is violated but overload 1 works.
        let _constraint_violation_fallback: Option<(TypeId, Vec<TypeId>)> = None;
        let mut all_arg_count_mismatches = true;
        let mut any_has_rest = false;
        let mut exact_expected_counts = std::collections::BTreeSet::new();
        let mut min_expected = usize::MAX;
        let mut max_expected = 0usize;
        let mut type_mismatch_count = 0usize;
        let mut has_non_count_non_type_failure = false;
        let mut best_type_mismatch: Option<(OverloadResolution, FxHashMap<u32, TypeId>)> = None;
        // When an overload returns TypeParameterConstraintViolation and there are
        // more overloads to try, we store it as a fallback and continue. If no
        // later overload succeeds, we use this fallback (e.g., for single-overload
        // constraint violations that must still resolve to a return type).
        let mut constraint_violation_fallback: Option<(TypeId, Vec<TypeId>)> = None;
        for (sig, &func_type) in signatures.iter().zip(signature_types.iter()) {
            let sig_helper = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                func_type,
                self.ctx.compiler_options.no_implicit_any,
            );

            // Reset TS2454 state to the pre-overload baseline for each candidate.
            self.ctx
                .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
            // Snapshot per-candidate diagnostic state so we can roll back on mismatch.
            let candidate_snap = self.ctx.snapshot_diagnostics();
            let candidate_ts2454_errors = self.ctx.emitted_ts2454_errors.clone();
            self.ctx.node_types = Default::default();
            refresh_all_args(self);
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

            let candidate_callable_ctx = CallableContext::new(func_type);
            let candidate_param_types: Vec<Option<TypeId>> = (0..args.len())
                .map(|i| {
                    sig_helper
                        .get_parameter_type_for_call(i, args.len())
                        .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect();
            let candidate_refresh_args: Vec<bool> = args
                .iter()
                .enumerate()
                .map(|(i, &arg_idx)| {
                    self.argument_needs_refresh_for_contextual_call(
                        arg_idx,
                        candidate_param_types.get(i).copied().flatten(),
                    )
                })
                .collect();
            let should_preinfer_candidate = !sig.type_params.is_empty()
                && candidate_refresh_args
                    .iter()
                    .copied()
                    .any(std::convert::identity);

            let prev_preserve_literals2 = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            let mut sig_arg_types = if should_preinfer_candidate {
                let round1_arg_types = self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| {
                        if candidate_refresh_args.get(i).copied().unwrap_or(false) {
                            None
                        } else {
                            candidate_param_types.get(i).copied().flatten()
                        }
                    },
                    false,
                    Some(&candidate_refresh_args),
                    candidate_callable_ctx,
                );
                let instantiated_params = self
                    .resolve_call_with_checker_adapter(
                        resolved_func_type,
                        &round1_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                        actual_this_type,
                    )
                    .2;

                if let Some(instantiated_params) = instantiated_params.as_ref() {
                    self.ctx
                        .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                            Self::should_preserve_speculative_call_diagnostic(diag)
                        });
                    self.ctx.restore_ts2454_state(&candidate_ts2454_errors);
                    self.ctx.node_types = Default::default();
                    refresh_all_args(self);

                    let refreshed_contextual_types = self
                        .contextual_param_types_from_instantiated_params(
                            instantiated_params,
                            args.len(),
                        )
                        .into_iter()
                        .map(|param_type| {
                            param_type.map(|param_type| {
                                self.normalize_contextual_call_param_type(param_type)
                            })
                        })
                        .collect::<Vec<_>>();
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| {
                            refreshed_contextual_types
                                .get(i)
                                .copied()
                                .flatten()
                                .or_else(|| candidate_param_types.get(i).copied().flatten())
                        },
                        false,
                        None,
                        candidate_callable_ctx,
                    )
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| candidate_param_types.get(i).copied().flatten(),
                        false,
                        None,
                        candidate_callable_ctx,
                    )
                }
            } else {
                self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| candidate_param_types.get(i).copied().flatten(),
                    false,
                    None,
                    candidate_callable_ctx,
                )
            };
            self.ctx.preserve_literal_types = prev_preserve_literals2;

            self.ensure_relation_input_ready(func_type);

            let (mut result, _instantiated_predicate, instantiated_params) = self
                .resolve_call_with_checker_adapter(
                    resolved_func_type,
                    &sig_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                );
            if !sig.type_params.is_empty()
                && !contextual_refresh_args.is_empty()
                && let Some(instantiated_params) = instantiated_params.as_ref()
            {
                let candidate_first_pass_end = self.ctx.snapshot_diagnostics();
                let preserved_candidate_arg_diags = self.collect_non_callback_diagnostics_between(
                    args,
                    &candidate_snap,
                    &candidate_first_pass_end,
                );
                self.ctx
                    .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                        Self::should_preserve_speculative_call_diagnostic(diag)
                    });
                if !preserved_candidate_arg_diags.is_empty() {
                    let mut merged = self.preserved_speculative_call_diagnostics(&candidate_snap);
                    self.extend_unique_diagnostics(&mut merged, preserved_candidate_arg_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&candidate_snap, merged);
                }
                self.ctx.restore_ts2454_state(&candidate_ts2454_errors);
                self.ctx.node_types = Default::default();
                refresh_all_args(self);

                let retry_callable_ctx = CallableContext::new(func_type);
                let refreshed_contextual_types = self
                    .contextual_param_types_from_instantiated_params(
                        instantiated_params,
                        args.len(),
                    );
                let refreshed_arg_types = self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| refreshed_contextual_types.get(i).copied().flatten(),
                    false,
                    None,
                    retry_callable_ctx,
                );

                let (retry_result, _retry_predicate, _retry_instantiated_params) = self
                    .resolve_call_with_checker_adapter(
                        resolved_func_type,
                        &refreshed_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                        actual_this_type,
                    );
                match retry_result {
                    CallResult::Success(_)
                    | CallResult::ArgumentTypeMismatch { .. }
                    | CallResult::TypeParameterConstraintViolation { .. } => {
                        sig_arg_types = refreshed_arg_types;
                        result = retry_result;
                    }
                    _ => {}
                }
            }

            match result {
                CallResult::Success(return_type) => {
                    if let Some((index, actual, expected)) = self
                        .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                            sig_helper
                                .get_parameter_type_for_call(index, args.len())
                                .or_else(|| {
                                    ContextualTypeContext::with_expected_and_options(
                                        checker.ctx.types,
                                        func_type,
                                        checker.ctx.compiler_options.no_implicit_any,
                                    )
                                    .get_parameter_type_for_call(index, args.len())
                                })
                        })
                    {
                        all_arg_count_mismatches = false;
                        type_mismatch_count += 1;
                        if type_mismatch_count == 1 {
                            best_type_mismatch = Some((
                                OverloadResolution {
                                    arg_types: sig_arg_types.clone(),
                                    result: CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected,
                                        actual,
                                        fallback_return: return_type,
                                    },
                                },
                                std::mem::take(&mut self.ctx.node_types),
                            ));
                        }
                        failures.push(PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ));
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    if self
                        .overload_candidate_has_hard_non_callback_arg_errors(args, &candidate_snap)
                    {
                        let preserved_first_pass_diags = self
                            .collect_non_callback_diagnostics_between(
                                args,
                                &overload_snap.diag,
                                &candidate_snap,
                            );
                        let kept_candidate_diags =
                            self.ctx.take_speculative_diagnostics(&candidate_snap);
                        // Merge: preserve hard speculative call diagnostics
                        // (e.g. TS2302/TS2708), then append first-pass and
                        // candidate diagnostics without duplication.
                        let mut merged =
                            self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                        self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                        self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                        self.ctx
                            .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                        let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                        self.ctx.node_types = std::mem::take(&mut original_node_types);
                        self.ctx.node_types.extend(sig_node_types);
                        self.validate_non_tuple_spreads_for_signature(args, func_type);
                        self.check_call_argument_excess_properties(
                            args,
                            &sig_arg_types,
                            |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                        );

                        return Some(OverloadResolution {
                            arg_types: sig_arg_types,
                            result: CallResult::Success(return_type),
                        });
                    }
                    let preserved_first_pass_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &overload_snap.diag,
                        &candidate_snap,
                    );
                    let kept_candidate_diags =
                        self.ctx.take_speculative_diagnostics(&candidate_snap);
                    // Merge: preserve hard speculative call diagnostics
                    // (e.g. TS2302/TS2708), then append first-pass and
                    // candidate diagnostics without duplication.
                    let mut merged =
                        self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                    self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                    self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                    let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                    self.ctx.node_types = std::mem::take(&mut original_node_types);
                    self.ctx.node_types.extend(sig_node_types);
                    self.validate_non_tuple_spreads_for_signature(args, func_type);

                    self.check_call_argument_excess_properties(
                        args,
                        &sig_arg_types,
                        |i, arg_count| sig_helper.get_parameter_type_for_call(i, arg_count),
                    );

                    return Some(OverloadResolution {
                        arg_types: sig_arg_types,
                        result: CallResult::Success(return_type),
                    });
                }
                CallResult::ArgumentTypeMismatch { index, .. } => {
                    if let Some(spread_idx) =
                        self.find_prior_non_tuple_spread_for_mismatch(args, index)
                    {
                        self.ctx.node_types = std::mem::take(&mut original_node_types);
                        self.error_spread_must_be_tuple_or_rest_at(spread_idx);
                        return Some(OverloadResolution {
                            arg_types: sig_arg_types,
                            result: CallResult::Success(sig.return_type),
                        });
                    }

                    all_arg_count_mismatches = false;
                    if let CallResult::ArgumentTypeMismatch {
                        expected,
                        actual,
                        fallback_return,
                        ..
                    } = result
                    {
                        type_mismatch_count += 1;
                        if type_mismatch_count == 1 {
                            best_type_mismatch = Some((
                                OverloadResolution {
                                    arg_types: sig_arg_types.clone(),
                                    result: CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected,
                                        actual,
                                        fallback_return,
                                    },
                                },
                                std::mem::take(&mut self.ctx.node_types),
                            ));
                        }
                        failures.push(PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ));
                    }
                }
                CallResult::ArgumentCountMismatch {
                    expected_min,
                    expected_max,
                    actual,
                } => {
                    if expected_max.is_none() {
                        any_has_rest = true;
                    } else if expected_min == expected_max.unwrap_or(expected_min) {
                        exact_expected_counts.insert(expected_min);
                    }
                    let max = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(max);
                    failures.push(PendingDiagnosticBuilder::argument_count_mismatch(
                        expected_min,
                        max,
                        actual,
                    ));
                }
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // If more overloads remain, store this as a fallback and try next.
                    // This handles cases like Object.freeze where overload 0
                    // (T extends Function) is violated for object args but overload 1
                    // (T extends {[idx:string]:U}) should be tried next.
                    if signatures.len() > 1 && constraint_violation_fallback.is_none() {
                        constraint_violation_fallback = Some((return_type, sig_arg_types.clone()));
                        self.ctx
                            .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                                Self::should_preserve_speculative_call_diagnostic(diag)
                            });
                        continue;
                    }
                    let preserved_first_pass_diags = self.collect_non_callback_diagnostics_between(
                        args,
                        &overload_snap.diag,
                        &candidate_snap,
                    );
                    let kept_candidate_diags =
                        self.ctx.take_speculative_diagnostics(&candidate_snap);
                    let mut merged =
                        self.preserved_speculative_call_diagnostics(&overload_snap.diag);
                    self.extend_unique_diagnostics(&mut merged, preserved_first_pass_diags);
                    self.extend_unique_diagnostics(&mut merged, kept_candidate_diags);
                    self.ctx
                        .rollback_and_replace_diagnostics(&overload_snap.diag, merged);
                    let sig_node_types = std::mem::take(&mut self.ctx.node_types);
                    self.ctx.node_types = std::mem::take(&mut original_node_types);
                    self.ctx.node_types.extend(sig_node_types);
                    return Some(OverloadResolution {
                        arg_types: sig_arg_types,
                        result: CallResult::Success(return_type),
                    });
                }
                _ => {
                    all_arg_count_mismatches = false;
                    has_non_count_non_type_failure = true;
                }
            }

            self.ctx
                .rollback_diagnostics_filtered(&candidate_snap, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
        }

        if !has_non_count_non_type_failure
            && type_mismatch_count == 1
            && let Some((best_type_mismatch, sig_node_types)) = best_type_mismatch
        {
            self.ctx
                .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
            self.ctx
                .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);
            self.ctx.node_types = std::mem::take(&mut original_node_types);
            self.ctx.node_types.extend(sig_node_types);
            return Some(best_type_mismatch);
        }

        // If we encountered a TypeParameterConstraintViolation while trying overloads
        // but no later overload succeeded cleanly, use the constraint-violation result
        // as a successful resolution (the return type is still valid; only the
        // constraint check itself failed, which is reported separately).
        if let Some((fallback_return_type, fallback_arg_types)) = constraint_violation_fallback {
            self.ctx
                .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                    Self::should_preserve_speculative_call_diagnostic(diag)
                });
            self.ctx.node_types = original_node_types;
            return Some(OverloadResolution {
                arg_types: fallback_arg_types,
                result: CallResult::Success(fallback_return_type),
            });
        }

        // No overload matched: drop speculative diagnostics from overload argument
        // collection and keep only overload-level diagnostics.
        // Roll back diagnostics and TS2454 state to the pre-overload snapshot
        // so the fallback path can re-evaluate cleanly.
        self.ctx
            .rollback_diagnostics_filtered(&overload_snap.diag, |diag| {
                Self::should_preserve_speculative_call_diagnostic(diag)
            });
        self.ctx
            .restore_ts2454_state(&overload_snap.emitted_ts2454_errors);

        // Restore original state if no overload matched
        self.ctx.node_types = original_node_types;
        if all_arg_count_mismatches && !failures.is_empty() {
            if !any_has_rest
                && !exact_expected_counts.is_empty()
                && !exact_expected_counts.contains(&args.len())
            {
                let mut lower = None;
                let mut upper = None;
                for &count in &exact_expected_counts {
                    if count < args.len() {
                        lower = Some(lower.map_or(count, |prev: usize| prev.max(count)));
                    } else if count > args.len() {
                        upper = Some(upper.map_or(count, |prev: usize| prev.min(count)));
                    }
                }
                if let (Some(expected_low), Some(expected_high)) = (lower, upper) {
                    return Some(OverloadResolution {
                        arg_types,
                        result: CallResult::OverloadArgumentCountMismatch {
                            actual: args.len(),
                            expected_low,
                            expected_high,
                        },
                    });
                }
            }

            return Some(OverloadResolution {
                arg_types,
                result: CallResult::ArgumentCountMismatch {
                    expected_min: min_expected,
                    expected_max: if any_has_rest {
                        None
                    } else if max_expected > min_expected {
                        Some(max_expected)
                    } else {
                        Some(min_expected)
                    },
                    actual: args.len(),
                },
            });
        }

        Some(OverloadResolution {
            arg_types: arg_types.clone(),
            result: CallResult::NoOverloadMatch {
                func_type: TypeId::ANY,
                arg_types,
                failures,
                fallback_return: signatures
                    .first()
                    .map(|sig| sig.return_type)
                    .unwrap_or(TypeId::ANY),
            },
        })
    }
}
