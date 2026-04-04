//! Call error emission functions (TS2345, TS2554, TS2769, etc.).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report an argument not assignable error using solver diagnostics with source tracking.
    /// When solver failure analysis identifies a specific reason (e.g. missing property),
    /// the detailed diagnostic is emitted as related information matching tsc's behavior.
    pub fn error_argument_not_assignable_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
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
        if self.arg_is_callback_with_unannotated_params(idx)
            && self.callback_params_are_unresolved(arg_type)
        {
            return;
        }
        // Run failure analysis to produce elaboration as related information,
        // matching tsc's behavior of emitting TS2741/TS2739/TS2740 etc. as
        // related diagnostics under the primary TS2345.
        let analysis = self.analyze_assignability_failure(arg_type, param_type);

        // When the failure reason is NoCommonProperties (weak types with no
        // properties in common), tsc emits TS2559 directly instead of TS2345.
        // If the source is callable/constructable and calling it would produce a
        // compatible type, tsc emits TS2560 ("did you mean to call it?") instead.
        // Use the unwidened literal type for the diagnostic message — tsc preserves
        // literal types (e.g., "12" not "number", "false" not "boolean") in
        // "has no properties in common" messages.
        if matches!(
            &analysis.failure_reason,
            Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
        ) {
            // Try to get the literal expression display (unwidened) from the AST
            let arg_str = self
                .literal_call_argument_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(arg_type));
            let param_str =
                self.format_call_parameter_type_for_diagnostic(param_type, arg_type, idx);

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
            let message = format_message(msg_template, &[&arg_str, &param_str]);
            let request =
                DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message);
            self.emit_render_request(idx, request);
            return;
        }

        let arg_str = self.format_call_argument_type_for_diagnostic(arg_type, param_type, idx);
        let param_str = self.format_call_parameter_type_for_diagnostic(param_type, arg_type, idx);
        let message = format_message(
            diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            &[&arg_str, &param_str],
        );

        let request = if let Some(reason) = analysis.failure_reason {
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

    /// Check if a callback function type has all-any/unknown parameter types,
    /// indicating contextual typing failed to provide concrete types.
    fn callback_params_are_unresolved(&self, arg_type: TypeId) -> bool {
        if let Some(shape) = tsz_solver::type_queries::get_function_shape(
            self.ctx.types.as_type_database(),
            arg_type,
        ) {
            shape.params.is_empty()
                || shape
                    .params
                    .iter()
                    .all(|p| matches!(p.type_id, TypeId::ANY | TypeId::UNKNOWN))
        } else {
            false
        }
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

        use tsz_solver::PendingDiagnostic;

        let argument_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
        let literal_anchor = self.overload_literal_argument_anchor(idx, failures);
        let shared_argument_anchor = self.shared_overload_argument_anchor(idx, &argument_failures);
        let mut formatter = self.ctx.create_type_formatter();
        let identical_argument_failures = argument_failures
            .first()
            .map(|first| {
                let rendered_first = formatter.render(first);
                argument_failures
                    .iter()
                    .skip(1)
                    .all(|failure| formatter.render(failure).message == rendered_first.message)
            })
            .unwrap_or(false);
        let remaining_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
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
        let anchor_argument_from_mixed_failures = shared_argument_anchor.is_some()
            && !remaining_failures.is_empty()
            && remaining_failures_are_count_mismatches;
        let anchor_argument_from_all_failures =
            all_failures_are_argument_mismatches && shared_argument_anchor.is_some();
        let anchor_first_argument = identical_argument_failures
            && !remaining_failures.is_empty()
            && remaining_failures_are_count_mismatches
            || anchor_argument_from_mixed_failures
            || anchor_argument_from_all_failures;

        let anchor_kind = if literal_anchor.is_some() {
            DiagnosticAnchorKind::Exact
        } else if anchor_first_argument {
            shared_argument_anchor
                .or_else(|| self.first_call_argument_anchor(idx))
                .map(|_| DiagnosticAnchorKind::Exact)
                .unwrap_or(DiagnosticAnchorKind::OverloadPrimary)
        } else {
            DiagnosticAnchorKind::OverloadPrimary
        };
        let anchor_idx = if let Some(anchor_idx) = literal_anchor {
            anchor_idx
        } else if anchor_first_argument {
            shared_argument_anchor
                .or_else(|| self.first_call_argument_anchor(idx))
                .unwrap_or(idx)
        } else {
            idx
        };
        let Some(anchor) = self.resolve_diagnostic_anchor(anchor_idx, anchor_kind) else {
            return;
        };

        let mut related = Vec::new();
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

        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
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
