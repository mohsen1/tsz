//! Call argument elaboration logic (object literal, array literal, function return).

use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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
        self.diagnostic_relation_boolean_guard(left, right)
            && self.diagnostic_relation_boolean_guard(right, left)
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

                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_method: shape.is_method,
                    };
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
        if arg_shape.properties.is_empty()
            && arg_shape.string_index.is_none()
            && arg_shape.number_index.is_none()
        {
            return None;
        }

        let mut unknown_properties = Vec::with_capacity(arg_shape.properties.len());
        for prop in &arg_shape.properties {
            let mut unknown_prop = tsz_solver::PropertyInfo::new(prop.name, TypeId::UNKNOWN);
            unknown_prop.optional = prop.optional;
            unknown_prop.readonly = prop.readonly;
            unknown_properties.push(unknown_prop);
        }
        let unknown_object = if arg_shape.string_index.is_some() || arg_shape.number_index.is_some()
        {
            let unknown_shape = tsz_solver::ObjectShape {
                properties: unknown_properties,
                string_index: arg_shape.string_index.as_ref().map(|sig| {
                    tsz_solver::IndexSignature {
                        value_type: TypeId::UNKNOWN,
                        ..*sig
                    }
                }),
                number_index: arg_shape.number_index.as_ref().map(|sig| {
                    tsz_solver::IndexSignature {
                        value_type: TypeId::UNKNOWN,
                        ..*sig
                    }
                }),
                ..Default::default()
            };
            self.ctx.types.factory().object_with_index(unknown_shape)
        } else {
            self.ctx.types.factory().object(unknown_properties)
        };

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

                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    callee_type,
                ) {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_method: shape.is_method,
                    };
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

        let snap = self.ctx.snapshot_full();
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

        self.ctx.rollback_full(&snap);

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
                    || self.diagnostic_relation_boolean_guard(branch_type, target_type)
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
                if self.diagnostic_relation_boolean_guard(source_prop_type, target_prop_type)
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

        let Some(expected_return_type) = self.first_callable_return_type(param_type) else {
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
                // check if the return expression type is assignable to the expected
                // return type. tsc reports TS2322 on the return expression when the
                // type violates the expected return type (e.g., returning a string
                // where Function is expected in a property assignment context).
                //
                // Skip void expected return types: void-returning callbacks accept any
                // return value, so elaborating would produce false positives.
                if expected_return_type == TypeId::VOID {
                    return false;
                }
                // Skip elaboration when the callback has explicit parameter type
                // annotations. tsc only elaborates return types for fully contextually-
                // typed callbacks (no explicit param annotations). When a developer
                // explicitly annotates parameter types, the error is reported at the
                // argument level (TS2345) rather than drilling into the return expression.
                let has_explicit_param_annotations =
                    func.parameters.nodes.iter().any(|param_idx| {
                        self.ctx
                            .arena
                            .get(*param_idx)
                            .and_then(|n| self.ctx.arena.get_parameter(n))
                            .is_some_and(|p| p.type_annotation.is_some())
                    });
                if has_explicit_param_annotations {
                    return false;
                }
                let body_type = self.get_type_of_node(func.body);
                if body_type == TypeId::ERROR
                    || body_type == TypeId::ANY
                    || expected_return_type == TypeId::ERROR
                    || expected_return_type == TypeId::ANY
                    || self.diagnostic_relation_boolean_guard(body_type, expected_return_type)
                {
                    return false;
                }
                // Skip elaboration when the body type is itself callable (a function type).
                // When the return type is a function but the expected type is not (or vice
                // versa), tsc reports TS2345 on the whole callback rather than TS2322 on
                // the body expression.
                if self.first_callable_return_type(body_type).is_some()
                    && self
                        .first_callable_return_type(expected_return_type)
                        .is_none()
                {
                    return false;
                }
                // Report the error at the return expression with return types.
                // tsc anchors expression-body arrow return mismatches at the body
                // expression (col of the literal/expression), not the arrow function.
                // E.g.: `const f: (a: number) => string = (a) => a + 1`
                // → TS2322 at `a + 1` with "Type 'number' is not assignable to type 'string'."
                let display_target = self.evaluate_type_with_env(expected_return_type);
                if self.array_elaboration_widening_required_for_display(body_type, display_target) {
                    self.error_type_not_assignable_at_with_widened_source_display(
                        body_type,
                        display_target,
                        func.body,
                    );
                } else {
                    self.error_type_not_assignable_at_with_anchor(
                        body_type,
                        display_target,
                        func.body,
                    );
                }
                true
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                // Conditionals need branch-level elaboration. Let the caller
                // handle these at the argument/assignment level.
                false
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(body_node) else {
                    return false;
                };
                self.try_elaborate_object_literal_arg_error(paren.expression, expected_return_type)
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
                    || self.diagnostic_relation_boolean_guard(body_type, expected_return_type)
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
                    // For functions that are the RHS of an assignment (e.g., `A.prototype.foo = function() {}`),
                    // use the assignment LHS as the anchor to match tsc behavior.
                    // Otherwise, use the function position as the anchor.
                    let diag_anchor = if self.is_rhs_of_assignment(func_idx) {
                        let lhs = self.find_assignment_lhs_for_rhs(func_idx);
                        lhs.unwrap_or(func_idx)
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

    fn first_callable_return_type(&mut self, ty: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::diagnostics::{
            callable_shape_for_type, function_shape, type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.first_callable_return_type(non_nullish);
        }

        if let Some(shape) = function_shape(self.ctx.types, ty) {
            return Some(shape.return_type);
        }

        if let Some(signatures) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, ty)
        {
            return signatures.first().map(|sig| sig.return_type);
        }

        if let Some(shape) = callable_shape_for_type(self.ctx.types, ty) {
            return shape.call_signatures.first().map(|sig| sig.return_type);
        }

        if let Some(app) = type_application(self.ctx.types, ty) {
            return self.first_callable_return_type(app.base);
        }

        None
    }
}
