//! Function call error reporting (TS2345, TS2554, TS2769).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        match arg_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.try_elaborate_object_literal_properties(arg_idx, param_type)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.try_elaborate_array_literal_elements(arg_idx, param_type)
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
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(body_node) else {
                    return false;
                };
                self.try_elaborate_object_literal_arg_error(paren.expression, expected_return_type)
            }
            _ => false,
        }
    }

    fn first_callable_return_type(&self, ty: TypeId) -> Option<TypeId> {
        use tsz_solver::type_queries::{
            get_callable_shape, get_function_shape, get_type_application,
        };

        if let Some(shape) = get_function_shape(self.ctx.types, ty) {
            return Some(shape.return_type);
        }

        if let Some(shape) = get_callable_shape(self.ctx.types, ty) {
            return shape.call_signatures.first().map(|sig| sig.return_type);
        }

        if let Some(app) = get_type_application(self.ctx.types, ty) {
            return self.first_callable_return_type(app.base);
        }

        None
    }

    /// Elaborate object literal property type mismatches with TS2322.
    fn try_elaborate_object_literal_properties(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When the target type is `never`, don't elaborate into property-level TS2322 errors.
        // tsc emits a single TS2345 on the whole argument instead.
        if param_type == TypeId::NEVER {
            return false;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        let mut elaborated = false;

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Only elaborate regular property assignments and shorthand properties
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
                _ => continue,
            };

            // Get the property name string
            let prop_name = match self.ctx.arena.get_identifier_at(prop_name_idx) {
                Some(ident) => ident.escaped_text.clone(),
                None => continue,
            };

            // Look up the expected property type in the target parameter type
            let target_prop_type = match self
                .resolve_property_access_with_env(param_type, &prop_name)
            {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => type_id,
                _ => continue,
            };

            // Get the type of the property value in the object literal
            let source_prop_type = self.get_type_of_node(prop_value_idx);

            // Skip if types are unresolved
            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            // Check if the property value type is assignable to the target property type
            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                let source_prop_type_for_diagnostic =
                    if self.is_fresh_literal_expression(prop_value_idx) {
                        self.widen_literal_type(source_prop_type)
                    } else {
                        source_prop_type
                    };

                // Emit TS2322 on the property name node
                self.error_type_not_assignable_at_with_anchor(
                    source_prop_type_for_diagnostic,
                    target_prop_type,
                    prop_name_idx,
                );
                elaborated = true;
            }
        }

        elaborated
    }

    /// Elaborate array literal element type mismatches with TS2322.
    fn try_elaborate_array_literal_elements(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When the target type is `never`, don't elaborate into element-level TS2322 errors.
        if param_type == TypeId::NEVER {
            return false;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let arr = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(arr) => arr.clone(),
            None => return false,
        };

        let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            param_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        let mut elaborated = false;

        for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Skip spread elements
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }

            // Get the expected element type from the parameter array/tuple type
            let target_element_type = if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                t
            } else if let Some(t) = ctx_helper.get_array_element_type() {
                t
            } else {
                continue;
            };

            let elem_type = self.get_type_of_node(elem_idx);

            // Skip if types are unresolved
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                tracing::debug!(
                    "try_elaborate_array_literal_elements: elem_type = {:?}, target_element_type = {:?}, file = {}",
                    elem_type,
                    target_element_type,
                    self.ctx.file_name
                );
                self.error_type_not_assignable_at_with_anchor(
                    elem_type,
                    target_element_type,
                    elem_idx,
                );
                elaborated = true;
            }
        }

        elaborated
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
        // Suppress cascading errors when either type is ERROR, ANY, or UNKNOWN
        if arg_type == TypeId::ERROR || param_type == TypeId::ERROR {
            return;
        }
        if arg_type == TypeId::ANY || param_type == TypeId::ANY {
            return;
        }
        if arg_type == TypeId::UNKNOWN || param_type == TypeId::UNKNOWN {
            return;
        }
        let Some(loc) = self.get_source_location(idx) else {
            return;
        };
        // Avoid cascading TS2345 when an excess-property diagnostic (TS2353)
        // has already been reported within this argument object literal span.
        let arg_end = loc.start.saturating_add(loc.length());
        if self.ctx.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                && diag.start >= loc.start
                && diag.start < arg_end
        }) {
            return;
        }
        let arg_str = self.format_type_for_assignability_message(arg_type);
        let param_str = self.format_type_for_assignability_message(param_type);
        let message = format_message(
            diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            &[&arg_str, &param_str],
        );
        let mut diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
        );

        // Run failure analysis to produce elaboration as related information,
        // matching tsc's behavior of emitting TS2741/TS2739/TS2740 etc. as
        // related diagnostics under the primary TS2345.
        let analysis = self.analyze_assignability_failure(arg_type, param_type);
        if let Some(ref reason) = analysis.failure_reason
            && let Some(related) =
                self.build_related_from_failure_reason(reason, arg_type, param_type, idx)
        {
            diag.related_information.push(related);
        }

        self.ctx.diagnostics.push(diag);
    }

    /// Build a `DiagnosticRelatedInformation` from a solver failure reason.
    /// Returns `None` if the reason doesn't map to a related diagnostic.
    fn build_related_from_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        _source: TypeId,
        _target: TypeId,
        idx: NodeIndex,
    ) -> Option<DiagnosticRelatedInformation> {
        use tsz_solver::SubtypeFailureReason;

        let (start, length) = self.get_node_span(idx)?;

        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => {
                // Don't emit TS2741 for primitives, wrapper built-ins,
                // intersection targets, or private brand properties
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    return None;
                }
                let tgt_str = self.format_type(*target_type);
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    return None;
                }
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                if prop_name.starts_with("__private_brand") {
                    return None;
                }
                let src_str = self.format_type(*source_type);
                let msg = format_message(
                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop_name, &src_str, &tgt_str],
                );
                Some(DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length: length.saturating_sub(start),
                    message_text: msg,
                })
            }
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => {
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    return None;
                }
                let tgt_str = self.format_type(*target_type);
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    return None;
                }
                let src_str = self.format_type(*source_type);
                let names: Vec<String> = property_names
                    .iter()
                    .map(|a| self.ctx.types.resolve_atom_ref(*a).to_string())
                    .collect();
                let count = names.len();
                if count <= 4 {
                    // TS2739: Type 'X' is missing the following properties from type 'Y': a, b, c
                    let props_str = names.join(", ");
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        &[&src_str, &tgt_str, &props_str],
                    );
                    Some(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length: length.saturating_sub(start),
                        message_text: msg,
                    })
                } else {
                    // TS2740: Type 'X' is missing the following properties from type 'Y': a, b, c, and N more.
                    let shown: Vec<&str> = names.iter().take(4).map(|s| s.as_str()).collect();
                    let more = count - 4;
                    let props_str = format!("{}, and {} more.", shown.join(", "), more);
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        &[&src_str, &tgt_str, &props_str],
                    );
                    Some(DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length: length.saturating_sub(start),
                        message_text: msg,
                    })
                }
            }
            SubtypeFailureReason::PropertyTypeMismatch { .. } => {
                // Could elaborate property mismatches in the future, but skip for now
                None
            }
            _ => None,
        }
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
        let excess_loc = if got > expected_max && expected_max < args.len() {
            // Compute span from the first excess argument to the last argument.
            let first_excess = &args[expected_max];
            let last_arg = &args[args.len() - 1];
            let start_loc = self.get_source_location(*first_excess);
            let end_loc = self.get_source_location(*last_arg);
            match (start_loc, end_loc) {
                (Some(s), Some(e)) => Some((s.start, e.end.saturating_sub(s.start))),
                _ => None,
            }
        } else {
            None
        };

        let (start, length) = if let Some((s, l)) = excess_loc {
            (s, l)
        } else {
            let report_idx = self.call_error_anchor_node(idx);
            if let Some(loc) = self.get_source_location(report_idx) {
                (loc.start, loc.length())
            } else {
                return;
            }
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

    /// Report a spread argument type error (TS2556).
    /// TS2556: A spread argument must either have a tuple type or be passed to a rest parameter.
    pub fn error_spread_must_be_tuple_or_rest_at(&mut self, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), diagnostic_messages::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER.to_string(), diagnostic_codes::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER));
        }
    }

    /// Report an "expected at least N arguments" error (TS2555).
    /// TS2555: Expected at least {0} arguments, but got {1}.
    pub fn error_expected_at_least_arguments_at(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        let report_idx = self.call_error_anchor_node(idx);
        if let Some(loc) = self.get_source_location(report_idx) {
            let message = format!("Expected at least {expected_min} arguments, but got {got}.");
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT,
            ));
        }
    }

    /// Prefer callee name span for call-arity diagnostics.
    fn call_error_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return idx;
        }

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return idx;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return idx;
        };

        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
        {
            return access.name_or_argument;
        }

        call.expression
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

        // tsc reports TS2769 at the first argument position, not the call expression.
        // Fall back to the call expression itself if there are no arguments.
        let report_idx = self.ts2769_first_arg_or_call(idx);
        let Some(loc) = self.get_source_location(report_idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let mut related = Vec::new();
        let span =
            tsz_solver::SourceSpan::new(self.ctx.file_name.as_str(), loc.start, loc.length());

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

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
            category: DiagnosticCategory::Error,
            message_text: diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL.to_string(),
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: related,
        });
    }

    /// tsc reports TS2769 at the first argument node rather than the full call
    /// expression. This returns the first argument's `NodeIndex`, or falls back
    /// to the call expression itself when there are no arguments.
    fn ts2769_first_arg_or_call(&self, call_idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(call_idx) else {
            return call_idx;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return call_idx;
        };
        if let Some(args) = &call.arguments
            && let Some(&first) = args.nodes.first()
        {
            return first;
        }
        call_idx
    }

    fn should_suppress_concat_overload_error(&mut self, idx: NodeIndex) -> bool {
        use crate::query_boundaries::checkers::call::array_element_type_for_type;
        use crate::query_boundaries::common::contains_type_parameters;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if name_ident.escaped_text != "concat" {
            return false;
        }

        let Some(args) = &call.arguments else {
            return false;
        };
        if args.nodes.is_empty() {
            return false;
        }

        args.nodes.iter().all(|&arg_idx| {
            let arg_type = self.get_type_of_node(arg_idx);
            array_element_type_for_type(self.ctx.types, arg_type).is_some()
                && contains_type_parameters(self.ctx.types, arg_type)
        })
    }

    /// Report TS2693: type parameter used as value
    pub fn error_type_parameter_used_as_value(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            use tsz_common::diagnostics::diagnostic_codes;

            let message =
                format!("'{name}' only refers to a type, but is being used as a value here.");

            self.ctx.push_diagnostic(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            ));
        }
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
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
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

        if let Some(loc) = self.get_source_location(report_idx) {
            use crate::diagnostics::diagnostic_codes;
            self.ctx.diagnostics.push(
                crate::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?".to_string(),
                    diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE,
                ),
            );
        }
    }

    /// Report TS2348: "Value of type '{0}' is not callable. Did you mean to include 'new'?"
    /// This is specifically for class constructors called without 'new'.
    pub fn error_class_constructor_without_new_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                .replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message,
            diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW,
        ));
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
        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

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

        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message.to_string(),
            code,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source, check_source_diagnostics};

    /// Alias: default options already have `strict_null_checks: true`.
    fn check_source_with_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        check_source_diagnostics(source)
    }

    fn check_source_without_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        check_source(
            source,
            "test.ts",
            CheckerOptions {
                strict_null_checks: false,
                ..CheckerOptions::default()
            },
        )
    }

    #[test]
    fn emits_ts2721_for_calling_null() {
        let diagnostics = check_source_with_strict_null("null();");
        assert!(
            diagnostics.iter().any(|d| d.code == 2721),
            "Expected TS2721 for `null()`, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn emits_ts2722_for_calling_undefined() {
        let diagnostics = check_source_with_strict_null("undefined();");
        assert!(
            diagnostics.iter().any(|d| d.code == 2722),
            "Expected TS2722 for `undefined()`, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn emits_ts2723_for_calling_null_or_undefined() {
        let diagnostics = check_source_with_strict_null("let f: null | undefined;\nf();");
        assert!(
            diagnostics.iter().any(|d| d.code == 2723),
            "Expected TS2723 for calling `null | undefined`, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn emits_ts2349_without_strict_null_checks() {
        // Without strictNullChecks, null/undefined are in every type's domain,
        // so we should get TS2349 (not callable) instead of TS2721/2722/2723.
        let diagnostics = check_source_without_strict_null("null();");
        let has_2349 = diagnostics.iter().any(|d| d.code == 2349);
        let has_272x = diagnostics.iter().any(|d| (2721..=2723).contains(&d.code));
        assert!(
            has_2349 && !has_272x,
            "Expected TS2349 (not TS272x) without strictNullChecks, got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn emits_ts2722_for_optional_method_call() {
        // When an optional method is called without optional chaining,
        // its type includes undefined, so TS2722 should be emitted.
        let diagnostics = check_source_with_strict_null(
            r#"
interface Foo {
    optionalMethod?(x: number): string;
}
declare let foo: Foo;
foo.optionalMethod(1);
"#,
        );
        assert!(
            diagnostics.iter().any(|d| d.code == 2722),
            "Expected TS2722 for calling optional method without ?., got: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
}
