//! Call argument elaboration logic (object literal, array literal, function return).

use crate::context::TypingRequest;
use crate::context::speculation::FullSpeculationSnapshot;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common as query_common;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[path = "elaboration_object_literal_completeness.rs"]
mod elaboration_object_literal_completeness;
#[path = "elaboration_object_properties.rs"]
mod elaboration_object_properties;

impl<'a> CheckerState<'a> {
    fn present_callable_property_target_display_type(&self, target_type: TypeId) -> TypeId {
        let stripped =
            crate::query_boundaries::common::remove_undefined(self.ctx.types, target_type);
        if stripped != target_type && self.stripped_property_context_is_callable(stripped) {
            stripped
        } else {
            target_type
        }
    }

    pub(in crate::error_reporter::call_errors) fn contextual_keyof_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                for &candidate_arg in &args.nodes {
                    if candidate_arg == arg_idx {
                        continue;
                    }
                    let candidate_type = self.get_type_of_node(candidate_arg);
                    if candidate_type == TypeId::ERROR || candidate_type == TypeId::ANY {
                        continue;
                    }

                    let candidate_keyof =
                        self.evaluate_type_for_assignability(self.ctx.types.keyof(candidate_type));
                    if candidate_keyof == TypeId::ERROR {
                        continue;
                    }
                    // `keyof null`, `keyof undefined`, and `keyof void` all
                    // reduce to `never`. tsc displays the
                    // reduced form; falling back to "keyof null" loses
                    // fingerprint parity (unknownControlFlow.ts ff1).
                    if candidate_keyof == TypeId::NEVER {
                        continue;
                    }

                    let same_key_space = self.contextual_keyof_parameter_types_share_key_space(
                        param_type,
                        candidate_keyof,
                    );
                    if same_key_space
                        && query_common::type_has_displayable_name(
                            self.ctx.types.as_type_database(),
                            candidate_type,
                        )
                    {
                        let base = self.format_type_for_assignability_message(candidate_type);
                        return Some(format!("keyof {base}"));
                    }
                }
                break;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn contextual_keyof_parameter_types_share_key_space(
        &mut self,
        param_type: TypeId,
        candidate_keyof: TypeId,
    ) -> bool {
        if self.types_are_mutually_assignable(param_type, candidate_keyof) {
            return true;
        }

        self.ctx
            .types
            .get_display_alias(param_type)
            .is_some_and(|alias| alias == candidate_keyof)
    }

