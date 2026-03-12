//! Function call error reporting (TS2345, TS2554, TS2769).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn widen_function_like_call_source(&mut self, type_id: TypeId) -> TypeId {
        let type_id = self.evaluate_type_with_env(type_id);
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);

        let widened = tsz_solver::operations::widening::widen_type(self.ctx.types, type_id);
        if widened != type_id {
            return widened;
        }

        if let Some(return_type) = get_function_return_type(self.ctx.types, type_id) {
            let widened_return = tsz_solver::widen_literal_type(self.ctx.types, return_type);
            if widened_return != return_type {
                let replaced =
                    replace_function_return_type(self.ctx.types, type_id, widened_return);
                if replaced != type_id {
                    return replaced;
                }
            }
        }

        type_id
    }

    fn should_prefer_property_target_type(
        &self,
        current: Option<TypeId>,
        candidate: TypeId,
    ) -> bool {
        if matches!(candidate, TypeId::ERROR | TypeId::ANY) {
            return false;
        }

        let Some(current) = current else {
            return true;
        };

        if matches!(current, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return true;
        }

        let current_has_type_params =
            tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, current);
        let candidate_has_type_params =
            tsz_solver::type_queries::contains_type_parameters_db(self.ctx.types, candidate);

        current_has_type_params && !candidate_has_type_params
    }

    fn elaboration_source_expression_type(&mut self, expr_idx: NodeIndex) -> TypeId {
        let prev_contextual = self.ctx.contextual_type;
        let diag_len = self.ctx.diagnostics.len();
        let emitted = self.ctx.emitted_diagnostics.clone();

        self.ctx.contextual_type = None;
        let ty = self.compute_type_of_node(expr_idx);
        self.ctx.contextual_type = prev_contextual;

        self.ctx.diagnostics.truncate(diag_len);
        self.ctx.emitted_diagnostics = emitted;
        ty
    }

    fn object_literal_target_property_type(
        &mut self,
        target_type: TypeId,
        prop_name_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<(TypeId, TypeId)> {
        let resolved_target = self.resolve_type_for_property_access(target_type);
        let evaluated_target = self.judge_evaluate(resolved_target);
        let contextual_target = self.evaluate_contextual_type(target_type);
        let mut contextual_property_type = None;
        let mut env_property_type = None;
        for candidate in [
            contextual_target,
            evaluated_target,
            resolved_target,
            target_type,
        ] {
            if let Some(property_type) =
                self.contextual_object_literal_property_type(candidate, prop_name)
                && self.should_prefer_property_target_type(contextual_property_type, property_type)
            {
                contextual_property_type = Some(property_type);
            }

            if let tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id, ..
            } = self.resolve_property_access_with_env(candidate, prop_name)
                && self.should_prefer_property_target_type(env_property_type, type_id)
            {
                env_property_type = Some(type_id);
            }
        }

        if let Some(type_id) = env_property_type.or(contextual_property_type) {
            let prop_atom = self.ctx.types.intern_string(prop_name);
            let declared_optional_type = [
                contextual_target,
                evaluated_target,
                resolved_target,
                target_type,
            ]
            .into_iter()
            .filter_map(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
            .find_map(|shape| {
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_atom && p.optional)
                    .map(|p| p.type_id)
            });

            let effective_type =
                if self.should_prefer_property_target_type(contextual_property_type, type_id) {
                    type_id
                } else {
                    contextual_property_type.unwrap_or(type_id)
                };
            return Some((
                effective_type,
                declared_optional_type.unwrap_or(effective_type),
            ));
        }

        let prop_node = self.ctx.arena.get(prop_name_idx)?;

        let prefer_number_index = prop_node.kind == SyntaxKind::NumericLiteral as u16;

        let index_value_type = [target_type, resolved_target, evaluated_target]
            .into_iter()
            .filter_map(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
            .find_map(|shape| {
                if prefer_number_index {
                    shape
                        .number_index
                        .as_ref()
                        .map(|sig| sig.value_type)
                        .or_else(|| shape.string_index.as_ref().map(|sig| sig.value_type))
                } else {
                    shape
                        .string_index
                        .as_ref()
                        .map(|sig| sig.value_type)
                        .or_else(|| shape.number_index.as_ref().map(|sig| sig.value_type))
                }
            })?;

        Some((index_value_type, index_value_type))
    }

    fn literal_call_argument_display(&self, arg_idx: NodeIndex) -> Option<String> {
        self.literal_expression_display(arg_idx)
    }

    fn format_call_argument_type_for_diagnostic(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> String {
        if self.is_literal_sensitive_assignment_target(param_type)
            && let Some(display) = self.literal_call_argument_display(arg_idx)
        {
            return display;
        }

        if let Some(display) =
            self.contextual_function_argument_display(arg_type, param_type, arg_idx)
        {
            return display;
        }

        let display_type = if param_type == TypeId::NEVER {
            let direct_arg_type = self.elaboration_source_expression_type(arg_idx);
            if direct_arg_type == TypeId::ERROR || direct_arg_type == arg_type {
                arg_type
            } else {
                direct_arg_type
            }
        } else {
            tsz_solver::widening::widen_type(self.ctx.types, arg_type)
        };
        self.format_type_for_assignability_message(display_type)
    }

    fn contextual_function_argument_display(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let func = self.ctx.arena.get_function(node)?;
        if !matches!(
            node.kind,
            k if k == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
        ) || !tsz_solver::type_queries::is_callable_type(self.ctx.types, arg_type)
        {
            return None;
        }

        let shape = tsz_solver::type_queries::get_function_shape(self.ctx.types, arg_type)?;
        let expected = self.evaluate_application_type(param_type);
        let expected = self.normalize_contextual_signature_with_env(expected);

        let mut rendered = Vec::with_capacity(func.parameters.nodes.len());
        for (index, &param_idx) in func.parameters.nodes.iter().enumerate() {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            let name = if let Some(name_node) = self.ctx.arena.get(param.name) {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    name_data.escaped_text.clone()
                } else if matches!(
                    name_node.kind,
                    k if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
                ) {
                    self.binding_name_for_signature_display(param.name)
                        .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
                        .unwrap_or_else(|| self.parameter_name_for_error(param.name))
                } else {
                    self.parameter_name_for_error(param.name)
                }
            } else {
                "_".to_string()
            };

            let optional = param.question_token || param.initializer.is_some();
            let rest = param.dot_dot_dot_token;

            let type_display = if param.type_annotation.is_some() {
                let annotated_type = self.get_type_from_type_node(param.type_annotation);
                self.format_type_for_assignability_message(annotated_type)
            } else if let Some(display) =
                self.contextual_rest_union_parameter_display(expected, index)
            {
                display
            } else if let Some(display) =
                self.contextual_generic_rest_parameter_display(expected, index, rest)
            {
                display
            } else {
                let type_id = self
                    .contextual_parameter_type_with_env_from_expected(expected, index, rest)
                    .or_else(|| shape.params.get(index).map(|param| param.type_id))
                    .unwrap_or(TypeId::ANY);
                self.format_type_for_assignability_message(type_id)
            };

            let type_display = if optional && !type_display.contains("undefined") {
                format!("{type_display} | undefined")
            } else {
                type_display
            };

            rendered.push(format!(
                "{}{}{}: {}",
                if rest { "..." } else { "" },
                name,
                if optional { "?" } else { "" },
                type_display
            ));
        }

        let return_display_type = if func.type_annotation.is_some() {
            shape.return_type
        } else {
            tsz_solver::widen_literal_type(self.ctx.types, shape.return_type)
        };

        Some(format!(
            "({}) => {}",
            rendered.join(", "),
            self.format_type_for_assignability_message(return_display_type)
        ))
    }

    fn contextual_generic_rest_parameter_display(
        &mut self,
        expected: TypeId,
        index: usize,
        is_rest: bool,
    ) -> Option<String> {
        let params = if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, expected)
        {
            shape.params.clone()
        } else {
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, expected)
                .and_then(|shape| shape.call_signatures.first().cloned())
                .map(|sig| sig.params)?
        };

        let last_param = params.last()?;
        if !last_param.rest {
            return None;
        }
        let rest_start = params.len().saturating_sub(1);
        if index < rest_start {
            return None;
        }
        if !crate::query_boundaries::assignability::contains_type_parameters(
            self.ctx.types,
            last_param.type_id,
        ) {
            return None;
        }

        let factory = self.ctx.types.factory();
        let display_type = if is_rest {
            let elem = factory.index_access(last_param.type_id, TypeId::NUMBER);
            factory.array(elem)
        } else {
            let offset = index - rest_start;
            let index_type = factory.literal_number(offset as f64);
            factory.index_access(last_param.type_id, index_type)
        };
        Some(self.format_type_for_assignability_message(display_type))
    }

    fn contextual_rest_union_parameter_display(
        &mut self,
        expected: TypeId,
        index: usize,
    ) -> Option<String> {
        let params = if let Some(shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, expected)
        {
            shape.params.clone()
        } else {
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, expected)
                .and_then(|shape| shape.call_signatures.first().cloned())
                .map(|sig| sig.params)?
        };

        let last_param = params.last()?;
        if !last_param.rest {
            return None;
        }
        let rest_start = params.len().saturating_sub(1);
        if index < rest_start {
            return None;
        }

        self.rest_union_member_display(last_param.type_id, index - rest_start)
    }

    fn rest_union_member_display(
        &mut self,
        rest_type: TypeId,
        rest_index: usize,
    ) -> Option<String> {
        let unwrapped = query_common::unwrap_readonly(self.ctx.types, rest_type);
        if let Some(members) = query_common::union_members(self.ctx.types, unwrapped) {
            let displays: Vec<String> = members
                .iter()
                .rev()
                .filter_map(|&member| self.rest_tuple_member_display(member, rest_index))
                .collect();
            let is_numeric_literal_union = displays.len() > 1
                && displays
                    .iter()
                    .all(|display| display.parse::<f64>().is_ok());
            if !is_numeric_literal_union {
                return None;
            }
            Some(displays.join(" | "))
        } else {
            None
        }
    }

    fn rest_tuple_member_display(&mut self, member: TypeId, rest_index: usize) -> Option<String> {
        let unwrapped = query_common::unwrap_readonly(self.ctx.types, member);
        if let Some(elements) = query_common::tuple_elements(self.ctx.types, unwrapped) {
            if let Some(element) = elements.get(rest_index) {
                return Some(self.format_type_for_assignability_message(element.type_id));
            }
            let last = elements.last()?;
            return last
                .rest
                .then(|| self.format_type_for_assignability_message(last.type_id));
        }

        query_common::array_element_type(self.ctx.types, unwrapped)
            .map(|element| self.format_type_for_assignability_message(element))
    }

    fn format_call_parameter_type_for_diagnostic(
        &mut self,
        param_type: TypeId,
        _arg_type: TypeId,
        arg_idx: NodeIndex,
    ) -> String {
        if let Some(display) = self.contextual_keyof_parameter_display(param_type, arg_idx) {
            return display;
        }

        if let Some(display) = self.contextual_constraint_parameter_display(param_type, arg_idx) {
            return display;
        }

        self.format_type_for_assignability_message(param_type)
    }

    fn contextual_keyof_parameter_display(
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

                    let same_key_space = (self.is_assignable_to(param_type, candidate_keyof)
                        && self.is_assignable_to(candidate_keyof, param_type))
                        || self.format_type_for_assignability_message(param_type)
                            == self.format_type_for_assignability_message(candidate_keyof);
                    if same_key_space {
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

    fn contextual_constraint_parameter_display(
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

                if let Some(shape) =
                    tsz_solver::type_queries::get_function_shape(self.ctx.types, callee_type)
                {
                    let sig = tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate.clone(),
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

                if let Some(signatures) =
                    tsz_solver::type_queries::get_call_signatures(self.ctx.types, callee_type)
                {
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
        let Some(type_param) = tsz_solver::type_param_info(self.ctx.types, raw_param.type_id)
        else {
            return;
        };
        let Some(raw_constraint) = type_param.constraint else {
            return;
        };

        let evaluated_constraint = self.evaluate_type_for_assignability(raw_constraint);
        let matches_evaluated = evaluated_constraint == evaluated_param
            || (self.is_assignable_to(evaluated_constraint, evaluated_param)
                && self.is_assignable_to(evaluated_param, evaluated_constraint));
        if !matches_evaluated {
            return;
        }

        let candidate = self.format_type_for_assignability_message(raw_constraint);
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
    pub(crate) fn try_elaborate_assignment_source_error(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
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
                    || self.is_assignable_to(branch_type, target_type)
                {
                    continue;
                }

                if self.try_elaborate_assignment_source_error(branch_idx, target_type) {
                    elaborated = true;
                    continue;
                }

                self.error_type_not_assignable_at_with_anchor(branch_type, target_type, branch_idx);
                elaborated = true;
            }

            return elaborated;
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
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION =>
            {
                let actual_return_type = self.get_type_of_node(func.body);
                if actual_return_type == TypeId::ERROR
                    || self.is_assignable_to(actual_return_type, expected_return_type)
                {
                    return false;
                }

                if self.try_elaborate_assignment_source_error(func.body, expected_return_type) {
                    return true;
                }

                self.error_type_not_assignable_at_with_anchor(
                    actual_return_type,
                    expected_return_type,
                    func.body,
                );
                true
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(body_node) else {
                    return false;
                };
                self.try_elaborate_object_literal_arg_error(paren.expression, expected_return_type)
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.try_elaborate_function_block_returns(func.body, expected_return_type)
            }
            _ => false,
        }
    }

    fn try_elaborate_function_block_returns(
        &mut self,
        block_idx: NodeIndex,
        expected_return_type: TypeId,
    ) -> bool {
        let Some(block_node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(block_node) else {
            return false;
        };

        let mut elaborated = false;
        for &stmt_idx in &block.statements.nodes {
            elaborated |=
                self.try_elaborate_return_statements_in_stmt(stmt_idx, expected_return_type);
        }
        elaborated
    }

    fn try_elaborate_return_statements_in_stmt(
        &mut self,
        stmt_idx: NodeIndex,
        expected_return_type: TypeId,
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
                self.try_elaborate_assignment_source_error(ret.expression, expected_return_type)
            }
            syntax_kind_ext::BLOCK => {
                self.try_elaborate_function_block_returns(stmt_idx, expected_return_type)
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(node) else {
                    return false;
                };
                let mut elaborated = self.try_elaborate_return_statements_in_stmt(
                    if_stmt.then_statement,
                    expected_return_type,
                );
                if if_stmt.else_statement.is_some() {
                    elaborated |= self.try_elaborate_return_statements_in_stmt(
                        if_stmt.else_statement,
                        expected_return_type,
                    );
                }
                elaborated
            }
            _ => false,
        }
    }

    fn first_callable_return_type(&mut self, ty: TypeId) -> Option<TypeId> {
        use tsz_solver::type_queries::{
            get_callable_shape, get_function_shape, get_type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.first_callable_return_type(non_nullish);
        }

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

        // Normalize optional/nullish parameter wrappers so object-literal elaboration
        // still reports property-level TS2322 for cases like `{...} | undefined`.
        let effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };

        // When the target type is `never`, don't elaborate into property-level TS2322 errors.
        // tsc emits a single TS2345 on the whole argument instead.
        if effective_param_type == TypeId::NEVER {
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

            let Some((target_prop_type, target_prop_type_for_diagnostic)) = self
                .object_literal_target_property_type(
                    effective_param_type,
                    prop_name_idx,
                    &prop_name,
                )
            else {
                continue;
            };

            // Get the type of the property value in the object literal
            let is_function_value = self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                )
            });
            let source_prop_type = if is_function_value {
                self.get_type_of_node(prop_value_idx)
            } else {
                self.elaboration_source_expression_type(prop_value_idx)
            };

            if is_function_value
                && target_prop_type != target_prop_type_for_diagnostic
                && source_prop_type != TypeId::ERROR
                && source_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self.is_assignable_to(source_prop_type, target_prop_type)
            {
                let source_prop_type_for_diagnostic =
                    self.widen_function_like_call_source(source_prop_type);
                self.error_type_not_assignable_at_with_anchor(
                    source_prop_type_for_diagnostic,
                    target_prop_type_for_diagnostic,
                    prop_name_idx,
                );
                elaborated = true;
                continue;
            }

            if self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::CONDITIONAL_EXPRESSION
                )
            }) && self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type)
            {
                elaborated = true;
                continue;
            }

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
                if self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type) {
                    elaborated = true;
                    continue;
                }

                let source_prop_type_for_diagnostic =
                    if self.is_fresh_literal_expression(prop_value_idx) {
                        self.widen_literal_type(source_prop_type)
                    } else {
                        source_prop_type
                    };

                // Emit TS2322 on the property name node, using the declared type
                // (without optional undefined) for the error message
                let source_prop_type_for_diagnostic =
                    self.widen_function_like_call_source(source_prop_type_for_diagnostic);
                self.error_type_not_assignable_at_with_anchor(
                    source_prop_type_for_diagnostic,
                    target_prop_type_for_diagnostic,
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

            let elem_type = self.elaboration_source_expression_type(elem_idx);

            if matches!(
                elem_node.kind,
                syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::CONDITIONAL_EXPRESSION
            ) && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
            {
                elaborated = true;
                continue;
            }

            // Skip if types are unresolved
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                if self.try_elaborate_assignment_source_error(elem_idx, target_element_type) {
                    elaborated = true;
                    continue;
                }

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

    /// Try to elaborate a variable initializer assignment failure by checking
    /// each property of an object literal against the target type's properties
    /// or index signatures.
    ///
    /// This is specifically for variable declarations (not function call arguments).
    /// Only handles `OBJECT_LITERAL_EXPRESSION` — arrays and functions are handled
    /// separately by `try_elaborate_initializer_elements`.
    ///
    /// Returns `true` if elaboration produced property-level diagnostics.
    pub fn try_elaborate_object_literal_properties_for_var_init(
        &mut self,
        init_idx: NodeIndex,
        declared_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return false;
        };

        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        self.try_elaborate_object_literal_properties(init_idx, declared_type)
    }

    /// Try to elaborate a variable initializer assignment failure with per-element errors.
    ///
    /// When a variable like `let x: [number, any] = [undefined, undefined]` fails assignability,
    /// tsc reports TS2322 on each mismatching element instead of on the whole assignment.
    /// This method first checks if the assignment would fail, then tries element-level elaboration.
    ///
    /// Returns `true` if elaboration handled the error (emitted element-level diagnostics),
    /// meaning the caller should NOT emit a generic TS2322.
    pub fn try_elaborate_initializer_elements(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
        init_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Check if initializer is an array literal
        let init_node = match self.ctx.arena.get(init_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Only elaborate array literal elements when the overall assignment fails.
        // Non-array initializers should return early above without triggering an
        // unrelated assignability relation on the full source/target types.
        if self.is_assignable_to(init_type, declared_type) {
            return false;
        }

        // When the source array has more elements than the target tuple,
        // the arity mismatch should be reported at the whole-assignment level,
        // not per-element. TSC reports "Type '[a, b, c, d]' is not assignable
        // to type '[a, b, c]'" for arity mismatches.
        if let Some(arr) = self.ctx.arena.get_literal_expr(init_node) {
            let source_count = arr.elements.nodes.len();
            if let Some(target_count) =
                tsz_solver::type_queries::get_fixed_tuple_length(self.ctx.types, declared_type)
                && source_count > target_count
            {
                return false;
            }
        }

        // Delegate to array literal element elaboration
        self.try_elaborate_array_literal_elements(init_idx, declared_type)
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
        let arg_str = self.format_call_argument_type_for_diagnostic(arg_type, param_type, idx);
        let param_str = self.format_call_parameter_type_for_diagnostic(param_type, arg_type, idx);
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

        self.ctx.push_diagnostic(diag);
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
                let tgt_str = self.format_type_diagnostic(*target_type);
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
                let widened = self.widen_type_for_display(*source_type);
                let src_str = self.format_type_diagnostic(widened);
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
                let tgt_str = self.format_type_diagnostic(*target_type);
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    return None;
                }
                let src_str = self.format_type_diagnostic(*source_type);
                // Filter out Object.prototype methods — they exist on every object
                // via prototype inheritance and should never appear as "missing".
                let names: Vec<String> = property_names
                    .iter()
                    .filter(|a| !is_object_prototype_method(&self.ctx.types.resolve_atom_ref(**a)))
                    .map(|a| self.ctx.types.resolve_atom_ref(*a).to_string())
                    .collect();
                if names.is_empty() {
                    return None;
                }
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
            SubtypeFailureReason::PropertyTypeMismatch { property_name, .. } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let msg = format_message(
                    diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    &[&prop_name],
                );
                Some(DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length: length.saturating_sub(start),
                    message_text: msg,
                })
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
        let report_idx = self.call_error_anchor_node(idx);
        let message = format!("Expected at least {expected_min} arguments, but got {got}.");
        self.error_at_node(
            report_idx,
            &message,
            diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT,
        );
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
            if let Some(arg_node) = self.ctx.arena.get(first)
                && arg_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && self.is_concat_call(call.expression)
                && let Some(array) = self.ctx.arena.get_literal_expr(arg_node)
                && let Some(&first_elem) = array.elements.nodes.first()
            {
                return first_elem;
            }
            return first;
        }
        call_idx
    }

    fn is_concat_call(&self, expr: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "concat")
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
