//! Call parameter display helpers for call diagnostics.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn conditional_callable_union_argument_display(
        &mut self,
        arg_type: TypeId,
    ) -> Option<String> {
        let members = query_common::union_members(self.ctx.types, arg_type)?;
        let mut parts = Vec::with_capacity(members.len());
        for member in members {
            if let Some(shape) = query_common::function_shape_for_type(self.ctx.types, member) {
                let params = shape
                    .params
                    .iter()
                    .map(|param| {
                        let name = param
                            .name
                            .map(|name| self.ctx.types.resolve_atom(name))
                            .unwrap_or_else(|| "_".to_string());
                        let optional = if param.optional { "?" } else { "" };
                        let rest = if param.rest { "..." } else { "" };
                        let ty = self.format_type_for_assignability_message(param.type_id);
                        format!("{rest}{name}{optional}: {ty}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type = self.format_type_for_assignability_message(shape.return_type);
                parts.push(format!("(({params}) => {return_type})"));
            } else {
                parts.push(self.format_type_for_assignability_message(member));
            }
        }

        Some(parts.join(" | "))
    }

    pub(in crate::error_reporter::call_errors) fn call_target_preserves_literal_argument_surface(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if self.enclosing_call_parameter_is_optional_non_rest(arg_idx) {
            return false;
        }
        let evaluated = self.evaluate_type_for_assignability(param_type);
        Self::union_has_primitive_members_only(self.ctx.types, param_type)
            || Self::union_has_primitive_members_only(self.ctx.types, evaluated)
    }

    fn union_has_primitive_members_only(
        types: &dyn tsz_solver::construction::TypeDatabase,
        ty: TypeId,
    ) -> bool {
        let Some(members) = query_common::union_members(types, ty) else {
            return false;
        };
        members
            .iter()
            .all(|&m| query_common::is_primitive_type(types, m))
    }

    /// Returns `true` when `param_type` is a union whose literal-sensitive
    /// members are limited to the synthetic `undefined` introduced by an
    /// optional parameter (`b?: T`) — i.e. dropping `undefined` from the
    /// union leaves a non-literal-sensitive target. In that case the call
    /// argument display should widen to the underlying target rather than
    /// preserve the literal text, matching tsc's diagnostic surface.
    pub(in crate::error_reporter::call_errors) fn literal_sensitivity_is_only_synthetic_optional_undefined(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if !self.enclosing_call_parameter_is_optional_non_rest(arg_idx) {
            return false;
        }
        let stripped = match query_common::union_members(self.ctx.types, param_type) {
            Some(members) => {
                let kept: Vec<TypeId> = members
                    .into_iter()
                    .filter(|m| *m != TypeId::UNDEFINED)
                    .collect();
                if kept.is_empty() {
                    return false;
                }
                self.ctx.types.factory().union_preserve_members(kept)
            }
            None => return false,
        };
        !self.is_literal_sensitive_assignment_target(stripped)
    }

    pub(in crate::error_reporter::call_errors) fn contextual_function_argument_display(
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
        ) || !crate::query_boundaries::common::is_callable_type(self.ctx.types, arg_type)
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
            let contextual_type_id = self.contextual_function_argument_parameter_display_type(
                arg_idx, expected, index, param, &shape,
            );

            let (type_display, display_type_id) = if param.type_annotation.is_some() {
                let annotated_type = self.get_type_from_type_node(param.type_annotation);
                let rendered_annotated = self.format_type_for_assignability_message(annotated_type);
                let display = if rendered_annotated == "error" {
                    self.sanitized_type_node_display(param.type_annotation)
                        .unwrap_or(rendered_annotated)
                } else {
                    rendered_annotated
                };
                (display, annotated_type)
            } else if let Some(display) =
                self.contextual_rest_union_parameter_display(expected, index)
            {
                (display, contextual_type_id)
            } else if let Some(display) =
                self.contextual_generic_rest_parameter_display(expected, index, rest)
            {
                (display, contextual_type_id)
            } else if matches!(
                self.ctx.arena.get(param.name).map(|node| node.kind),
                Some(k)
                    if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN
            ) {
                let display = if matches!(contextual_type_id, TypeId::ANY | TypeId::UNKNOWN) {
                    self.binding_pattern_parameter_type_display(param.name)
                        .unwrap_or_else(|| {
                            self.format_type_for_assignability_message(contextual_type_id)
                        })
                } else {
                    self.format_type_for_assignability_message(contextual_type_id)
                };
                (display, contextual_type_id)
            } else {
                (
                    self.format_type_for_assignability_message(contextual_type_id),
                    contextual_type_id,
                )
            };

            let type_display = if optional {
                self.optional_parameter_type_display(type_display, display_type_id)
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
                .filter(|ty| !ty.is_unknown_or_error())
                .unwrap_or(TypeId::VOID);
            let next_type = self
                .get_generator_next_type_argument(shape.return_type)
                .filter(|ty| !ty.is_unknown_or_error())
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
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, expected)
        {
            shape.params.clone()
        } else {
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, expected)
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
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, expected)
        {
            shape.params.clone()
        } else {
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, expected)
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

    pub(in crate::error_reporter) fn format_call_parameter_type_for_diagnostic(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
        arg_idx: NodeIndex,
        strip_noinfer_for_mismatch: bool,
    ) -> String {
        let direct_param_display = self.format_type_diagnostic(param_type);

        if let Some(display) = self.overloaded_recursive_typeof_parameter_display(param_type) {
            return display;
        }

        if let Some(display) =
            self.constrained_variadic_tuple_parameter_display(param_type, arg_type)
        {
            return display;
        }

        if let Some(display) =
            self.underfilled_generic_variadic_tuple_parameter_display(param_type, arg_type)
        {
            return display;
        }

        if let Some(display) =
            self.expanded_rest_tuple_parameter_display_for_call(param_type, arg_idx)
        {
            return display;
        }

        if strip_noinfer_for_mismatch
            && let Some(display) =
                self.noinfer_call_parameter_mismatch_display(param_type, arg_type)
        {
            return display;
        }

        if let Some(display) = self.contextual_generic_call_parameter_display(param_type, arg_idx)
            && (!display.starts_with('{') || direct_param_display.starts_with('{'))
        {
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

        if let Some(display) = self.generic_call_parameter_alias_display(param_type, arg_idx)
            && (display.contains("IterableIterator<") || !display.contains("IterableIterator"))
            && (display.contains("Promise<") || !display.contains("Promise"))
            && (!display.starts_with('{') || direct_param_display.starts_with('{'))
        {
            return self.strip_synthetic_optional_from_display_for_arg(display, arg_type);
        }

        if let Some(display) =
            self.property_access_call_parameter_annotation_display(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) =
            self.property_access_call_parameter_annotation_display(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) = self.instantiated_call_parameter_display(arg_idx) {
            return self.strip_synthetic_optional_from_display_for_arg(display, arg_type);
        }

        if let Some(display_type) =
            self.materialize_finite_mapped_call_parameter_display_type(param_type)
        {
            return self.format_type_for_assignability_message(display_type);
        }

        if let Some(display) = self.contextual_constraint_parameter_display(param_type, arg_idx) {
            return display;
        }

        if let Some(display) =
            self.simple_function_call_parameter_annotation_display(param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) =
            self.contextual_generic_mapped_parameter_display(param_type, arg_type, arg_idx)
        {
            return display;
        }

        if direct_param_display.contains("typeof import(") {
            return direct_param_display;
        }

        if query_common::type_application(self.ctx.types, param_type).is_some() {
            // tsc shows the resolved literal/primitive form (e.g. `'"b"'`) instead
            // of the alias (e.g. `'KeysExtendedBy<M, number>'`) when a generic
            // type-alias application reduces to a literal, primitive, or union of
            // those. Object/interface results keep the alias form.
            let original_contains_type_parameters =
                query_common::contains_type_parameters(self.ctx.types, param_type);
            let evaluated = self.evaluate_type_with_env(param_type);
            if evaluated != param_type
                && query_common::is_literal_or_primitive_or_compound_of_those(
                    self.ctx.types,
                    evaluated,
                )
            {
                return self.format_type_diagnostic(evaluated);
            }
            if evaluated != param_type
                && evaluated != TypeId::ERROR
                && !matches!(evaluated, TypeId::ANY | TypeId::UNKNOWN)
                && !original_contains_type_parameters
                && !query_common::contains_type_parameters(self.ctx.types, evaluated)
            {
                return self.format_type_diagnostic(evaluated);
            }
            return self.format_type_diagnostic(param_type);
        }

        if let Some(display) = self.non_tuple_spread_optional_parameter_display(param_type, arg_idx)
        {
            return display;
        }

        if self.enclosing_call_parameter_is_optional_non_rest(arg_idx) {
            // Widen the param type with `| undefined` to model the optional
            // surface, then defer to the strip-aware formatter:
            //   - When stripping `| undefined` leaves at least one non-nullish
            //     member, tsc elides the synthetic surface (e.g. `number | undefined`
            //     against `string` arg renders as `number`).
            //   - When stripping leaves the union empty (the underlying type is
            //     already nullish, e.g. `null` from `function f(x = null)`), tsc
            //     keeps the full union (`null | undefined`) — the strip helper
            //     declines to strip in that case and the surface is preserved.
            // This matches tsc's behaviour for both `f(x = null)` (renders
            // `null | undefined`) and `fn.apply` (renders `[base: any, ...args:
            // any[]]` without the synthetic `| undefined`).
            let widened_param_type = if param_type == TypeId::UNDEFINED
                || query_common::union_list_id(self.ctx.types, param_type).is_some_and(|list_id| {
                    self.ctx
                        .types
                        .type_list(list_id)
                        .contains(&TypeId::UNDEFINED)
                }) {
                param_type
            } else {
                self.ctx.types.union2(param_type, TypeId::UNDEFINED)
            };
            let display = self.format_assignability_type_for_message(widened_param_type, arg_type);
            return self.strip_synthetic_optional_from_display_for_arg(display, arg_type);
        }

        // When the parameter is a union mixing a non-primitive member (e.g.
        // `object`) with `null`/`undefined`, tsc strips the nullish members in
        // the TS2345 target display (`object | null` → `object`). When every
        // remaining member after stripping is a primitive (e.g.
        // `boolean | null | undefined` → `boolean`), tsc preserves the full
        // union — the structural rule is "strip only when the result contains
        // at least one non-primitive member."
        if let Some(stripped) =
            self.strip_nullish_for_non_primitive_union_target(param_type, arg_type)
        {
            return self.format_type_for_assignability_message(stripped);
        }

        let fallback =
            self.format_assignability_type_for_message_preserving_nullish(param_type, arg_type);
        if fallback.starts_with('{') && !direct_param_display.starts_with('{') {
            direct_param_display
        } else {
            fallback
        }
    }

    fn overloaded_recursive_typeof_parameter_display(
        &mut self,
        param_type: TypeId,
    ) -> Option<String> {
        if !query_common::is_type_query_type(self.ctx.types, param_type) {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(param_type);
        if evaluated == param_type || evaluated == TypeId::ERROR {
            return None;
        }
        let shape = query_common::callable_shape_for_type(self.ctx.types, evaluated)?;
        if shape.call_signatures.len() <= 1
            || !query_common::function_signature_has_typeof(self.ctx.types, evaluated)
        {
            return None;
        }
        let mut formatter =
            tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                .with_diagnostic_mode()
                .with_preserve_optional_parameter_surface_syntax(true)
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                );
        Some(formatter.format(evaluated).into_owned())
    }

    pub(in crate::error_reporter::call_errors) fn materialize_finite_mapped_call_parameter_display_type(
        &mut self,
        param_type: TypeId,
    ) -> Option<TypeId> {
        let display_type = self.evaluate_type_for_assignability(param_type);
        let constraint = query_common::type_param_info(self.ctx.types, param_type)
            .and_then(|info| info.constraint);
        let constraint_display_type =
            constraint.map(|constraint| self.evaluate_type_for_assignability(constraint));
        let mapped_id = query_common::mapped_type_id(self.ctx.types, display_type)
            .or_else(|| query_common::mapped_type_id(self.ctx.types, param_type))
            .or_else(|| {
                constraint
                    .and_then(|constraint| query_common::mapped_type_id(self.ctx.types, constraint))
            })
            .or_else(|| {
                constraint_display_type.and_then(|display_type| {
                    query_common::mapped_type_id(self.ctx.types, display_type)
                })
            })?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let names = crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
            self.ctx.types,
            mapped_id,
        )?;
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|a, b| {
            self.ctx
                .types
                .resolve_atom_ref(*a)
                .cmp(&self.ctx.types.resolve_atom_ref(*b))
        });

        let mut properties = Vec::with_capacity(names.len());
        for name in names {
            let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
            let type_id =
                crate::query_boundaries::state::checking::get_finite_mapped_property_display_type(
                    self.ctx.types,
                    mapped_id,
                    &property_name,
                )?;
            let mut property = tsz_solver::PropertyInfo::new(name, type_id);
            property.optional = mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add);
            property.readonly = mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add);
            properties.push(property);
        }

        Some(self.ctx.types.factory().object(properties))
    }

    fn simple_function_call_parameter_annotation_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if self.object_literal_is_missing_required_target_property(arg_idx, param_type) {
            return None;
        }

        let computed_display = self.format_type_for_assignability_message(param_type);
        if !computed_display.starts_with('{') && computed_display != "[]" {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let call = self.ctx.arena.get_call_expr(parent)?;
        let arg_index = call
            .arguments
            .as_ref()?
            .nodes
            .iter()
            .position(|&n| n == arg_idx)?;
        let callee_sym = self
            .resolve_identifier_symbol(call.expression)
            .or_else(|| self.resolve_qualified_symbol(call.expression))?;
        let callee = self.ctx.binder.get_symbol(callee_sym)?;

        callee.declarations.iter().copied().find_map(|decl_idx| {
            let node = self.ctx.arena.get(decl_idx)?;
            let func = self.ctx.arena.get_function(node)?;
            let param_idx = *func.parameters.nodes.get(arg_index)?;
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            param
                .type_annotation
                .into_option()
                .and_then(|annotation| self.sanitized_type_node_display(annotation))
        })
    }

    fn strip_nullish_for_non_primitive_union_target(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<TypeId> {
        let stripped = self.strip_nullish_for_assignability_display(param_type, arg_type)?;
        let stripped_members = query_common::union_members(self.ctx.types, stripped);
        let has_non_primitive = match stripped_members {
            Some(members) => members
                .iter()
                .any(|&m| !query_common::is_primitive_type(self.ctx.types, m)),
            None => !query_common::is_primitive_type(self.ctx.types, stripped),
        };
        if has_non_primitive {
            Some(stripped)
        } else {
            None
        }
    }

    /// When the argument is a non-tuple spread (e.g. `...mixed` where
    /// `mixed: (number|string)[]`) landing on an optional non-rest parameter,
    /// tsc displays the parameter type widened with `| undefined`. A non-tuple
    /// variadic spread may leave the position unfilled, so the parameter's
    /// optional nature surfaces in the error message. Tuple spreads have
    /// definite arity and regular arguments definitely fill their slot, so
    /// neither case widens.
    fn non_tuple_spread_optional_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        use crate::query_boundaries::checkers::call::array_element_type_for_type;

        let arg_node = self.ctx.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return None;
        }
        let spread_data = self.ctx.arena.get_spread(arg_node)?;
        let spread_expr = spread_data.expression;

        let spread_type = self.get_type_of_node(spread_expr);
        let spread_type = self.resolve_type_for_property_access(spread_type);
        let spread_type = self.resolve_lazy_type(spread_type);
        let spread_type = self.evaluate_type_with_env(spread_type);

        if query_common::tuple_elements(self.ctx.types, spread_type).is_some() {
            return None;
        }
        let is_non_tuple_variadic = array_element_type_for_type(self.ctx.types, spread_type)
            .is_some()
            || self.is_iterable_type(spread_type);
        if !is_non_tuple_variadic {
            return None;
        }

        if !self.enclosing_call_parameter_is_optional_non_rest(arg_idx) {
            return None;
        }
        // The solver typically widens optional-param types to include `undefined`
        // before the relation check; format without the display-level strip so
        // tsc's `T | undefined` surface is preserved. For a raw `T` param, union
        // with undefined first.
        let widened = if param_type == TypeId::UNDEFINED
            || query_common::union_list_id(self.ctx.types, param_type).is_some_and(|list_id| {
                self.ctx
                    .types
                    .type_list(list_id)
                    .contains(&TypeId::UNDEFINED)
            }) {
            param_type
        } else {
            self.ctx.types.union2(param_type, TypeId::UNDEFINED)
        };

        Some(self.format_type_for_assignability_message(widened))
    }

    fn enclosing_call_parameter_is_optional_non_rest(&mut self, arg_idx: NodeIndex) -> bool {
        let Some((callee_type, arg_pos)) = self.enclosing_call_arg_position(arg_idx) else {
            return false;
        };

        self.callee_param_is_optional_non_rest_at(callee_type, arg_pos)
    }

    /// Returns `true` when the callee's parameter at `arg_pos` is optional (and
    /// non-rest) for at least one callable shape reachable from `callee_type`.
    /// For union callees we walk the members so that the synthetic `| undefined`
    /// surface contributed by an optional parameter (e.g. `b?: number`) is
    /// elided in diagnostics, matching tsc's display rules.
    fn callee_param_is_optional_non_rest_at(
        &mut self,
        callee_type: TypeId,
        arg_pos: usize,
    ) -> bool {
        let param_is_optional_non_rest = |params: &[tsz_solver::ParamInfo]| {
            params
                .get(arg_pos)
                .map(|p| p.optional && !p.rest)
                .unwrap_or(false)
        };

        if let Some(shape) = query_common::function_shape_for_type(self.ctx.types, callee_type)
            && param_is_optional_non_rest(&shape.params)
        {
            return true;
        }

        if query_common::call_signatures_for_type(self.ctx.types, callee_type).is_some_and(
            |signatures| {
                signatures
                    .iter()
                    .any(|sig| param_is_optional_non_rest(&sig.params))
            },
        ) {
            return true;
        }

        // Union of callables: tsc's union-call rules synthesise the parameter
        // type as `T | undefined` when at least one member treats the slot as
        // optional. Treat the surface as optional in that case so the display
        // strips the synthetic `| undefined`, matching tsc.
        if let Some(members) = query_common::union_members(self.ctx.types, callee_type) {
            return members
                .into_iter()
                .any(|member| self.callee_param_is_optional_non_rest_at(member, arg_pos));
        }

        false
    }

    fn enclosing_call_arg_position(&mut self, arg_idx: NodeIndex) -> Option<(TypeId, usize)> {
        let mut current = arg_idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                || node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                let call = self.ctx.arena.get_call_expr(node)?;
                let args = call.arguments.as_ref()?;
                let arg_pos = args.nodes.iter().position(|&a| a == arg_idx)?;
                let callee_type = self.get_type_of_node(call.expression);
                return Some((callee_type, arg_pos));
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
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
        let readonly =
            crate::query_boundaries::common::readonly_inner_type(self.ctx.types, resolved)
                .is_some();
        resolved = query_common::unwrap_readonly(self.ctx.types, resolved);
        let elements = query_common::tuple_elements(self.ctx.types, resolved)?;
        if !elements.iter().any(|element| element.rest) {
            return None;
        }

        Some(self.format_tuple_element_display(&elements, readonly))
    }

    pub(crate) fn format_tuple_element_display(
        &mut self,
        elements: &[TupleElement],
        readonly: bool,
    ) -> String {
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

        if readonly {
            format!("readonly {tuple_display}")
        } else {
            tuple_display
        }
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
        let Some(mut current) = self.ctx.arena.parent_of(arg_idx) else {
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
        self.assign_relation_outcome(left, right).related
            || self.assign_relation_outcome(right, left).related
    }
}
