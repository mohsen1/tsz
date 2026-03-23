//! Function call error reporting (TS2345, TS2554, TS2769).
use crate::context::TypingRequest;
use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
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

        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
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
        let snap = self.ctx.snapshot_diagnostics();

        let ty = self.compute_type_of_node_with_request(expr_idx, &TypingRequest::NONE);

        self.ctx.rollback_diagnostics(&snap);
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

    fn object_literal_property_name_text(&self, prop_name_idx: NodeIndex) -> Option<String> {
        self.get_property_name(prop_name_idx)
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
            crate::query_boundaries::common::widen_type(self.ctx.types, arg_type)
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
                    .all(|display| tsz_solver::utils::is_numeric_literal_name(display));
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
                // tsc does not elaborate simple expression-bodied arrow functions.
                // It reports TS2345 at the call argument level, not TS2322 on the
                // return expression. Let the caller emit TS2345 instead.
                false
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
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                // Expression-bodied arrow: () => new Animal()
                // When the new-expression type isn't assignable to the expected
                // return type (e.g. Animal missing 'woof' required by Dog),
                // emit the assignability error at the expression position.
                // This matches tsc which emits TS2741 at `new Animal()` instead
                // of TS2345 on the whole callback.
                let body_type = self.get_type_of_node(func.body);
                if body_type == TypeId::ERROR
                    || body_type == TypeId::ANY
                    || expected_return_type == TypeId::ERROR
                    || expected_return_type == TypeId::ANY
                    || self.is_assignable_to(body_type, expected_return_type)
                {
                    return false;
                }
                self.error_type_not_assignable_at(body_type, expected_return_type, func.body);
                true
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
        use crate::query_boundaries::diagnostics::{
            callable_shape_for_type, function_shape, type_application,
        };

        if let (Some(non_nullish), Some(_nullish_cause)) = self.split_nullish_type(ty) {
            return self.first_callable_return_type(non_nullish);
        }

        if let Some(shape) = function_shape(self.ctx.types, ty) {
            return Some(shape.return_type);
        }

        if let Some(shape) = callable_shape_for_type(self.ctx.types, ty) {
            return shape.call_signatures.first().map(|sig| sig.return_type);
        }

        if let Some(app) = type_application(self.ctx.types, ty) {
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

        // When exactOptionalPropertyTypes is enabled and the failure is due to
        // exact optional property mismatch, don't elaborate per-property errors.
        // The caller will emit a top-level TS2375 instead.
        let source_type = self.get_type_of_node(arg_idx);
        if self.has_exact_optional_property_mismatch(source_type, param_type) {
            return false;
        }

        // Normalize optional/nullish wrappers (e.g., `{...} | undefined`).
        let effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };

        // Don't elaborate `never` targets — tsc emits a single TS2345 instead.
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

        let diagnostics_before_epc = self.ctx.diagnostics.len();
        self.check_object_literal_excess_properties(source_type, effective_param_type, arg_idx);
        let had_excess_property = self.ctx.diagnostics[diagnostics_before_epc..]
            .iter()
            .any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                )
            });
        if had_excess_property {
            return true;
        }

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
            let Some(prop_name) = self.object_literal_property_name_text(prop_name_idx) else {
                continue;
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

            // Get the type of the property value in the object literal.
            // Use the cached (contextually-typed) type for the assignability check.
            // This preserves literal types that were narrowed by contextual typing
            // (e.g., `value: "hello"` in a mapped type context stays as `"hello"`,
            // not widened to `string`).
            let is_function_value = self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                )
            });
            let source_prop_type = self.get_type_of_node(prop_value_idx);

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

            // Only try to elaborate sub-expression errors when the property value
            // is NOT assignable to the target. Without this guard, elaboration can
            // produce false-positive TS2322 errors on nested elements (e.g., array
            // literal elements) even when the overall property type is compatible.
            if source_prop_type != TypeId::ERROR
                && source_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self.is_assignable_to(source_prop_type, target_prop_type)
                && self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                    matches!(
                        node.kind,
                        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARROW_FUNCTION
                            | syntax_kind_ext::FUNCTION_EXPRESSION
                            | syntax_kind_ext::CONDITIONAL_EXPRESSION
                    )
                })
                && self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type)
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

        if !elaborated
            && self.should_suppress_object_literal_call_mismatch(source_type, effective_param_type)
        {
            return true;
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

            // For object/array literal elements, use contextually-typed type
            // to decide whether to elaborate (avoids false positives from widening).
            if matches!(
                elem_node.kind,
                syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            ) {
                let contextual_elem_type = self.get_type_of_node(elem_idx);
                if contextual_elem_type != TypeId::ERROR
                    && contextual_elem_type != TypeId::ANY
                    && target_element_type != TypeId::ERROR
                    && target_element_type != TypeId::ANY
                    && self.is_assignable_to(contextual_elem_type, target_element_type)
                {
                    // Element is contextually assignable — no error needed.
                    continue;
                }
                if self.try_elaborate_assignment_source_error(elem_idx, target_element_type) {
                    elaborated = true;
                    continue;
                }
                // Fall through to the non-object element check below.
            }

            // For function/conditional elements, try to elaborate without a guard.
            if matches!(
                elem_node.kind,
                syntax_kind_ext::ARROW_FUNCTION
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

    /// Elaborate object literal property mismatches for variable declarations.
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

    /// Elaborate array literal element mismatches for variable declarations.
    pub fn try_elaborate_initializer_elements(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
        init_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let init_node = match self.ctx.arena.get(init_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Only elaborate when the overall assignment fails.
        if self.is_assignable_to(init_type, declared_type) {
            return false;
        }

        // Arity mismatch — report at whole-assignment level, not per-element.
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
        // Suppress when types are identical or either is a special escape-hatch type.
        if arg_type == param_type
            || arg_type == TypeId::ERROR
            || param_type == TypeId::ERROR
            || arg_type == TypeId::ANY
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

        let arg_str = self.format_call_argument_type_for_diagnostic(arg_type, param_type, idx);
        let param_str = self.format_call_parameter_type_for_diagnostic(param_type, arg_type, idx);
        let message = format_message(
            diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            &[&arg_str, &param_str],
        );

        // Run failure analysis to produce elaboration as related information,
        // matching tsc's behavior of emitting TS2741/TS2739/TS2740 etc. as
        // related diagnostics under the primary TS2345.
        let analysis = self.analyze_assignability_failure(arg_type, param_type);
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
        let (start, length) =
            if let Some((s, l)) = self.resolve_excess_argument_span(args, expected_max) {
                (s, l)
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
        self.error_at_anchor(
            idx,
            DiagnosticAnchorKind::CallPrimary,
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
        let anchor_plain_call_argument =
            all_failures_are_argument_mismatches && !self.overload_callee_is_property_like(idx);
        let anchor_first_argument = identical_argument_failures
            && !remaining_failures.is_empty()
            && remaining_failures_are_count_mismatches
            || anchor_plain_call_argument;

        let anchor_kind = if literal_anchor.is_some() {
            DiagnosticAnchorKind::Exact
        } else if anchor_first_argument {
            self.first_call_argument_anchor(idx)
                .map(|_| DiagnosticAnchorKind::Exact)
                .unwrap_or(DiagnosticAnchorKind::OverloadPrimary)
        } else {
            DiagnosticAnchorKind::OverloadPrimary
        };
        let anchor_idx = if let Some(anchor_idx) = literal_anchor {
            anchor_idx
        } else if anchor_first_argument {
            self.first_call_argument_anchor(idx).unwrap_or(idx)
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

    fn first_call_argument_anchor(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        call.arguments.as_ref()?.nodes.first().copied()
    }

    fn overload_callee_is_property_like(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };

        matches!(
            callee_node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        )
    }

    fn overload_literal_argument_anchor(
        &mut self,
        idx: NodeIndex,
        failures: &[tsz_solver::PendingDiagnostic],
    ) -> Option<NodeIndex> {
        if failures.is_empty()
            || !failures.iter().all(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
        {
            return None;
        }

        let arg_idx = self.first_call_argument_anchor(idx)?;
        let mut shared_anchor = None;

        for failure in failures {
            let expected_type = match failure.args.as_slice() {
                [_, tsz_solver::DiagnosticArg::Type(expected_type)] => *expected_type,
                _ => return None,
            };
            let anchor = self.literal_argument_mismatch_anchor(arg_idx, expected_type)?;
            if let Some(existing) = shared_anchor {
                if existing != anchor {
                    return None;
                }
            } else {
                shared_anchor = Some(anchor);
            }
        }

        shared_anchor
    }

    fn literal_argument_mismatch_anchor(
        &mut self,
        source_idx: NodeIndex,
        target_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.object_literal_mismatch_anchor(expr_idx, target_type)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_mismatch_anchor(expr_idx, target_type)
            }
            _ => None,
        }
    }

    fn object_literal_mismatch_anchor(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::SubtypeFailureReason;

        let source_type = self.get_type_of_node(arg_idx);
        let effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };
        if effective_param_type == TypeId::NEVER {
            return None;
        }

        let arg_node = self.ctx.arena.get(arg_idx)?;
        let obj = self.ctx.arena.get_literal_expr(arg_node)?.clone();

        for &elem_idx in &obj.elements.nodes {
            let elem_node = match self.ctx.arena.get(elem_idx) {
                Some(node) => node,
                None => continue,
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                    (prop.name, prop.name)
                }
                _ => continue,
            };

            let prop_name = self.object_literal_property_name_text(prop_name_idx)?;
            let (target_prop_type, _) = self.object_literal_target_property_type(
                effective_param_type,
                prop_name_idx,
                &prop_name,
            )?;
            let source_prop_type = self.get_type_of_node(prop_value_idx);

            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                return self
                    .literal_argument_mismatch_anchor(prop_value_idx, target_prop_type)
                    .or(Some(prop_name_idx));
            }
        }

        let analysis = self.analyze_assignability_failure(source_type, effective_param_type);
        match analysis.failure_reason.as_ref() {
            Some(
                SubtypeFailureReason::MissingProperty { .. }
                | SubtypeFailureReason::MissingProperties { .. }
                | SubtypeFailureReason::OptionalPropertyRequired { .. },
            ) => Some(arg_idx),
            _ => None,
        }
    }

    fn array_literal_mismatch_anchor(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        if param_type == TypeId::NEVER {
            return None;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return None,
        };
        let arr = self.ctx.arena.get_literal_expr(arg_node)?.clone();
        let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            param_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let elem_node = match self.ctx.arena.get(elem_idx) {
                Some(node) => node,
                None => continue,
            };
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }

            let target_element_type = if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                t
            } else if let Some(t) = ctx_helper.get_array_element_type() {
                t
            } else {
                continue;
            };

            if let Some(anchor) =
                self.literal_argument_mismatch_anchor(elem_idx, target_element_type)
            {
                return Some(anchor);
            }

            let elem_type = self.elaboration_source_expression_type(elem_idx);
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                return Some(elem_idx);
            }
        }

        None
    }

    pub(super) fn is_concat_call(&self, expr: NodeIndex) -> bool {
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

#[cfg(test)]
#[path = "call_errors_tests.rs"]
mod tests;
