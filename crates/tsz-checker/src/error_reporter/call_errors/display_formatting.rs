//! Type display formatting helpers for call error diagnostics.

use crate::context::TypingRequest;
use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn sanitized_type_node_display(&mut self, type_node: NodeIndex) -> Option<String> {
        self.node_text(type_node)
            .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, true))
            .map(|text| self.format_annotation_like_type(&text))
    }

    fn explicit_callback_return_display_from_parameter(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.ctx.arena.get(func.body)?;
        let return_expr = if body_node.kind == syntax_kind_ext::BLOCK {
            let block = self.ctx.arena.get_block(body_node)?;
            block.statements.nodes.iter().rev().find_map(|&stmt_idx| {
                let stmt = self.ctx.arena.get(stmt_idx)?;
                let ret = self.ctx.arena.get_return_statement(stmt)?;
                ret.expression.into_option()
            })?
        } else {
            func.body
        };

        let return_name = self.ctx.arena.get_identifier_text(return_expr)?;
        func.parameters.nodes.iter().find_map(|&param_idx| {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            let param_name = self.ctx.arena.get_identifier_text(param.name)?;
            if param_name != return_name {
                return None;
            }
            if param.type_annotation.is_none() {
                return None;
            }
            let type_node = param.type_annotation;
            self.sanitized_type_node_display(type_node)
        })
    }

    fn replace_type_param_name_in_display(
        display: &str,
        param_name: &str,
        replacement: &str,
    ) -> String {
        let chars: Vec<char> = display.chars().collect();
        let needle: Vec<char> = param_name.chars().collect();
        let mut out = String::with_capacity(display.len() + replacement.len());
        let mut i = 0usize;

        while i < chars.len() {
            let matches = i + needle.len() <= chars.len()
                && chars[i..i + needle.len()] == needle[..]
                && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_')
                && (i + needle.len() == chars.len()
                    || !chars[i + needle.len()].is_alphanumeric()
                        && chars[i + needle.len()] != '_');

            if matches {
                out.push_str(replacement);
                i += needle.len();
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }

        out
    }

    fn explicit_type_argument_callback_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !crate::query_boundaries::common::contains_type_by_id(
            self.ctx.types,
            param_type,
            TypeId::ERROR,
        ) {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let (callee_expr, args, type_args): (
            NodeIndex,
            &[NodeIndex],
            &tsz_parser::parser::NodeList,
        ) = match parent.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                let call = self.ctx.arena.get_call_expr(parent)?;
                (
                    call.expression,
                    &call.arguments.as_ref()?.nodes,
                    call.type_arguments.as_ref()?,
                )
            }
            _ => return None,
        };
        if type_args.nodes.is_empty() {
            return None;
        }

        let arg_index = args.iter().position(|&candidate| candidate == arg_idx)?;
        let callee_type = self.get_type_of_node(callee_expr);
        let raw_sig = crate::query_boundaries::checkers::call::get_contextual_signature_for_arity(
            self.ctx.types,
            callee_type,
            args.len(),
        )?;
        if raw_sig.type_params.len() != type_args.nodes.len() {
            return None;
        }

        let raw_param_type = raw_sig
            .params
            .get(arg_index)
            .map(|param| param.type_id)
            .or_else(|| {
                let last = raw_sig.params.last()?;
                last.rest.then_some(last.type_id)
            })?;
        if !tsz_solver::type_queries::is_callable_type(self.ctx.types, raw_param_type) {
            return None;
        }

        let mut display = self.format_type_for_assignability_message(raw_param_type);
        for (tp, &arg_type_node) in raw_sig.type_params.iter().zip(type_args.nodes.iter()) {
            let replacement = self.sanitized_type_node_display(arg_type_node)?;
            let tp_name = self.ctx.types.resolve_atom_ref(tp.name);
            display = Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
        }

        Some(display)
    }

    fn contextual_function_parameter_display_with_annotation_fallback(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !crate::query_boundaries::common::contains_type_by_id(
            self.ctx.types,
            param_type,
            TypeId::ERROR,
        ) {
            return None;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let func = self.ctx.arena.get_function(node)?;
        if !matches!(
            node.kind,
            k if k == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
        ) || !tsz_solver::type_queries::is_callable_type(self.ctx.types, param_type)
        {
            return None;
        }

        let expected = self.evaluate_application_type(param_type);
        let expected = self.normalize_contextual_signature_with_env(expected);
        let shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                param_type,
            )
        })?;

        let mut rendered = Vec::with_capacity(shape.params.len());
        for (index, param) in shape.params.iter().enumerate() {
            let name = param
                .name
                .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
                .unwrap_or_else(|| format!("arg{index}"));
            let mut type_display = self.format_type_for_assignability_message(param.type_id);
            if type_display == "error"
                && let Some(&actual_param_idx) = func.parameters.nodes.get(index)
                && let Some(actual_param_node) = self.ctx.arena.get(actual_param_idx)
                && let Some(actual_param) = self.ctx.arena.get_parameter(actual_param_node)
                && actual_param.type_annotation.is_some()
            {
                type_display = self
                    .sanitized_type_node_display(actual_param.type_annotation)
                    .unwrap_or(type_display);
            }
            if param.optional && !type_display.contains("undefined") {
                type_display.push_str(" | undefined");
            }
            rendered.push(format!(
                "{}{}{}: {}",
                if param.rest { "..." } else { "" },
                name,
                if param.optional { "?" } else { "" },
                type_display
            ));
        }

        let mut return_display = self.format_type_for_assignability_message(shape.return_type);
        if return_display == "error" {
            return_display = self
                .explicit_callback_return_display_from_parameter(func)
                .unwrap_or(return_display);
        }

        let type_param_prefix = if shape.type_params.is_empty() {
            String::new()
        } else {
            let names = shape
                .type_params
                .iter()
                .map(|tp| self.ctx.types.resolve_atom_ref(tp.name).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("<{names}>")
        };

        Some(format!(
            "{}({}) => {}",
            type_param_prefix,
            rendered.join(", "),
            return_display
        ))
    }

    pub(in crate::error_reporter::call_errors) fn widen_function_like_call_source(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let type_id = self.evaluate_type_with_env(type_id);
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);

        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
        if widened != type_id {
            return widened;
        }

        if let Some(return_type) = get_function_return_type(self.ctx.types, type_id) {
            let widened_return =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, return_type);
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
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, current);
        let candidate_has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, candidate);

        current_has_type_params && !candidate_has_type_params
    }

    pub(in crate::error_reporter) fn elaboration_source_expression_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let snap = self.ctx.snapshot_diagnostics();

        let ty = self.compute_type_of_node_with_request(expr_idx, &TypingRequest::NONE);

        self.ctx.rollback_diagnostics(&snap);
        ty
    }

    pub(in crate::error_reporter) fn object_literal_target_property_type(
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
                    .map(|p| {
                        // tsc displays optional property types with `| undefined`
                        // in error messages (e.g., `IFoo[] | undefined` not just `IFoo[]`).
                        // Create a union with undefined if not already present.
                        if p.type_id == TypeId::UNDEFINED {
                            p.type_id
                        } else if let Some(list_id) =
                            tsz_solver::union_list_id(self.ctx.types, p.type_id)
                        {
                            let members = self.ctx.types.type_list(list_id);
                            if members.contains(&TypeId::UNDEFINED) {
                                p.type_id
                            } else {
                                self.ctx.types.union2(p.type_id, TypeId::UNDEFINED)
                            }
                        } else {
                            self.ctx.types.union2(p.type_id, TypeId::UNDEFINED)
                        }
                    })
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

        // For type parameters, also check the constraint for index signatures
        let constraint_target =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, target_type);

        let candidates: Vec<TypeId> = [target_type, resolved_target, evaluated_target]
            .into_iter()
            .chain(constraint_target)
            .chain(constraint_target.map(|c| self.resolve_type_for_property_access(c)))
            .chain(constraint_target.map(|c| self.judge_evaluate(c)))
            .collect();

        let index_value_type = candidates
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

    /// Check whether a target type has a named property matching ANY property
    /// in a source object literal.  Used to detect "index-signature-only"
    /// target types where per-property elaboration would produce confusing
    /// diagnostics.  Returns `true` if at least one source property matches a
    /// named target property (not an index signature).
    pub(in crate::error_reporter::call_errors) fn target_has_named_property_for_any_source_prop(
        &mut self,
        source_obj_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(source_obj_idx) else {
            return true; // conservative: assume named properties exist
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return true;
        };
        let obj = obj.clone();
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let prop_name = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|p| self.get_property_name(p.name)),
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|p| self.get_property_name(p.name)),
                _ => continue,
            };
            if let Some(name) = prop_name
                && self.target_has_named_property(&name, target_type)
            {
                return true;
            }
        }
        false
    }

    /// Check whether a target type has a named (non-index-signature) property
    /// with the given name.  Returns `true` when the target resolves to an
    /// object shape that contains a property entry whose name matches
    /// `prop_name`.  Returns `false` when the only path to `prop_name` goes
    /// through a string/number index signature.
    ///
    /// For union types, returns `true` if any member has the named property.
    fn target_has_named_property(&mut self, prop_name: &str, target_type: TypeId) -> bool {
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let resolved = self.resolve_type_for_property_access(target_type);
        let evaluated = self.judge_evaluate(resolved);
        for candidate in [target_type, resolved, evaluated] {
            // Check union members individually
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
            {
                for member in members {
                    if let Some(shape) =
                        tsz_solver::type_queries::get_object_shape(self.ctx.types, member)
                        && shape.properties.iter().any(|p| p.name == prop_atom)
                    {
                        return true;
                    }
                }
            }
            if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
                && shape.properties.iter().any(|p| p.name == prop_atom)
            {
                return true;
            }
        }
        false
    }

    pub(in crate::error_reporter) fn object_literal_property_name_text(
        &self,
        prop_name_idx: NodeIndex,
    ) -> Option<String> {
        self.get_property_name(prop_name_idx)
    }

    pub(in crate::error_reporter::call_errors) fn literal_call_argument_display(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        self.literal_expression_display(arg_idx)
    }

    fn zero_argument_call_list_display(&self, arg_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        if call
            .arguments
            .as_ref()
            .is_none_or(|args| args.nodes.is_empty())
        {
            Some("[]".to_string())
        } else {
            None
        }
    }

    pub(in crate::error_reporter::call_errors) fn format_call_argument_type_for_diagnostic(
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

        let mut display_type = if param_type == TypeId::NEVER {
            if let Some(display) = self.zero_argument_call_list_display(arg_idx) {
                return display;
            }
            let direct_arg_type = self.elaboration_source_expression_type(arg_idx);
            if direct_arg_type == TypeId::ERROR || direct_arg_type == arg_type {
                arg_type
            } else {
                direct_arg_type
            }
        } else {
            crate::query_boundaries::common::widen_type(self.ctx.types, arg_type)
        };

        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, display_type) {
            let evaluated_display = self.evaluate_type_for_assignability(display_type);
            if tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated_display)
                .is_some()
            {
                display_type = evaluated_display;
            }
        }
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
        let normalized_arg_type = self.evaluate_type_with_env(arg_type);
        let normalized_arg_type = self.resolve_type_for_property_access(normalized_arg_type);
        let normalized_arg_type = self.resolve_lazy_type(normalized_arg_type);
        let normalized_arg_type = self.evaluate_application_type(normalized_arg_type);
        let shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            normalized_arg_type,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                arg_type,
            )
        })?;
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
                let rendered_annotated = self.format_type_for_assignability_message(annotated_type);
                if rendered_annotated == "error" {
                    self.sanitized_type_node_display(param.type_annotation)
                        .unwrap_or(rendered_annotated)
                } else {
                    rendered_annotated
                }
            } else if let Some(display) =
                self.contextual_rest_union_parameter_display(expected, index)
            {
                display
            } else if let Some(display) =
                self.contextual_generic_rest_parameter_display(expected, index, rest)
            {
                display
            } else if matches!(
                self.ctx.arena.get(param.name).map(|node| node.kind),
                Some(k)
                    if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
            ) {
                let type_id = self
                    .contextual_parameter_type_with_env_from_expected(expected, index, rest)
                    .or_else(|| shape.params.get(index).map(|param| param.type_id))
                    .unwrap_or(TypeId::ANY);
                if matches!(type_id, TypeId::ANY | TypeId::UNKNOWN) {
                    self.binding_pattern_parameter_type_display(param.name)
                        .unwrap_or_else(|| self.format_type_for_assignability_message(type_id))
                } else {
                    self.format_type_for_assignability_message(type_id)
                }
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

        let return_display = if func.type_annotation.is_some() {
            self.format_type_for_assignability_message(shape.return_type)
        } else if func.asterisk_token {
            let generator_name = if func.is_async {
                "AsyncGenerator"
            } else {
                "Generator"
            };
            let yield_type = self
                .get_generator_yield_type_argument(shape.return_type)
                .unwrap_or(TypeId::ANY);
            let return_type = self
                .get_generator_return_type_argument(shape.return_type)
                .filter(|ty| !matches!(*ty, TypeId::UNKNOWN | TypeId::ERROR))
                .unwrap_or(TypeId::VOID);
            let next_type = self
                .get_generator_next_type_argument(shape.return_type)
                .filter(|ty| !matches!(*ty, TypeId::UNKNOWN | TypeId::ERROR))
                .unwrap_or(TypeId::ANY);
            format!(
                "{generator_name}<{}, {}, {}>",
                self.format_type_for_assignability_message(yield_type),
                self.format_type_for_assignability_message(return_type),
                self.format_type_for_assignability_message(next_type)
            )
        } else {
            let return_display_type = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                shape.return_type,
            );
            let rendered_return = self.format_type_for_assignability_message(return_display_type);
            if rendered_return == "error" {
                self.explicit_callback_return_display_from_parameter(func)
                    .unwrap_or(rendered_return)
            } else {
                rendered_return
            }
        };
        let type_param_prefix = if shape.type_params.is_empty() {
            String::new()
        } else {
            let names = shape
                .type_params
                .iter()
                .map(|tp| self.ctx.types.resolve_atom_ref(tp.name).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("<{names}>")
        };

        Some(format!(
            "{}({}) => {}",
            type_param_prefix,
            rendered.join(", "),
            return_display
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

    pub(in crate::error_reporter::call_errors) fn format_call_parameter_type_for_diagnostic(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
        arg_idx: NodeIndex,
    ) -> String {
        if let Some(display) =
            self.expanded_rest_tuple_parameter_display_for_call(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) = self.contextual_generic_call_parameter_display(param_type, arg_idx) {
            return display;
        }

        if let Some(display) =
            self.contextual_function_parameter_display_with_annotation_fallback(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) =
            self.explicit_type_argument_callback_parameter_display(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) = self.contextual_keyof_parameter_display(param_type, arg_idx) {
            return display;
        }

        if let Some(display) = self.contextual_constraint_parameter_display(param_type, arg_idx) {
            return display;
        }

        if let Some(display) =
            self.contextual_generic_mapped_parameter_display(param_type, arg_type, arg_idx)
        {
            return display;
        }

        if query_common::type_application(self.ctx.types, param_type).is_some() {
            return self.format_type_diagnostic(param_type);
        }

        self.format_type_for_assignability_message(param_type)
    }

    fn expanded_rest_tuple_parameter_display_for_call(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(arg_idx)?;
        let call_idx = if node.kind == syntax_kind_ext::CALL_EXPRESSION
            || node.kind == syntax_kind_ext::NEW_EXPRESSION
        {
            arg_idx
        } else {
            let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
            let parent = self.ctx.arena.get(parent_idx)?;
            let is_call_like = parent.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent.kind == syntax_kind_ext::NEW_EXPRESSION;
            let call = is_call_like
                .then(|| self.ctx.arena.get_call_expr(parent))
                .flatten()?;
            (call.expression == arg_idx).then_some(parent_idx)?
        };
        let call_node = self.ctx.arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION
            && call_node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return None;
        }

        self.format_variadic_tuple_display_without_alias(param_type)
    }

    fn format_variadic_tuple_display_without_alias(&mut self, type_id: TypeId) -> Option<String> {
        let mut resolved = self.evaluate_type_with_env(type_id);
        resolved = self.resolve_type_for_property_access(resolved);
        resolved = self.resolve_lazy_type(resolved);
        resolved = self.evaluate_application_type(resolved);
        let readonly = tsz_solver::readonly_inner_type(self.ctx.types, resolved).is_some();
        resolved = query_common::unwrap_readonly(self.ctx.types, resolved);
        let elements = query_common::tuple_elements(self.ctx.types, resolved)?;
        if !elements.iter().any(|element| element.rest) {
            return None;
        }

        let parts: Vec<String> = elements
            .iter()
            .map(|element| {
                let normalized = self.normalize_assignability_display_type(element.type_id);
                let display = self.format_type_diagnostic(normalized);
                match (element.rest, element.name, element.optional) {
                    (true, Some(name), _) => {
                        let name = self.ctx.types.resolve_atom_ref(name);
                        format!("...{name}: {display}")
                    }
                    (true, None, _) => format!("...{display}"),
                    (false, Some(name), true) => {
                        let name = self.ctx.types.resolve_atom_ref(name);
                        format!("{name}?: {display}")
                    }
                    (false, Some(name), false) => {
                        let name = self.ctx.types.resolve_atom_ref(name);
                        format!("{name}: {display}")
                    }
                    (false, None, true) => format!("{display}?"),
                    (false, None, false) => display,
                }
            })
            .collect();
        let tuple_display = format!("[{}]", parts.join(", "));

        Some(if readonly {
            format!("readonly {tuple_display}")
        } else {
            tuple_display
        })
    }

    fn contextual_generic_call_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !crate::query_boundaries::common::contains_type_by_id(
            self.ctx.types,
            param_type,
            TypeId::UNKNOWN,
        ) {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let (callee_expr, args): (NodeIndex, &[NodeIndex]) = match parent.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.ctx.arena.get_call_expr(parent)?;
                let args = call.arguments.as_ref()?;
                (call.expression, &args.nodes)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                let new_expr = self.ctx.arena.get_call_expr(parent)?;
                let args = new_expr.arguments.as_ref()?;
                (new_expr.expression, &args.nodes)
            }
            _ => return None,
        };
        let arg_index = args.iter().position(|&candidate| candidate == arg_idx)?;
        let callee_type = self.get_type_of_node(callee_expr);
        let raw_param_type =
            crate::query_boundaries::checkers::call::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type,
                args.len(),
            )
            .and_then(|shape| {
                shape
                    .params
                    .get(arg_index)
                    .map(|param| param.type_id)
                    .or_else(|| {
                        let last = shape.params.last()?;
                        last.rest.then_some(last.type_id)
                    })
            })?;

        if !crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            raw_param_type,
        ) {
            return None;
        }

        if !self.should_preserve_raw_generic_call_parameter_display(arg_idx, raw_param_type) {
            return None;
        }

        Some(self.format_type_for_assignability_message(raw_param_type))
    }

    fn should_preserve_raw_generic_call_parameter_display(
        &mut self,
        arg_idx: NodeIndex,
        raw_param_type: TypeId,
    ) -> bool {
        let mut child = arg_idx;
        let Some(mut current) = self.ctx.arena.get_extended(arg_idx).map(|ext| ext.parent) else {
            return false;
        };

        while current.is_some() {
            let parent_idx = current;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::IF_STATEMENT {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(parent) else {
                    return false;
                };
                if child != if_stmt.then_statement && child != if_stmt.else_statement {
                    return false;
                }

                let mut positive_branch = child == if_stmt.then_statement;
                let mut condition = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(if_stmt.expression);
                if let Some(node) = self.ctx.arena.get(condition)
                    && let Some(unary) = self.ctx.arena.get_unary_expr(node)
                    && unary.operator == SyntaxKind::ExclamationToken as u16
                {
                    positive_branch = !positive_branch;
                    condition = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(unary.operand);
                }

                if positive_branch {
                    return false;
                }

                let Some(cond_node) = self.ctx.arena.get(condition) else {
                    return false;
                };
                let Some(call) = self.ctx.arena.get_call_expr(cond_node) else {
                    return false;
                };
                let Some(args) = call.arguments.as_ref() else {
                    return false;
                };
                let callee_type = self.get_type_of_node(call.expression);
                let Some(predicate) =
                    crate::query_boundaries::checkers::call::extract_predicate_signature(
                        self.ctx.types,
                        callee_type,
                    )
                else {
                    return false;
                };
                let Some(predicate_type) = predicate.predicate.type_id else {
                    return false;
                };
                let Some(predicate_arg) = predicate
                    .predicate
                    .parameter_index
                    .and_then(|index| args.nodes.get(index).copied())
                else {
                    return false;
                };
                if !self.same_reference_symbol(predicate_arg, arg_idx) {
                    return false;
                }

                return self.types_overlap_for_diagnostic_display(predicate_type, raw_param_type);
            }

            child = parent_idx;
            let Some(next) = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent)
            else {
                return false;
            };
            current = next;
        }

        false
    }

    fn same_reference_symbol(&self, left: NodeIndex, right: NodeIndex) -> bool {
        if left == right {
            return true;
        }

        let left = self.ctx.arena.skip_parenthesized_and_assertions(left);
        let right = self.ctx.arena.skip_parenthesized_and_assertions(right);
        if left == right {
            return true;
        }

        self.ctx
            .binder
            .resolve_identifier(self.ctx.arena, left)
            .zip(self.ctx.binder.resolve_identifier(self.ctx.arena, right))
            .is_some_and(|(a, b)| a == b)
    }

    fn types_overlap_for_diagnostic_display(&mut self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable_to(left, right) || self.is_assignable_to(right, left)
    }
}