    fn types_are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.call_elaboration_mutual_relation_outcome(left, right)
            .related
            && self
                .call_elaboration_mutual_relation_outcome(right, left)
                .related
    }

    pub(in crate::error_reporter::call_errors) fn contextual_constraint_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let evaluated_param = self.evaluate_type_for_assignability(param_type);
        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                let arg_pos = args
                    .nodes
                    .iter()
                    .position(|&candidate| candidate == arg_idx)?;
                let callee_type = self.get_type_of_node(call.expression);
                let arg_count = args.nodes.len();

                let mut display = None;
                let mut ambiguous = false;

                if let Some(shape) = diagnostic_query::function_shape(self.ctx.types, callee_type) {
                    let sig = diagnostic_query::call_signature_from_function_shape_for_display(
                        shape.as_ref(),
                    );
                    if self.call_signature_accepts_arg_count(&sig, arg_count) {
                        self.collect_constraint_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                    }
                }

                if let Some(signatures) = crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    for sig in signatures {
                        if !self.call_signature_accepts_arg_count(&sig, arg_count) {
                            continue;
                        }
                        self.collect_constraint_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                        if ambiguous {
                            break;
                        }
                    }
                }

                return (!ambiguous).then_some(display).flatten();
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        None
    }

    pub(in crate::error_reporter::call_errors) fn contextual_generic_mapped_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let evaluated_arg = self.evaluate_type_for_assignability(arg_type);
        let arg_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated_arg)?;
        let unknown_object =
            diagnostic_query::object_type_with_unknown_display_members(self.ctx.types, &arg_shape)?;

        let evaluated_param = self.evaluate_type_for_assignability(param_type);
        let mut current = arg_idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.ctx.arena.get_call_expr(node)
                && let Some(args) = &call.arguments
            {
                let arg_pos = args
                    .nodes
                    .iter()
                    .position(|&candidate| candidate == arg_idx)?;
                let callee_type = self.get_type_of_node(call.expression);
                let arg_count = args.nodes.len();

                let mut display = None;
                let mut ambiguous = false;

                if let Some(shape) = diagnostic_query::function_shape(self.ctx.types, callee_type) {
                    let sig = diagnostic_query::call_signature_from_function_shape_for_display(
                        shape.as_ref(),
                    );
                    if self.call_signature_accepts_arg_count(&sig, arg_count) {
                        self.collect_generic_mapped_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            unknown_object,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                    }
                }

                if let Some(signatures) = crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    for sig in signatures {
                        if !self.call_signature_accepts_arg_count(&sig, arg_count) {
                            continue;
                        }
                        self.collect_generic_mapped_parameter_display_candidate(
                            &sig,
                            arg_pos,
                            unknown_object,
                            evaluated_param,
                            &mut display,
                            &mut ambiguous,
                        );
                        if ambiguous {
                            break;
                        }
                    }
                }

                return (!ambiguous).then_some(display).flatten();
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        None
    }

    fn collect_generic_mapped_parameter_display_candidate(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_pos: usize,
        unknown_object: TypeId,
        evaluated_param: TypeId,
        display: &mut Option<String>,
        ambiguous: &mut bool,
    ) {
        if *ambiguous || sig.type_params.is_empty() {
            return;
        }
        let Some(raw_param) = self.raw_param_for_argument_index(sig, arg_pos) else {
            return;
        };
        let mut from_type_param_constraint = false;
        let candidate_source_type =
            if query_common::type_application(self.ctx.types, raw_param.type_id).is_some() {
                raw_param.type_id
            } else if let Some(type_param) =
                query_common::type_param_info(self.ctx.types, raw_param.type_id)
                && let Some(constraint) = type_param.constraint
            {
                from_type_param_constraint = true;
                constraint
            } else {
                return;
            };

        let mut substitution = query_common::TypeSubstitution::new();
        for tp in &sig.type_params {
            substitution.insert(tp.name, unknown_object);
        }
        if substitution.is_empty() {
            return;
        }

        let candidate =
            query_common::instantiate_type(self.ctx.types, candidate_source_type, &substitution);
        let evaluated_candidate = self.evaluate_type_for_assignability(candidate);
        let matches_evaluated = evaluated_candidate == evaluated_param
            || self.types_are_mutually_assignable(evaluated_candidate, evaluated_param);
        if !(matches_evaluated
            || from_type_param_constraint
                && query_common::object_shape_for_type(self.ctx.types, evaluated_candidate)
                    .is_some())
        {
            return;
        }

        let candidate_display = if evaluated_candidate != candidate
            && evaluated_candidate != TypeId::ERROR
            && !query_common::contains_type_parameters(self.ctx.types, evaluated_candidate)
        {
            self.format_type_for_assignability_message(evaluated_candidate)
        } else {
            self.format_type_diagnostic(candidate)
        };
        if display
            .as_ref()
            .is_some_and(|existing| existing != &candidate_display)
        {
            *ambiguous = true;
            return;
        }
        *display = Some(candidate_display);
    }

    fn collect_constraint_parameter_display_candidate(
        &mut self,
        sig: &tsz_solver::CallSignature,
        arg_pos: usize,
        evaluated_param: TypeId,
        display: &mut Option<String>,
        ambiguous: &mut bool,
    ) {
        if *ambiguous {
            return;
        }

        let Some(raw_param) = self.raw_param_for_argument_index(sig, arg_pos) else {
            return;
        };
        let Some(type_param) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, raw_param.type_id)
        else {
            return;
        };
        let Some(raw_constraint) = type_param.constraint else {
            return;
        };

        let evaluated_constraint = self.evaluate_type_for_assignability(raw_constraint);
        let matches_evaluated = evaluated_constraint == evaluated_param
            || self.types_are_mutually_assignable(evaluated_constraint, evaluated_param);
        if !matches_evaluated {
            return;
        }

        let evaluated_number_literal_union = if let Some(members) =
            query_common::union_members(self.ctx.types, evaluated_constraint)
        {
            !members.is_empty()
                && members.iter().all(|&member| {
                    matches!(
                        query_common::literal_value(self.ctx.types, member),
                        Some(query_common::LiteralValue::Number(_))
                    )
                })
        } else {
            matches!(
                query_common::literal_value(self.ctx.types, evaluated_constraint),
                Some(query_common::LiteralValue::Number(_))
            )
        };
        let candidate_display_type = if evaluated_constraint != raw_constraint
            && evaluated_constraint != TypeId::ERROR
            && (evaluated_number_literal_union
                || !query_common::contains_type_parameters(self.ctx.types, evaluated_constraint))
        {
            evaluated_constraint
        } else {
            raw_constraint
        };
        let candidate = self.format_type_for_assignability_message(candidate_display_type);
        if display
            .as_ref()
            .is_some_and(|existing| existing != &candidate)
        {
            *ambiguous = true;
            return;
        }
        *display = Some(candidate);
    }

    /// Try to elaborate a generic assignability mismatch when the source expression is
    /// a literal that can be decomposed into more precise element/property errors.
    ///
    /// Indirect callers reaching this entry from object-literal property values
    /// or array element values inside a generic call argument should use
    /// [`try_elaborate_assignment_source_error_in_call_arg`] instead, so the
    /// arrow/function-expression interception below stays inside the
    /// unresolved-holes guard.
    pub(crate) fn try_elaborate_assignment_source_error(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        self.try_elaborate_assignment_source_error_with_options(
            source_idx,
            target_type,
            /* allow_unresolved_holes */ true,
        )
    }

    /// Variant for indirect callers (object-literal property values, array
    /// element values) where `target_type` may still contain inference holes
    /// belonging to an enclosing generic call. Skips the arrow/function-expr
    /// interception that would otherwise produce false TS2322s by elaborating
    /// against an uninstantiated parameter.
    pub(crate) fn try_elaborate_assignment_source_error_in_call_arg(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        self.try_elaborate_assignment_source_error_with_options(
            source_idx,
            target_type,
            /* allow_unresolved_holes */ false,
        )
    }

    pub(crate) fn try_elaborate_callback_body_diagnostics(
        &mut self,
        arg_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        thread_local! {
            static CALLBACK_BODY_ELABORATION_DEPTH: std::cell::Cell<u32> =
                const { std::cell::Cell::new(0) };
        }

        struct DepthReset;
        impl Drop for DepthReset {
            fn drop(&mut self) {
                CALLBACK_BODY_ELABORATION_DEPTH.with(|depth| {
                    depth.set(depth.get().saturating_sub(1));
                });
            }
        }

        if CALLBACK_BODY_ELABORATION_DEPTH.with(|depth| {
            if depth.get() > 0 {
                true
            } else {
                depth.set(1);
                false
            }
        }) {
            return false;
        }
        let _depth_reset = DepthReset;

        if !self.arg_is_callback_with_unannotated_params(arg_idx) {
            return false;
        }

        let Some(callback_idx) = self.callback_function_index(arg_idx) else {
            return false;
        };
        let Some(callback_node) = self.ctx.arena.get(callback_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(callback_node) else {
            return false;
        };
        let Some(body_node) = self.ctx.arena.get(func.body) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::BLOCK {
            return false;
        }

        let body_spans = self.callback_body_spans(arg_idx);
        if body_spans.is_empty() {
            return false;
        }

        let snap = FullSpeculationSnapshot::new(&self.ctx);
        self.invalidate_expression_for_contextual_retry(arg_idx);
        self.ctx.daa_error_nodes.remove(&arg_idx.0);
        self.ctx.flow_narrowed_nodes.remove(&arg_idx.0);

        let diag_snap = self.ctx.snapshot_diagnostics();
        let request = TypingRequest::with_contextual_type(target_type);
        let _ = self.get_type_of_node_with_request(arg_idx, &request);
        let diagnostics: Vec<_> = self
            .ctx
            .speculative_diagnostics_since(&diag_snap)
            .iter()
            .filter(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        | diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                        | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                        | diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                ) && body_spans
                    .iter()
                    .any(|(start, end)| diag.start >= *start && diag.start < *end)
            })
            .cloned()
            .collect();

        snap.rollback(&mut self.ctx.speculation_state());

        if diagnostics.is_empty() {
            return false;
        }

        for diag in diagnostics {
            if !self.ctx.diagnostics.iter().any(|existing| {
                existing.code == diag.code
                    && existing.start == diag.start
                    && existing.length == diag.length
                    && existing.message_text == diag.message_text
            }) {
                self.ctx.push_diagnostic(diag);
            }
        }
        true
    }

    fn try_elaborate_assignment_source_error_with_options(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
        allow_unresolved_holes: bool,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // A plain `expr as T` / `<T>expr` assertion (not `as const`, not
        // `satisfies`) yields the asserted, *non-fresh* type `T`. Per-property /
        // excess elaboration applies only to fresh object/array literals, so
        // descending into the assertion operand below (`skip_parenthesized_and_assertions`
        // strips the assertion and re-derives a fresh inner-literal type) would
        // manufacture diagnostics `tsc` never reports: TS2353 instead of the
        // weak-type TS2559, or a per-property TS2322 instead of the argument-level
        // TS2345 with its structural chain. Returning `false` defers to the
        // caller's argument/assignment-level report. `satisfies` and `as const`
        // preserve freshness (excluded by the predicate) and still elaborate.
        // Matches `tsc`'s `getRegularTypeOfObjectLiteral` / `elaborateError`
        // boundary (the latter descends through parens but not assertions).
        if self.expression_is_plain_type_assertion(source_idx) {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        if let Some(node) = self.ctx.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let source_type = self
                .ctx
                .node_types
                .get(&expr_idx.0)
                .copied()
                .unwrap_or_else(|| self.elaboration_source_expression_type(expr_idx));
            if query_common::is_remapped_mapped_index_access(self.ctx.types, source_type) {
                return false;
            }
        }

        if let Some(node) = self.ctx.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && self.assignment_source_is_return_expression(source_idx)
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            let mut elaborated = false;

            for branch_idx in [cond.when_true, cond.when_false] {
                let branch_idx = self.ctx.arena.skip_parenthesized_and_assertions(branch_idx);
                let branch_type = self.get_type_of_node(branch_idx);
                if branch_type == TypeId::ERROR
                    || branch_type == TypeId::ANY
                    || target_type == TypeId::ERROR
                    || target_type == TypeId::ANY
                    || self
                        .return_relation_outcome(branch_type, target_type)
                        .related
                {
                    continue;
                }

                if self.try_elaborate_assignment_source_error_with_options(
                    branch_idx,
                    target_type,
                    allow_unresolved_holes,
                ) {
                    elaborated = true;
                    continue;
                }

                self.error_type_not_assignable_at_with_anchor(branch_type, target_type, branch_idx);
                elaborated = true;
            }

            return elaborated;
        }

        // Direct assignment to a function-type target: take the dedicated path that
        // permits return-expression elaboration even when the expected return type
        // contains a type parameter from the *target's own* generic signature.
        // Unlike the call-argument path (which sees uninstantiated type parameters
        // belonging to the enclosing call's inference state), the target type here
        // is the final declared type, so a free `T` in the return position is
        // genuinely unsatisfied by a concrete body type.
        if let Some(arg_node) = self.ctx.arena.get(expr_idx)
            && (arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
            && allow_unresolved_holes
        {
            return self.try_elaborate_function_arg_return_error_with_options(
                expr_idx,
                target_type,
                /* allow_unresolved_holes */ true,
            );
        }

        self.try_elaborate_object_literal_arg_error(expr_idx, target_type)
    }

    /// Try to elaborate an argument type mismatch for object/array literal arguments.
    ///
    /// When an object literal argument has a property whose value type doesn't match
    /// the expected property type, tsc reports TS2322 on the specific property name
    /// rather than TS2345 on the whole argument. Similarly for array literals, tsc
    /// reports TS2322 on each element that doesn't match the expected element type.
    ///
    /// Returns `true` if elaboration produced at least one property-level error (TS2322),
    /// meaning the caller should NOT emit TS2345 on the whole argument.
    pub fn try_elaborate_object_literal_arg_error(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        self.try_elaborate_object_literal_arg_error_with_source(arg_idx, param_type, None)
    }

    pub(crate) fn try_emit_polymorphic_this_object_literal_arg_errors(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        let arg_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(arg_node).cloned() else {
            return false;
        };

        let candidates = [
            param_type,
            self.evaluate_contextual_type(param_type),
            self.evaluate_type_with_env(param_type),
            self.resolve_type_for_property_access(param_type),
            self.evaluate_type_for_assignability(param_type),
        ];

        let mut emitted = false;
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    match self.ctx.arena.get_method_decl(elem_node) {
                        Some(method) => (method.name, elem_idx),
                        None => continue,
                    }
                }
                _ => continue,
            };

            let is_computed_property = self
                .ctx
                .arena
                .get(prop_name_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let Some(prop_name) = self
                .object_literal_property_name_text(prop_name_idx)
                .or_else(|| {
                    is_computed_property
                        .then(|| self.get_property_name_resolved(prop_name_idx))
                        .flatten()
                })
            else {
                continue;
            };

            let source_prop_type = self.get_type_of_node(prop_value_idx);
            if source_prop_type == TypeId::ERROR || source_prop_type == TypeId::ANY {
                continue;
            }

            for candidate in candidates {
                let Some((target_prop_type, _)) =
                    self.object_literal_target_property_type(candidate, prop_name_idx, &prop_name)
                else {
                    continue;
                };
                if target_prop_type == TypeId::ERROR || target_prop_type == TypeId::ANY {
                    continue;
                }
                if self
                    .call_arg_relation_outcome(source_prop_type, target_prop_type)
                    .related
                    && self.emit_polymorphic_this_property_assignment_error(
                        source_prop_type,
                        target_prop_type,
                        prop_name_idx,
                    )
                {
                    emitted = true;
                    break;
                }
            }
        }
        emitted
    }

    /// Like `try_elaborate_object_literal_arg_error`, but accepts an optional
    /// `source_type_override` for cases where `get_type_of_node` returns a
    /// contextually-typed version that doesn't reflect the actual mismatch
    /// (e.g., method declarations in object literals passed as generic call arguments).
    pub fn try_elaborate_object_literal_arg_error_with_source(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
        source_type_override: Option<TypeId>,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        match arg_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => self
                .try_elaborate_object_literal_properties_with_source(
                    arg_idx,
                    param_type,
                    source_type_override,
                ),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if self.try_elaborate_array_literal_elements(arg_idx, param_type) {
                    true
                } else {
                    let source_type = source_type_override
                        .unwrap_or_else(|| self.elaboration_source_expression_type(arg_idx));
                    self.try_elaborate_array_literal_mismatch_from_failure_reason(
                        arg_idx,
                        source_type,
                        param_type,
                    )
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.try_elaborate_function_arg_return_error(arg_idx, param_type)
            }
            _ => false,
        }
    }

    /// Mirror `tsc`'s `elaborateArrowFunction` gate: the elaborator only drills
    /// into a function-expression body's return when **no** parameter carries an
    /// explicit type annotation (`some(node.parameters, hasType)` makes
    /// `elaborateArrowFunction` return `false`). When any parameter is annotated
    /// — even with a type that matches the contextual parameter — the mismatch is
    /// reported at the function-type level (the parameter-contravariance frame,
    /// e.g. `Type '(x: string) => number' is not assignable to type
    /// '(x: number) => string'`) instead of being anchored at the body return
    /// expression. This keeps object-literal property and argument arrows in
    /// parity with `tsc`'s diagnostic anchor and message.
    ///
    /// `func_value_idx` is the function-expression / arrow / method node.
    ///
    /// This is deliberately narrower than
    /// `function_like_has_explicit_signature_annotations` (which also reports a
    /// standalone explicit *return* type annotation): `tsc`'s
    /// `elaborateArrowFunction` gate keys on parameters only, so an arrow with a
    /// return annotation but no parameter annotation (`(x): string => …`) is
    /// still elaborated into its body.
    pub(crate) fn function_value_has_explicit_param_annotation(
        &self,
        func_value_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(func_value_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        func.parameters.nodes.iter().any(|param_idx| {
            self.ctx
                .arena
                .get(*param_idx)
                .and_then(|n| self.ctx.arena.get_parameter(n))
                .is_some_and(|p| p.type_annotation.is_some())
        })
    }

    fn try_elaborate_function_arg_return_error(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        self.try_elaborate_function_arg_return_error_with_options(
            arg_idx, param_type, /* allow_unresolved_holes */ false,
        )
    }

    /// Like [`try_elaborate_function_arg_return_error`], but with a switch that
    /// controls whether elaboration runs when the expected return type contains
    /// unresolved type parameters / inference placeholders.
    ///
    /// `allow_unresolved_holes = false` (default for call-argument paths):
    ///     skip elaboration. During generic call inference the expected return
    ///     type can still reference uninstantiated type parameters from the
    ///     enclosing call (e.g., `B` from `compose<A, B, C>`); checking a
    ///     concrete body type against such placeholders produces false TS2322s.
    ///
    /// `allow_unresolved_holes = true` (used by direct assignment):
    ///     proceed with elaboration. The target type is the *final* declared
    ///     target (e.g., the variable's annotation), so any type parameter in
    ///     the return position is bound by the target's own generic signature
    ///     rather than an outer inference state. A free `T` here is genuinely
    ///     unsatisfied by a concrete body type, and tsc anchors the resulting
    ///     TS2322 at the body expression.
    fn try_elaborate_function_arg_return_error_with_options(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
        allow_unresolved_holes: bool,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(arg_node) else {
            return false;
        };

        let Some(expected_return_type) = self.union_callable_return_type(param_type) else {
            return false;
        };

        // When the target is a callable type with additional properties (e.g.,
        // `ArrayConstructor` with `isArray`, `from`, `of`), the primary failure
        // is missing properties (TS2739), not return type mismatch (TS2322).
        // Skip function body elaboration so the standard `diagnose_assignment_failure`
        // path produces TS2739 instead. tsc does the same: it reports missing
        // properties on the callable, not return type mismatches on the function body.
        if let Some(callable) = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types.as_type_database(),
            param_type,
        ) && !callable.properties.is_empty()
        {
            return false;
        }

        // For generator function callbacks, the callable return type is
        // Generator<Y, R, N> or AsyncGenerator<Y, R, N>, but the body's
        // `return` statements produce TReturn (R), not the full Generator type.
        // Elaborating return statements against the full Generator type produces
        // false TS2322 errors (e.g., "Type 'number' is not assignable to type
        // 'Generator<0, 0, 1>'"). Skip callback return elaboration for
        // generators — the body's return type checking is already handled
        // correctly in check_return_statement with the unwrapped TReturn type.
        if func.asterisk_token {
            return false;
        }

        // Skip elaboration when the expected return type contains unresolved
        // type parameters or inference placeholders. During generic call
        // inference, the expected callback return type may still reference
        // uninstantiated type parameters (e.g., `B` from `compose<A, B, C>`).
        // Checking the body expression type against such placeholders would
        // produce false TS2322 errors since concrete types like `T[]` are
        // not assignable to an unresolved type parameter `B`.
        //
        // For direct variable-initializer / direct-assignment elaboration
        // (`allow_unresolved_holes = true`), the target type is final and any
        // remaining type parameters are bound by the target's own quantifier,
        // so the elaboration is sound and matches tsc. However, even in the
        // `allow_unresolved_holes = true` path, if the callable target has NO
        // own generic type parameters, the unresolved type parameters must
        // come from an outer inference context (e.g., `B` in `(...args: A) => B`
        // from `pipe<A,B,C>`). In that case, elaboration produces false TS2322
        // errors and should be skipped.
        if self.type_has_unresolved_inference_holes(expected_return_type) {
            let should_skip = if !allow_unresolved_holes {
                true
            } else {
                // allow_unresolved_holes=true (direct-assignment context): skip
                // when the callable has no own type params in its signatures,
                // meaning the holes come from an outer generic context.
                let callable_has_own_type_params =
                    crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types.as_type_database(),
                        param_type,
                    )
                    .map(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| !sig.type_params.is_empty())
                    })
                    .unwrap_or(false)
                        || crate::query_boundaries::common::function_shape_for_type(
                            self.ctx.types,
                            param_type,
                        )
                        .is_some_and(|shape| !shape.type_params.is_empty());
                !callable_has_own_type_params
            };
            if should_skip {
                return false;
            }
        }

        let Some(body_node) = self.ctx.arena.get(func.body) else {
            return false;
        };

        // Async call-argument callbacks are checked through the async-return
        // path, where the body expression is awaited before comparison. This
        // generic call-argument elaborator only sees the callable return type;
        // drilling into an async expression body there can turn valid
        // `T | PromiseLike<T>` contexts into false synchronous TS2322s.
        //
        // Direct assignment/JSDoc contexts still need expression-body
        // elaboration so `async () => 0` assigned to `function(): string`
        // reports at the returned `0` like `tsc`. Keep block bodies out of this
        // path because `tsc` does not drill into those async JSDoc returns.
        if func.is_async && (!allow_unresolved_holes || body_node.kind == syntax_kind_ext::BLOCK) {
            return false;
        }

        match body_node.kind {
            // Expression-bodied arrow function: () => ({ ... })
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.try_elaborate_object_literal_arg_error(func.body, expected_return_type)
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::BINARY_EXPRESSION =>
            {
                // For expression-bodied arrows with simple literal/expression bodies,
                // report TS2322 on the return expression when its type violates the
                // expected return type (e.g., returning a string where Function is
                // expected in a property assignment context).
                self.elaborate_expression_body_return_mismatch(
                    arg_idx,
                    func.body,
                    expected_return_type,
                    param_type,
                )
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                // Expression-bodied arrow whose body is a conditional
                // (`() => cond ? a : b`). `tsc`'s `elaborateArrowFunction`
                // elaborates the body expression; for a conditional it does not
                // recurse per-branch but anchors the single TS2322 at the whole
                // conditional (source = the conditional's union type). Mirror the
                // simple-expression path so a call-argument callback drills to the
                // conditional instead of the coarse whole-argument TS2345.
                self.elaborate_expression_body_return_mismatch(
                    arg_idx,
                    func.body,
                    expected_return_type,
                    param_type,
                )
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Drill through the parenthesis and route the inner expression
                // the same way an unparenthesized body would. An object/array
                // literal keeps its per-property/element elaboration anchored at
                // the inner literal (`tsc`'s `elaborateError` skips the parens for
                // those). Any other body (conditional, simple return expression)
                // anchors the mismatch at the whole parenthesized expression —
                // `tsc` reports there, so pass the paren node (its type is
                // transparent to the inner) rather than the inner expression.
                let Some(paren) = self.ctx.arena.get_parenthesized(body_node) else {
                    return false;
                };
                let inner = paren.expression;
                let inner_kind = self.ctx.arena.get(inner).map(|node| node.kind);
                match inner_kind {
                    Some(k)
                        if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
                    {
                        self.try_elaborate_object_literal_arg_error(inner, expected_return_type)
                    }
                    _ => self.elaborate_expression_body_return_mismatch(
                        arg_idx,
                        func.body,
                        expected_return_type,
                        param_type,
                    ),
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                // Pass param_type for proper error message display
                self.try_elaborate_function_block_returns_with_param_type(
                    func.body,
                    expected_return_type,
                    param_type,
                    arg_idx,
                )
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                // Expression-bodied arrow: () => new Animal()
                // When the new-expression type isn't assignable to the expected
                // return type (e.g. Animal missing 'woof' required by Dog),
                // emit the assignability error at the expression position.
                // This matches tsc which emits TS2741 at `new Animal()` instead
                // of TS2345 on the whole callback.
                //
                // Use Exact anchor to prevent RewriteAssignment from walking up
                // to the parent arrow function. Without this, the diagnostic
                // anchor becomes the arrow function node, causing the source type
                // to be displayed as the function type (e.g., `() => Animal`)
                // instead of the body expression type (`Animal`), and preventing
                // the solver from producing the specific MissingProperty failure
                // reason needed for TS2741.
                let body_type = self.get_type_of_node(func.body);
                if body_type == TypeId::ERROR
                    || body_type == TypeId::ANY
                    || expected_return_type == TypeId::ERROR
                    || expected_return_type == TypeId::ANY
                    || self
                        .return_relation_outcome(body_type, expected_return_type)
                        .related
                {
                    return false;
                }
                // Evaluate the expected return type to strip type wrappers like
                // NoInfer<T> → T for display purposes. tsc displays `Dog` not
                // `NoInfer<Dog>` in TS2741 messages because it evaluates the type
                // before rendering the diagnostic.
                let display_target = self.evaluate_type_with_env(expected_return_type);
                self.error_type_not_assignable_at_with_anchor(body_type, display_target, func.body);
                true
            }
            _ => false,
        }
    }

    /// Anchor a return-type mismatch at an expression-bodied arrow's body
    /// expression (`body_idx`) as `tsc`'s `elaborateArrowFunction` does, rather
    /// than reporting the coarse whole-argument TS2345. Shared by the
    /// simple-expression and conditional-expression body arms.
    ///
    /// Returns `true` (suppressing the caller's TS2345) only when a genuine
    /// mismatch is emitted; returns `false` — deferring to the argument-level
    /// diagnostic — for `void` expected returns, explicitly-annotated callback
    /// parameters (the `elaborateArrowFunction` gate keys on parameters), an
    /// `error`/`any` on either side, or an already-related pair. A body whose
    /// type is itself callable drills like any other (e.g. `const f: () => number
    /// = () => g` where `g: () => string` reports `Type '() => string' is not
    /// assignable to type 'number'.` at `g`, not the whole arrow) — *except* when
    /// the arrow's parameter arity also differs from the target callback, in
    /// which case `tsc` keeps the whole-callback TS2345 (`foo(() => g)` against
    /// `(x: string) => string`, parser536727).
    fn elaborate_expression_body_return_mismatch(
        &mut self,
        arg_idx: NodeIndex,
        body_idx: NodeIndex,
        expected_return_type: TypeId,
        expected_callback_type: TypeId,
    ) -> bool {
        if expected_return_type == TypeId::VOID {
            return false;
        }
        if self.function_value_has_explicit_param_annotation(arg_idx) {
            return false;
        }
        let body_type = self.get_type_of_node(body_idx);
        if body_type == TypeId::ERROR
            || body_type == TypeId::ANY
            || expected_return_type == TypeId::ERROR
            || expected_return_type == TypeId::ANY
            || self
                .return_relation_outcome(body_type, expected_return_type)
                .related
        {
            return false;
        }
        // Narrowed callable-body carve-out. `tsc` keeps the whole-callback
        // `TS2345` (no body drill) when the body's own type is callable, the
        // expected return is not, AND the arrow's parameter arity does not match
        // the target callback — the callback then mismatches in more than just
        // its return. `foo(() => g)` against `(x: string) => string` (callable
        // body, `string` return, 0 vs 1 params) stays argument-level
        // (parser536727), while `run(() => producer)` against `() => number`
        // (0 vs 0 params) still drills to the body. A non-callable body (an
        // object/optional-property source) is never exempted here.
        if self.first_callable_return_type(body_type).is_some()
            && self
                .first_callable_return_type(expected_return_type)
                .is_none()
            && !self.arrow_param_arity_matches_expected_callback(arg_idx, expected_callback_type)
        {
            return false;
        }
        // tsc anchors expression-body arrow return mismatches at the body
        // expression, not the arrow function. E.g.:
        //   `const f: (a: number) => string = (a) => a + 1`
        //   → TS2322 at `a + 1` with "Type 'number' is not assignable to type 'string'."
        let display_target = self.evaluate_type_with_env(expected_return_type);
        if self.array_elaboration_widening_required_for_display(body_type, display_target) {
            self.error_type_not_assignable_at_with_widened_source_display(
                body_type,
                display_target,
                body_idx,
            );
        } else {
            self.error_type_not_assignable_at_with_anchor(body_type, display_target, body_idx);
        }
        true
    }

    /// `true` when the arrow at `arg_idx` has the same parameter count as the
    /// first call signature of `expected_callback_type`. Used to gate the
    /// expression-body return drill: `tsc`'s `elaborateArrowFunction` anchors the
    /// return mismatch at the body only when the arrow's parameters line up with
    /// the target callback; a param-arity difference keeps the whole-callback
    /// `TS2345`. Defaults to `true` (no exemption) when either side has no
    /// comparable signature, preserving the prior drill behavior.
    fn arrow_param_arity_matches_expected_callback(
        &mut self,
        arg_idx: NodeIndex,
        expected_callback_type: TypeId,
    ) -> bool {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return true;
        };
        let Some(func) = self.ctx.arena.get_function(arg_node) else {
            return true;
        };
        let arrow_param_count = func.parameters.nodes.len();
        match self.first_callable_param_count(expected_callback_type) {
            Some(expected) => arrow_param_count == expected,
            None => true,
        }
    }

    /// Parameter count of the first call signature of `ty`, resolved the same way
    /// [`Self::first_callable_return_type`] resolves the return type (function
    /// shape, then call signatures, then callable shape, unwrapping nullish
    /// unions and applications). `None` when `ty` has no comparable signature.
    fn first_callable_param_count(&mut self, ty: TypeId) -> Option<usize> {
        use crate::query_boundaries::diagnostics::{
            callable_shape_for_type, function_shape, type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.first_callable_param_count(non_nullish);
        }
        if let Some(shape) = function_shape(self.ctx.types, ty) {
            return Some(shape.params.len());
        }
        if let Some(signatures) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, ty)
        {
            return signatures.first().map(|sig| sig.params.len());
        }
        if let Some(shape) = callable_shape_for_type(self.ctx.types, ty) {
            return shape.call_signatures.first().map(|sig| sig.params.len());
        }
        if let Some(app) = type_application(self.ctx.types, ty) {
            return self.first_callable_param_count(app.base);
        }
        None
    }

    fn try_elaborate_function_block_returns_with_param_type(
        &mut self,
        block_idx: NodeIndex,
        expected_return_type: TypeId,
        param_type: TypeId,
        func_idx: NodeIndex,
    ) -> bool {
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };

        let mut elaborated = false;
        for &stmt_idx in &block.statements.nodes {
            elaborated |= self.try_elaborate_return_statements_in_stmt_with_param_type(
                stmt_idx,
                expected_return_type,
                param_type,
                func_idx,
            );
        }
        elaborated
    }

    fn try_elaborate_return_statements_in_stmt_with_param_type(
        &mut self,
        stmt_idx: NodeIndex,
        expected_return_type: TypeId,
        param_type: TypeId,
        func_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.ctx.arena.get_return_statement(node) else {
                    return false;
                };
                if ret.expression.is_none() {
                    return false;
                }
                if expected_return_type == TypeId::VOID {
                    return false;
                }

                let return_type = self.get_type_of_node(ret.expression);
                // When we have a valid function index, use full function types for error display
                if func_idx.0 != 0 {
                    let func_type = self.get_type_of_node(func_idx);
                    // Widen the function type for display to match tsc behavior
                    // (e.g., show `() => string` instead of `() => "foo"`)
                    let widened_func_type =
                        crate::query_boundaries::common::widen_type_deep(self.ctx.types, func_type);
                    // `tsc`'s `elaborateArrowFunction` never drills into a block
                    // body, so this function-level mismatch is anchored at the
                    // function value's *binding target*, not the function node:
                    // the assignment LHS (`A.prototype.foo = function() {}`), the
                    // variable declaration's name (`const f: () => number = () =>
                    // { return "x"; }`), or the enclosing `return` statement.
                    // Fall back to the function position only when the value is
                    // in a context where that anchor already matches `tsc` (call
                    // argument, object-literal property, …).
                    //
                    // For the variable-initializer / return-statement binding
                    // targets, the report must also stay at the binding without
                    // drilling into the source shape: `tsc` reports the single
                    // function-level mismatch even when the returned value is an
                    // object literal (`() => { return { a: "x" }; }`), which the
                    // default source-elaboration path would otherwise drill into.
                    if let Some(binding_anchor) = self.function_value_binding_anchor(func_idx) {
                        !self
                            .check_assignable_or_report_at_exact_anchor_without_source_elaboration_with_display_types(
                                return_type,
                                expected_return_type,
                                widened_func_type,
                                param_type,
                                ret.expression,
                                binding_anchor,
                            )
                    } else {
                        let diag_anchor = if self.is_rhs_of_assignment(func_idx) {
                            self.find_assignment_lhs_for_rhs(func_idx)
                                .unwrap_or(func_idx)
                        } else {
                            func_idx
                        };
                        !self.check_assignable_or_report_at_with_display_types(
                            return_type,
                            expected_return_type,
                            widened_func_type,
                            param_type,
                            ret.expression,
                            diag_anchor, // Use appropriate anchor based on context
                        )
                    }
                } else {
                    !self.check_assignable_or_report_at_without_source_elaboration(
                        return_type,
                        expected_return_type,
                        ret.expression,
                        ret.expression,
                    )
                }
            }
            syntax_kind_ext::BLOCK => self.try_elaborate_function_block_returns_with_param_type(
                stmt_idx,
                expected_return_type,
                param_type,
                func_idx,
            ),
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(node) else {
                    return false;
                };
                let mut elaborated = self.try_elaborate_return_statements_in_stmt_with_param_type(
                    if_stmt.then_statement,
                    expected_return_type,
                    param_type,
                    func_idx,
                );
                if if_stmt.else_statement.is_some() {
                    elaborated |= self.try_elaborate_return_statements_in_stmt_with_param_type(
                        if_stmt.else_statement,
                        expected_return_type,
                        param_type,
                        func_idx,
                    );
                }
                elaborated
            }
            _ => false,
        }
    }

    /// Collect the return types of every call signature reachable from `ty`,
    /// resolving through nullish unwrapping, a bare function shape, a callable
    /// shape's call signatures, and a type application's base — the shared
    /// traversal behind both [`Self::first_callable_return_type`] and
    /// [`Self::union_callable_return_type`]. `None` when `ty` exposes no callable
    /// shape at all; `Some(vec)` (possibly empty) once a callable arm matched.
    fn callable_return_types(&mut self, ty: TypeId) -> Option<Vec<TypeId>> {
        use crate::query_boundaries::diagnostics::{
            callable_shape_for_type, function_shape, type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.callable_return_types(non_nullish);
        }

        if let Some(shape) = function_shape(self.ctx.types, ty) {
            return Some(vec![shape.return_type]);
        }

        if let Some(signatures) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, ty)
        {
            return Some(signatures.iter().map(|sig| sig.return_type).collect());
        }

        if let Some(shape) = callable_shape_for_type(self.ctx.types, ty) {
            return Some(
                shape
                    .call_signatures
                    .iter()
                    .map(|sig| sig.return_type)
                    .collect(),
            );
        }

        if let Some(app) = type_application(self.ctx.types, ty) {
            return self.callable_return_types(app.base);
        }

        None
    }

    fn first_callable_return_type(&mut self, ty: TypeId) -> Option<TypeId> {
        self.callable_return_types(ty)?.first().copied()
    }

    /// The expected return type governing the arrow-body drill, matching `tsc`'s
    /// `elaborateArrowFunction`: `getUnionType(map(getSignaturesOfType(target,
    /// Call), getReturnTypeOfSignature))`.
    ///
    /// When the target is an overload set (multiple call signatures), the arrow's
    /// inferred return type is related against the UNION of every signature's
    /// return type — not just the first. A contextually-typed arrow whose body
    /// unions across the overload parameters — e.g. `(x) => x` assigned to
    /// `{ (x: string): string; (x: number): number }`, whose body `x` is
    /// contextually typed `string | number` — has return type `string | number`.
    /// Relating that against only the *first* signature's return type (`string`,
    /// as [`Self::first_callable_return_type`] yields) makes the drill spuriously
    /// fail, so the drill reports an inner `TS2322` anchored inside the body and
    /// suppresses `tsc`'s outer whole-function relation error. Relating against
    /// the union (`string | number`) lets the drill relation succeed exactly as
    /// `tsc`'s does, so this elaborator declines and the caller reports the
    /// outer `TS2322`/`TS2345` at the assignment/argument site with the full
    /// function-type message and its nested elaboration.
    ///
    /// For a single-signature target the union collapses to that one return type,
    /// so this is identical to [`Self::first_callable_return_type`] there and only
    /// changes behavior for genuine overload sets.
    fn union_callable_return_type(&mut self, ty: TypeId) -> Option<TypeId> {
        // Reduce the collected returns to `tsc`'s `getUnionType`: none → decline,
        // one → the sole member (no needless intern), many → the interned union
        // (which dedupes/reduces).
        let return_types = self.callable_return_types(ty)?;
        match return_types.len() {
            0 => None,
            1 => Some(return_types[0]),
            _ => Some(diagnostic_query::display_union_type(
                self.ctx.types,
                return_types,
            )),
        }
    }

    /// The expected return type governing the arrow-body drill for an object
    /// literal member whose declared type is `ty`.
    ///
    /// Uses [`Self::union_callable_return_type`] so an overloaded member type
    /// (`{ (x: string): string; (x: number): number }`) drives the drill with
    /// the UNION of its signature returns, matching `tsc`'s `elaborateArrowFunction`
    /// exactly as the argument/assignment sites do; a single-signature member
    /// collapses to that one return type, leaving the alias-resolution behavior
    /// below unchanged.
    ///
    /// [`Self::union_callable_return_type`] answers for a type whose callable
    /// structure is already materialized. A member written through a type alias
    /// (`cb: Fn` for `type Fn = () => string`) arrives as an unresolved
    /// `TypeData::Lazy(DefId)` that it does not see through, so the drill read
    /// "not callable" for a type `tsc` considers identical to the inline
    /// signature and reported the `TS2322` at the property name instead of at
    /// the offending body expression.
    ///
    /// The alias hop is spent on `ty` **as written** and only after the direct
    /// probe declines. Both halves matter:
    ///
    /// - Resolving after the probe declines cannot change an answer it already
    ///   gave, only add one where there was none.
    /// - Hopping on `ty` itself rather than inside the probe keeps the hop away
    ///   from the probe's nullish-stripping arm. An optional or explicitly
    ///   nullable member (`cb?: Fn`, `cb: Fn | undefined`) is a union, not an
    ///   alias, so it declines here — which is what `tsc` does: it reports the
    ///   whole function type at the member for those, drilling neither the
    ///   alias form nor the inline one.
    ///
    /// A *generic* alias application (`cb: G<string>` for `type G<T> = () =>
    /// T`) arrives as `TypeData::Application`, whose `base` is itself an
    /// unresolved `Lazy(DefId)` reference to `G`'s own uninstantiated body —
    /// neither the direct probe nor the lazy hop above (which only resolves
    /// `ty` itself, and an `Application` is not a `Lazy`) sees through it. A
    /// third, narrower hop evaluates `ty` through the same substitution the
    /// checker uses everywhere else a type application is finally read
    /// ([`Self::judge_evaluate`]), so the return type driving the drill's
    /// relation and message is the *instantiated* `string`, not the alias's
    /// own type parameter `T`. This hop is gated on `ty` actually being an
    /// application so it cannot fire on an unrelated declined shape.
    fn callable_return_type_for_drill(&mut self, ty: TypeId) -> Option<TypeId> {
        if let Some(found) = self.union_callable_return_type(ty) {
            return Some(found);
        }
        let resolved = self.resolve_lazy_type(ty);
        if resolved != ty
            && let Some(found) = self.union_callable_return_type(resolved)
        {
            return Some(found);
        }
        if crate::query_boundaries::diagnostics::type_application(self.ctx.types, ty).is_some() {
            let evaluated = self.judge_evaluate(ty);
            if evaluated != ty {
                return self.union_callable_return_type(evaluated);
            }
        }
        None
    }
}
