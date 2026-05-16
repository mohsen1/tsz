//! Call error emission functions (TS2345, TS2554, TS2769, etc.).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn should_suppress_argument_not_assignable_diagnostic(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> bool {
        // Suppress when types are identical or either is a special escape-hatch type.
        if arg_type == param_type
            || arg_type == TypeId::ERROR
            || param_type == TypeId::ERROR
            // `any` suppresses most call-site assignability errors, but tsc still
            // reports TS2345 for the bottom-type case `any -> never`.
            || (arg_type == TypeId::ANY && param_type != TypeId::NEVER)
            || param_type == TypeId::ANY
            || arg_type == TypeId::UNKNOWN
            || param_type == TypeId::UNKNOWN
        {
            return true;
        }
        if (param_type == TypeId::NEVER
            || self.evaluate_type_for_assignability(param_type) == TypeId::NEVER)
            && self.generic_indexed_access_argument_surface(arg_type)
        {
            return true;
        }

        if crate::query_boundaries::assignability::are_types_structurally_identical(
            self.ctx.types,
            &self.ctx,
            arg_type,
            param_type,
        ) {
            return true;
        }

        self.same_non_class_nominal_display(arg_type, param_type)
    }

    fn same_non_class_nominal_display(&mut self, arg_type: TypeId, param_type: TypeId) -> bool {
        let arg_display = self.format_type_diagnostic(arg_type);
        if arg_display != self.format_type_diagnostic(param_type)
            || !arg_display.contains('<')
            || arg_display.starts_with("typeof ")
        {
            return false;
        }

        match (
            self.non_class_nominal_def(arg_type),
            self.non_class_nominal_def(param_type),
        ) {
            (Some(arg_def), Some(param_def)) => arg_def == param_def,
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        }
    }

    fn non_class_nominal_def(&mut self, type_id: TypeId) -> Option<tsz_solver::DefId> {
        let def_id = self.nominal_def_for_argument_display(type_id).or_else(|| {
            let evaluated = self.evaluate_type_for_assignability(type_id);
            self.nominal_def_for_argument_display(evaluated)
        })?;
        let def = self.ctx.definition_store.get(def_id)?;
        (!matches!(def.kind, tsz_solver::def::DefKind::Class)).then_some(def_id)
    }

    fn nominal_def_for_argument_display(&self, type_id: TypeId) -> Option<tsz_solver::DefId> {
        crate::query_boundaries::common::type_application(self.ctx.types, type_id)
            .and_then(|app| crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base))
            .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id))
    }

    /// Report an argument not assignable error using solver diagnostics with source tracking.
    /// When solver failure analysis identifies a specific reason (e.g. missing property),
    /// the detailed diagnostic is emitted as related information matching tsc's behavior.
    pub fn error_argument_not_assignable_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_argument_not_assignable_at_with_relation_failure(
            arg_type, param_type, idx, None,
        );
    }

    pub(crate) fn error_argument_not_assignable_at_with_relation_failure(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
        relation_failure: Option<&crate::query_boundaries::relation_types::RelationFailure>,
    ) {
        if self.should_suppress_argument_not_assignable_diagnostic(arg_type, param_type) {
            return;
        }
        if self.should_suppress_constraint_cascade_constructor_argument(arg_type, param_type) {
            return;
        }

        if self.should_suppress_partial_self_argument_mismatch(arg_type, param_type) {
            return;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            arg_type, param_type, idx,
        ) {
            return;
        }
        if self
            .should_suppress_promise_then_nullable_callback_arg_mismatch(arg_type, param_type, idx)
        {
            return;
        }
        if self.is_callback_like_argument(idx)
            && self.is_assignable_via_generator_never_yield_callback(arg_type, param_type)
        {
            return;
        }

        // Suppress cascading TS2345 when TS2353 (excess property) already covers this span.
        if let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) {
            let arg_end = anchor.start.saturating_add(anchor.length);
            if self.ctx.diagnostics.iter().any(|diag| {
                diag.code
                    == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                    && diag.start >= anchor.start
                    && diag.start < arg_end
            }) {
                return;
            }
        }
        // Suppress TS2345 for callbacks with unannotated parameters that rely on
        // contextual typing, but ONLY when contextual typing genuinely failed to
        // resolve parameter types (they remained `any`/`unknown`).
        // When contextual typing DID resolve concrete types and the mismatch
        // persists, the error is real — e.g., individual params `(a: 1|2, b: "1"|"2")`
        // vs a readonly tuple union rest parameter `(...args: readonly [1, "1"] | readonly [2, "2"])`.
        //
        // Additionally, only suppress when the target signature actually has a
        // parameter at every position the source callback declares. If the
        // target has fewer parameters than the source (and no rest), contextual
        // typing cannot supply types for the extra source parameters and the
        // parameter-count mismatch ("Target signature provides too few
        // arguments") must surface as TS2345 — see issue #4027.
        if self.arg_is_callback_with_unannotated_params(idx)
            && self.callback_type_params_are_unresolved(arg_type)
            && self.target_can_contextually_type_callback_params(idx, param_type)
        {
            return;
        }
        if self.try_elaborate_array_literal_mismatch_with_relation_failure(
            idx,
            arg_type,
            param_type,
            relation_failure,
        ) {
            return;
        }
        if self.try_elaborate_callback_body_diagnostics(idx, param_type) {
            return;
        }
        // Use relation evidence from the caller when available. Legacy callers
        // build a boundary request here so TS2345 fallback rendering still goes
        // through the same relation/failure path.
        let fallback_outcome;
        let failure_reason = if let Some(reason) = relation_failure {
            Some(reason.to_solver_failure_reason())
        } else {
            use crate::query_boundaries::assignability::RelationRequest;
            let (prepared_arg, prepared_param) =
                self.prepare_assignability_inputs(arg_type, param_type);
            let request = RelationRequest::call_arg(prepared_arg, prepared_param);
            fallback_outcome = self.execute_relation_request(&request);
            fallback_outcome.failure.as_ref().map(
                crate::query_boundaries::relation_types::RelationFailure::to_solver_failure_reason,
            )
        };

        // When the failure reason is NoCommonProperties (weak types with no
        // properties in common), tsc emits TS2559 directly instead of TS2345.
        // If the source is callable/constructable and calling it would produce a
        // compatible type, tsc emits TS2560 ("did you mean to call it?") instead.
        // Use the unwidened literal type for the diagnostic message — tsc preserves
        // literal types (e.g., "12" not "number", "false" not "boolean") in
        // "has no properties in common" messages.
        if matches!(
            &failure_reason,
            Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
        ) {
            // Try to get the literal expression display (unwidened) from the AST
            let mut arg_str = self
                .literal_call_argument_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(arg_type));
            arg_str = self.rewrite_source_display_for_non_literal_target_assignability(
                arg_type, param_type, arg_str,
            );
            let param_str = self.format_type_for_diagnostic_role(
                param_type,
                DiagnosticTypeDisplayRole::CallParameter {
                    argument: arg_type,
                    argument_idx: idx,
                },
            );

            // Check if the source is callable/constructable and calling would fix
            // the type mismatch — if so, emit TS2560 instead of TS2559.
            let (msg_template, code) = if self
                .should_suggest_calling_for_weak_type(arg_type, param_type)
            {
                (
                    diagnostic_messages::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                    diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                )
            } else {
                (
                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                )
            };
            if code
                == diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT
            {
                arg_str = self.widen_weak_type_callable_source_display(arg_type, arg_str);
            }
            let (arg_str, param_str) =
                self.finalize_pair_display_for_diagnostic(arg_type, param_type, arg_str, param_str);
            let message = format_message(msg_template, &[&arg_str, &param_str]);
            let request =
                DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message);
            self.emit_render_request(idx, request);
            return;
        }

        let mut arg_str = self.format_type_for_diagnostic_role(
            arg_type,
            DiagnosticTypeDisplayRole::CallArgument {
                parameter: param_type,
                argument_idx: idx,
            },
        );
        if param_type == TypeId::BOOLEAN && matches!(arg_str.as_str(), "true[]" | "false[]") {
            arg_str = "boolean[]".to_string();
        }
        let mut param_str = self.format_type_for_diagnostic_role(
            param_type,
            DiagnosticTypeDisplayRole::CallParameter {
                argument: arg_type,
                argument_idx: idx,
            },
        );
        if let Some(display) =
            self.mapped_property_mismatch_parameter_display(&param_str, failure_reason.as_ref())
        {
            param_str = display;
        }
        if let Some(display) =
            self.constrained_variadic_tuple_parameter_display(param_type, arg_type)
        {
            param_str = display;
        }
        if arg_str.starts_with('{') && param_str.contains("<{") {
            param_str = Self::widen_object_member_literals_inside_generic_display(&param_str);
        }
        if let Some((generic_arg_str, generic_param_str)) =
            self.generic_direct_primitive_mismatch_display(arg_type, param_type, idx)
        {
            arg_str = generic_arg_str;
            param_str = generic_param_str;
        }
        if let Some(widened_arg_str) = self
            .widen_literal_call_argument_display_against_plain_primitive_parameter(
                arg_type, idx, &param_str,
            )
        {
            arg_str = widened_arg_str;
        }
        if self.inline_literal_satisfies_has_permissive_target(idx) {
            arg_str = Self::widen_member_literals_in_display_text(&arg_str);
        }
        param_str = Self::trim_single_unbalanced_trailing_type_arg_close(param_str);
        let (arg_str, param_str) =
            self.finalize_pair_display_for_diagnostic(arg_type, param_type, arg_str, param_str);
        let message = format_message(
            diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            &[&arg_str, &param_str],
        );

        let request = if let Some(reason) = failure_reason {
            DiagnosticRenderRequest::with_failure_reason(
                DiagnosticAnchorKind::Exact,
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
                message,
                reason,
                arg_type,
                param_type,
            )
        } else {
            DiagnosticRenderRequest::simple(
                DiagnosticAnchorKind::Exact,
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
                message,
            )
        };

        self.emit_render_request(idx, request);
    }

    fn should_suppress_promise_then_nullable_callback_arg_mismatch(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) -> bool {
        if !self.is_callback_like_argument(idx)
            || !self.type_is_nullish_only(param_type)
            || matches!(arg_type, TypeId::ERROR | TypeId::ANY)
        {
            return false;
        }

        let Some(call_idx) = self.parent_call_containing_argument(idx) else {
            return false;
        };
        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
            return false;
        };
        let callee_idx = self.ctx.arena.skip_parenthesized(call.expression);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if name.escaped_text != "then" {
            return false;
        }

        let receiver_type = self.get_type_of_node(access.expression);
        let evaluated_receiver = self.evaluate_type_with_env(receiver_type);
        self.type_ref_is_promise_like(receiver_type)
            || self.type_ref_is_promise_like(evaluated_receiver)
    }

    fn type_is_nullish_only(&self, type_id: TypeId) -> bool {
        match type_id {
            TypeId::NULL | TypeId::UNDEFINED => true,
            _ => crate::query_boundaries::common::union_members(self.ctx.types, type_id)
                .is_some_and(|members| {
                    !members.is_empty()
                        && members
                            .iter()
                            .all(|&member| matches!(member, TypeId::NULL | TypeId::UNDEFINED))
                }),
        }
    }

    fn parent_call_containing_argument(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..100 {
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_idx = ext.parent;
            let parent = self.ctx.arena.get(parent_idx)?;
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .and_then(|call| call.arguments.as_ref())
                        .is_some_and(|args| args.nodes.contains(&current))
                        .then_some(parent_idx);
                }
                _ => return None,
            }
        }
        None
    }

    fn should_suppress_constraint_cascade_constructor_argument(
        &self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> bool {
        if !self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
        {
            return false;
        }
        if !crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, arg_type) {
            return false;
        }
        if crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, param_type)
            || crate::query_boundaries::common::is_callable_type(self.ctx.types, param_type)
        {
            return true;
        }
        crate::query_boundaries::common::union_members(self.ctx.types, param_type).is_some_and(
            |members| {
                members.iter().all(|&member| {
                    crate::query_boundaries::common::is_constructor_like_type(
                        self.ctx.types,
                        member,
                    ) || crate::query_boundaries::common::is_callable_type(self.ctx.types, member)
                })
            },
        )
    }

    fn trim_single_unbalanced_trailing_type_arg_close(display: String) -> String {
        let Some(candidate) = display.strip_suffix('>') else {
            return display;
        };

        let mut opens = 0usize;
        let mut closes = 0usize;
        let mut prev = '\0';
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for ch in display.chars() {
            if escaped {
                escaped = false;
                prev = ch;
                continue;
            }
            if in_single || in_double {
                if ch == '\\' {
                    escaped = true;
                } else if in_single && ch == '\'' {
                    in_single = false;
                } else if in_double && ch == '"' {
                    in_double = false;
                }
                prev = ch;
                continue;
            }
            match ch {
                '\'' => in_single = true,
                '"' => in_double = true,
                '<' => opens += 1,
                '>' if prev != '=' => closes += 1,
                _ => {}
            }
            prev = ch;
        }

        if closes == opens.saturating_add(1) {
            candidate.to_string()
        } else {
            display
        }
    }

    fn widen_literal_call_argument_display_against_plain_primitive_parameter(
        &mut self,
        arg_type: TypeId,
        arg_idx: NodeIndex,
        param_display: &str,
    ) -> Option<String> {
        let param_base = match param_display {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            _ => return None,
        };
        let source = self
            .literal_type_from_initializer(arg_idx)
            .unwrap_or(arg_type);
        let source_base =
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, source);
        if source_base == source || source_base == param_base {
            return None;
        }
        Some(self.format_type_for_assignability_message(source_base))
    }

    pub(in crate::error_reporter::call_errors) fn mapped_property_mismatch_parameter_display(
        &mut self,
        param_display: &str,
        failure_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Option<String> {
        if !param_display.trim_start().starts_with("{ [") {
            return None;
        }
        let tsz_solver::SubtypeFailureReason::PropertyTypeMismatch {
            property_name,
            target_property_type,
            ..
        } = failure_reason?
        else {
            return None;
        };

        let mut property = tsz_solver::PropertyInfo::new(*property_name, *target_property_type);
        property.optional = Self::type_includes_undefined(self.ctx.types, *target_property_type);
        let display_type = self.ctx.types.factory().object(vec![property]);
        Some(self.format_type_for_assignability_message(display_type))
    }

    fn type_includes_undefined(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
        ty == TypeId::UNDEFINED
            || crate::query_boundaries::common::union_members(db, ty)
                .is_some_and(|members| members.contains(&TypeId::UNDEFINED))
    }

    fn widen_object_member_literals_inside_generic_display(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0;
        let mut angle_depth = 0usize;
        let mut object_depth = 0usize;

        while i < bytes.len() {
            let ch = bytes[i] as char;
            match ch {
                '<' => {
                    angle_depth += 1;
                    out.push(ch);
                    i += 1;
                }
                '>' => {
                    angle_depth = angle_depth.saturating_sub(1);
                    out.push(ch);
                    i += 1;
                }
                '{' if angle_depth > 0 => {
                    object_depth += 1;
                    out.push(ch);
                    i += 1;
                }
                '}' if object_depth > 0 => {
                    object_depth -= 1;
                    out.push(ch);
                    i += 1;
                }
                ':' if angle_depth > 0 && object_depth > 0 => {
                    out.push(ch);
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        out.push(bytes[i] as char);
                        i += 1;
                    }
                    if i >= bytes.len() {
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\\' {
                                i = (i + 2).min(bytes.len());
                                continue;
                            }
                            if bytes[i] == b'"' {
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        out.push_str("string");
                    } else if display[i..].starts_with("true") {
                        i += 4;
                        out.push_str("boolean");
                    } else if display[i..].starts_with("false") {
                        i += 5;
                        out.push_str("boolean");
                    } else {
                        out.push(bytes[i] as char);
                        i += 1;
                    }
                }
                _ => {
                    out.push(ch);
                    i += 1;
                }
            }
        }

        out
    }

    /// Report an argument count mismatch error using solver diagnostics with source tracking.
    /// TS2554: Expected {0} arguments, but got {1}.
    ///
    /// When there are excess arguments (`got > expected_max`), tsc points the
    /// diagnostic span at the excess arguments rather than the call expression.
    /// The `args` slice provides the argument node indices so we can compute
    /// the span from the first excess argument to the last argument.
    pub fn error_argument_count_mismatch_at(
        &mut self,
        expected_min: usize,
        expected_max: usize,
        got: usize,
        idx: NodeIndex,
        args: &[NodeIndex],
    ) {
        // When there are excess arguments, point to them instead of the callee.
        let (start, length) = if let Some((s, l)) =
            self.resolve_excess_argument_span(args, expected_max)
        {
            (s, l)
        } else if self.is_new_expression(idx) {
            // For `new X()` with too few arguments, TSC uses the full
            // `new X(...)` span (starting from the `new` keyword).
            if let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) {
                (anchor.start, anchor.length)
            } else {
                return;
            }
        } else if let Some(anchor) =
            self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::CallPrimary)
        {
            (anchor.start, anchor.length)
        } else {
            return;
        };

        let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
            self.ctx.types,
            &self.ctx.binder.symbols,
            self.ctx.file_name.as_str(),
        )
        .with_def_store(&self.ctx.definition_store);
        let diag = builder.argument_count_mismatch(expected_min, expected_max, got, start, length);
        self.ctx
            .diagnostics
            .push(diag.to_checker_diagnostic(&self.ctx.file_name));
    }

    /// TS2560 ("did you mean to call it?") in call-site weak-type comparisons
    /// expects widened primitive names for callable sources.
    fn widen_weak_type_callable_source_display(
        &self,
        arg_type: TypeId,
        _arg_str: String,
    ) -> String {
        Self::widen_member_literals_in_display_text(
            &self.format_type_diagnostic(self.widen_literal_type(arg_type)),
        )
    }

    /// Check if a node is a `new` expression.
    fn is_new_expression(&self, idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    /// Report a spread argument type error (TS2556).
    /// TS2556: A spread argument must either have a tuple type or be passed to a rest parameter.
    pub fn error_spread_must_be_tuple_or_rest_at(&mut self, idx: NodeIndex) {
        self.error_at_node(
            idx,
            diagnostic_messages::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
            diagnostic_codes::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
        );
    }

    /// Report an "expected at least N arguments" error (TS2555).
    /// TS2555: Expected at least {0} arguments, but got {1}.
    pub fn error_expected_at_least_arguments_at(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        let message = format!("Expected at least {expected_min} arguments, but got {got}.");
        // For `new` expressions, TSC uses the full `new X(...)` span.
        let anchor_kind = if self.is_new_expression(idx) {
            DiagnosticAnchorKind::Exact
        } else {
            DiagnosticAnchorKind::CallPrimary
        };
        self.error_at_anchor(
            idx,
            anchor_kind,
            &message,
            diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT,
        );
    }

    /// Report "No overload matches this call" with related overload failures.
    pub fn error_no_overload_matches_at(
        &mut self,
        idx: NodeIndex,
        failures: &[tsz_solver::PendingDiagnostic],
    ) {
        tracing::debug!(
            "error_no_overload_matches_at: File name: {}",
            self.ctx.file_name
        );

        if self.should_suppress_concat_overload_error(idx) {
            return;
        }

        use crate::query_boundaries::common::PendingDiagnostic;

        let argument_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
        let mut literal_anchor = self.overload_literal_argument_anchor(idx, failures);
        let shared_argument_anchor = self
            .shared_overload_argument_anchor_from_spans(idx, &argument_failures)
            .or_else(|| self.shared_overload_argument_anchor(idx, &argument_failures));
        let identical_argument_failures = {
            let mut formatter = self.ctx.create_type_formatter();
            argument_failures
                .first()
                .map(|first| {
                    let rendered_first = formatter.render(first);
                    argument_failures
                        .iter()
                        .skip(1)
                        .all(|failure| formatter.render(failure).message == rendered_first.message)
                })
                .unwrap_or(false)
        };
        let remaining_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
        let callback_body_failure_span = if !argument_failures.is_empty() {
            let callback_spans: Vec<(u32, u32)> = self
                .logical_call_argument_nodes(idx)
                .unwrap_or_default()
                .into_iter()
                .filter(|&arg_idx| self.is_callback_like_argument(arg_idx))
                .flat_map(|arg_idx| self.callback_body_spans(arg_idx))
                .collect();
            let mut shared = None;
            let mut all_callback_body_spans = !callback_spans.is_empty();
            for failure in &argument_failures {
                let Some(span) = failure.span.as_ref() else {
                    all_callback_body_spans = false;
                    break;
                };
                if !callback_spans
                    .iter()
                    .any(|(start, end)| span.start >= *start && span.start < *end)
                {
                    all_callback_body_spans = false;
                    break;
                }
                if let Some((start, length)) = shared {
                    if start != span.start || length != span.length {
                        all_callback_body_spans = false;
                        break;
                    }
                } else {
                    shared = Some((span.start, span.length));
                }
            }
            all_callback_body_spans
                .then_some(shared)
                .flatten()
                .map(|(start, length)| ResolvedDiagnosticAnchor {
                    node_idx: idx,
                    start,
                    length,
                })
        } else {
            None
        };
        let remaining_failures_are_count_mismatches = remaining_failures.iter().all(|failure| {
            matches!(
                failure.code,
                diagnostic_codes::EXPECTED_ARGUMENTS_BUT_GOT
                    | diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT
            )
        });
        let all_failures_are_argument_mismatches = !failures.is_empty()
            && failures.iter().all(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            });
        let anchor_argument_from_first_argument_mismatch = all_failures_are_argument_mismatches
            && shared_argument_anchor.is_none()
            && !(self.overload_callee_is_property_like(idx)
                && self
                    .logical_call_argument_nodes(idx)
                    .is_some_and(|args| args.len() > 1))
            && self.first_argument_mismatches_all_overload_expected_types(idx, &argument_failures);
        let anchor_argument_from_mixed_failures = shared_argument_anchor.is_some()
            && !remaining_failures.is_empty()
            && remaining_failures_are_count_mismatches;
        // When all overload failures share the same argument anchor but the
        // failure messages disagree *and* the argument is an object literal,
        // tsc treats the overload set — not the argument — as the culprit and
        // anchors the top-level TS2769 at the callee. This covers cases like
        // `v({s:"", n:0})` against `(x:{s:string}) | (x:{n:number})`, where
        // each overload rejects a different excess property on the same
        // literal. For non-object-literal arguments (e.g., `fn(true)` vs
        // `(x:string)|(x:number)`), tsc still anchors at the argument.
        let is_tagged_template_call = self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION);
        let shared_argument_is_object_literal = shared_argument_anchor.is_some_and(|anchor_idx| {
            self.ctx
                .arena
                .get(anchor_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        });
        // For object-literal overload failures, tsc's anchor depends on the
        // actual excess-property culprit. If every overload rejects the same
        // property (`fn({ z: 3, a: 3 })` against `{x}`/`{y}`), anchor at that
        // property. If overloads reject different properties (`v({s,n})`
        // against `{s}`/`{n}`), anchor at the callee because no single property
        // explains the whole overload failure.
        let shared_excess_property_name = if shared_argument_is_object_literal
            && !argument_failures.is_empty()
        {
            let mut first_name = None;
            let mut all_same = true;
            for failure in &argument_failures {
                let Some(tsz_solver::DiagnosticArg::Type(arg_type)) = failure.args.first() else {
                    all_same = false;
                    break;
                };
                let Some(tsz_solver::DiagnosticArg::Type(param_type)) = failure.args.get(1) else {
                    all_same = false;
                    break;
                };
                use crate::query_boundaries::assignability::RelationRequest;
                let (prepared_arg, prepared_param) =
                    self.prepare_assignability_inputs(*arg_type, *param_type);
                let request = RelationRequest::call_arg(prepared_arg, prepared_param);
                let outcome = self.execute_relation_request(&request);
                let Some(tsz_solver::SubtypeFailureReason::ExcessProperty {
                    property_name, ..
                }) = outcome
                    .failure
                    .as_ref()
                    .map(crate::query_boundaries::relation_types::RelationFailure::to_solver_failure_reason)
                else {
                    all_same = false;
                    break;
                };
                match &first_name {
                    Some(first_name) if first_name != &property_name => {
                        all_same = false;
                        break;
                    }
                    Some(_) => {}
                    None => first_name = Some(property_name),
                }
            }
            all_same && first_name.is_some()
        } else {
            false
        };
        let anchor_argument_from_all_failures = all_failures_are_argument_mismatches
            && shared_argument_anchor.is_some()
            && (!shared_argument_is_object_literal
                || is_tagged_template_call
                || identical_argument_failures
                || shared_excess_property_name);
        let raw_argument_anchor =
            shared_argument_anchor.or_else(|| self.first_call_argument_anchor(idx));
        let argument_anchor_is_callback = raw_argument_anchor
            .is_some_and(|anchor_idx| self.is_callback_expression_argument(anchor_idx));
        let callback_overloads_are_callable_only = argument_failures.iter().all(|failure| {
            matches!(failure.args.get(1), Some(tsz_solver::DiagnosticArg::Type(param_ty))
                if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, *param_ty).is_some()
                    || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, *param_ty).is_some())
        });
        let callback_argument_has_prior_diagnostics =
            raw_argument_anchor.is_some_and(|anchor_idx| {
                self.ctx.arena.get(anchor_idx).is_some_and(|arg_node| {
                    self.ctx.diagnostics.iter().any(|diag| {
                        diag.code != diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                            && diag.start >= arg_node.pos
                            && diag.start < arg_node.end
                    })
                })
            });
        let single_callback_argument = self
            .ctx
            .arena
            .get(idx)
            .and_then(|call_node| self.ctx.arena.get_call_expr(call_node))
            .and_then(|call_expr| call_expr.arguments.as_ref())
            .is_some_and(|args| args.nodes.len() == 1);
        let is_new_call = self.is_new_expression(idx);
        let is_bind_method_call = self
            .ctx
            .arena
            .get(idx)
            .and_then(|call_node| self.ctx.arena.get_call_expr(call_node))
            .is_some_and(|call_expr| {
                let is_bind = self
                    .ctx
                    .arena
                    .get(call_expr.expression)
                    .and_then(|callee| {
                        if callee.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                            self.ctx.arena.get_access_expr(callee)
                        } else {
                            None
                        }
                    })
                    .and_then(|access| self.ctx.arena.get(access.name_or_argument))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == "bind");
                if !is_bind {
                    return false;
                }

                // tsc anchors callback.bind(2)-style failures at `bind`, but
                // keeps first-argument anchoring for `bind(undefined)` mismatches.
                let first_arg_is_undefined = call_expr
                    .arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first().copied())
                    .and_then(|arg_idx| self.ctx.arena.get(arg_idx))
                    .and_then(|arg_node| self.ctx.arena.get_identifier(arg_node))
                    .is_some_and(|ident| ident.escaped_text == "undefined");
                !first_arg_is_undefined
            });
        if is_bind_method_call {
            // For `callback.bind(2)`-style overload failures, tsc anchors TS2769
            // at `bind`, not at the argument literal.
            literal_anchor = None;
        }
        let allow_callback_argument_anchor = argument_anchor_is_callback
            && single_callback_argument
            && all_failures_are_argument_mismatches
            && callback_overloads_are_callable_only
            && !callback_argument_has_prior_diagnostics;
        let allow_new_argument_anchor = is_new_call
            && anchor_argument_from_all_failures
            && !self.is_weak_collection_constructor_new(idx);
        let anchor_first_argument = (!is_new_call || allow_new_argument_anchor)
            && (!argument_anchor_is_callback || allow_callback_argument_anchor)
            && !is_bind_method_call
            && (identical_argument_failures
                && !remaining_failures.is_empty()
                && remaining_failures_are_count_mismatches
                || anchor_argument_from_mixed_failures
                || anchor_argument_from_all_failures
                || anchor_argument_from_first_argument_mismatch);
        let tagged_generic_overload_anchor = if is_tagged_template_call
            && self.tagged_template_callee_has_generic_call_signature(idx)
        {
            self.tagged_template_generic_overload_anchor(idx)
        } else {
            None
        };
        let anchor_kind = if let Some(anchor_idx) = tagged_generic_overload_anchor {
            if anchor_idx == idx {
                DiagnosticAnchorKind::OverloadPrimary
            } else {
                DiagnosticAnchorKind::Exact
            }
        } else if literal_anchor.is_some() {
            DiagnosticAnchorKind::Exact
        } else if anchor_first_argument {
            shared_argument_anchor
                .or_else(|| self.first_call_argument_anchor(idx))
                .map(|_| DiagnosticAnchorKind::Exact)
                .unwrap_or(DiagnosticAnchorKind::OverloadPrimary)
        } else {
            DiagnosticAnchorKind::OverloadPrimary
        };
        let anchor_idx = if let Some(anchor_idx) = tagged_generic_overload_anchor {
            anchor_idx
        } else if let Some(anchor_idx) = literal_anchor {
            anchor_idx
        } else if anchor_first_argument {
            let raw_anchor = raw_argument_anchor.unwrap_or(idx);
            // When the anchor is an object literal expression, tsc drills down
            // to the first property so the TS2769 diagnostic points at the
            // first property name (e.g. `z` in `{ z: 3 }`) rather than `{`.
            self.first_object_literal_property(raw_anchor)
                .unwrap_or(raw_anchor)
        } else {
            idx
        };
        let Some(anchor) = callback_body_failure_span
            .or_else(|| self.resolve_diagnostic_anchor(anchor_idx, anchor_kind))
        else {
            return;
        };
        let mut related = Vec::new();
        let mut formatter = self.ctx.create_type_formatter();
        let span =
            tsz_solver::SourceSpan::new(self.ctx.file_name.as_str(), anchor.start, anchor.length);

        tracing::debug!("File name: {}", self.ctx.file_name);

        for failure in failures {
            let pending: PendingDiagnostic = PendingDiagnostic {
                span: Some(span.clone()),
                ..failure.clone()
            };
            let diag = formatter.render(&pending);
            if let Some(diag_span) = diag.span.as_ref() {
                related.push(DiagnosticRelatedInformation {
                    file: diag_span.file.to_string(),
                    start: diag_span.start,
                    length: diag_span.length,
                    message_text: diag.message.clone(),
                    category: DiagnosticCategory::Message,
                    code: diag.code,
                });
            }
        }

        self.emit_render_request_at_anchor(
            anchor,
            DiagnosticRenderRequest::with_related(
                anchor_kind,
                diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
                diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL.to_string(),
                related,
                RelatedInformationPolicy::OVERLOAD_FAILURES,
            ),
        );
    }

    fn tagged_template_callee_has_generic_call_signature(&mut self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(tagged) = self.ctx.arena.get_tagged_template(node).cloned() else {
            return false;
        };
        let tag_type = self.get_type_of_node(tagged.tag);
        let tag_type = self.resolve_ref_type(tag_type);
        let tag_type = self.resolve_lazy_type(tag_type);
        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, tag_type)
            .is_some_and(|shape| {
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty())
            })
    }

    fn is_weak_collection_constructor_new(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_call_expr(node)
            .and_then(|call| self.ctx.arena.get_identifier_text(call.expression))
            .is_some_and(|name| matches!(name, "WeakMap" | "WeakSet"))
    }

    fn tagged_template_generic_overload_anchor(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let args = self.logical_call_argument_nodes(idx)?;
        let first_substitution = args.get(1).copied()?;
        let first_node = self.ctx.arena.get(first_substitution)?;
        if first_node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
            || first_node.kind == tsz_scanner::SyntaxKind::TrueKeyword as u16
            || first_node.kind == tsz_scanner::SyntaxKind::FalseKeyword as u16
        {
            Some(first_substitution)
        } else {
            Some(idx)
        }
    }

    /// Report TS2693: type parameter used as value
    pub fn error_type_parameter_used_as_value(&mut self, name: &str, idx: NodeIndex) {
        use tsz_common::diagnostics::diagnostic_codes;

        let message = format!("'{name}' only refers to a type, but is being used as a value here.");

        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
        );
    }

    /// Report a "this type mismatch" error using solver diagnostics with source tracking.
    pub fn error_this_type_mismatch_at(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag =
                builder.this_type_mismatch(expected_this, actual_this, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a "type is not callable" error using solver diagnostics with source tracking.
    pub fn error_not_callable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        // Suppress cascade errors from unresolved types.
        // In strictNullChecks mode, TS18046 is preferred for `unknown`;
        // in non-strict mode, `unknown` should emit a TS2349 callability error.
        if type_id == TypeId::ERROR
            || (type_id == TypeId::UNKNOWN && self.ctx.compiler_options.strict_null_checks)
        {
            return;
        }

        // For property access expressions (e.g., `obj.notMethod`), narrow the error
        // span to just the property name, matching tsc's behavior for chained calls.
        let report_idx = if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            access.name_or_argument
        } else {
            idx
        };

        if let Some(loc) = self.get_source_location(report_idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.not_callable(type_id, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS6234: "This expression is not callable because it is a 'get' accessor.
    /// Did you mean to access it without '()'?"
    pub fn error_get_accessor_not_callable_at(&mut self, idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        let report_idx = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| {
                if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    self.ctx
                        .arena
                        .get_access_expr(node)
                        .map(|access| access.name_or_argument)
                } else {
                    None
                }
            })
            .unwrap_or(idx);

        self.error_at_node(
            report_idx,
            "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?",
            diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE,
        );
    }

    /// Report TS2348: "Value of type '{0}' is not callable. Did you mean to include 'new'?"
    /// This is specifically for class constructors called without 'new'.
    pub fn error_class_constructor_without_new_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                .replace("{0}", &type_str);

        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW,
        );
    }

    /// TS2350 was removed in tsc 6.0 — no longer emitted.
    pub const fn error_non_void_function_called_with_new_at(&mut self, _idx: NodeIndex) {}

    /// Report TS2721/TS2722/TS2723: "Cannot invoke an object which is possibly 'null'/'undefined'/'null or undefined'."
    /// Emitted when strictNullChecks is on and the callee type includes null/undefined.
    pub fn error_cannot_invoke_possibly_nullish_at(
        &mut self,
        nullish_cause: TypeId,
        idx: NodeIndex,
    ) {
        let (message, code) = if nullish_cause == TypeId::NULL {
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL,
            )
        } else if nullish_cause == TypeId::UNDEFINED {
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED,
            )
        } else {
            // Union of null and undefined (or void)
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED,
            )
        };

        self.error_at_node(idx, message, code);
    }
}
