//! Literal inference helpers for generic call expression declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::{FunctionData, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_reused_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.imported_static_method_declared_return_type_text(expr_idx)
            .or_else(|| self.call_expression_returned_local_class_constructor_text(expr_idx, false))
            .or_else(|| {
                self.super_method_call_return_type_text(expr_idx)
                    .or_else(|| self.generic_call_literal_type_text(expr_idx))
                    .or_else(|| self.call_expression_function_variable_return_type_text(expr_idx))
                    .or_else(|| self.generic_call_returned_identity_callback_type_text(expr_idx))
                    .or_else(|| self.call_expression_local_overload_return_type_text(expr_idx))
                    .or_else(|| self.call_expression_source_return_type_text(expr_idx))
                    .or_else(|| self.bind_call_remaining_function_type_text(expr_idx))
                    .or_else(|| self.call_expression_declared_return_type_text(expr_idx))
            })
            .map(Self::normalize_constructor_arrow_return_object_text)
            .map(|type_text| {
                self.expand_rest_tuple_parameters_in_function_type_text(expr_idx, &type_text)
                    .unwrap_or(type_text)
            })
            .map(|type_text| {
                Self::expand_parameters_utility_tuple_type_text(&type_text).unwrap_or(type_text)
            })
    }

    fn call_expression_local_overload_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        let symbol = binder.symbols.get(sym_id)?;

        let mut candidates = Vec::new();
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(callable) = Self::callable_decl_parts_from_node(self.arena, decl_node) else {
                continue;
            };
            if callable.body.is_some()
                || callable
                    .type_parameters
                    .is_some_and(|params| !params.nodes.is_empty())
                || callable.type_annotation.is_none()
                || !self.function_signature_accepts_call_arguments(
                    self.arena,
                    callable.parameters,
                    call,
                )
            {
                continue;
            }
            let Some(return_type) = self
                .emit_type_node_text_from_arena(self.arena, callable.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, callable.type_annotation))
            else {
                continue;
            };
            let exact =
                self.overload_signature_exact_literal_match(callable.parameters, &args.nodes);
            if exact || self.overload_signature_accepts_arguments(callable.parameters, &args.nodes)
            {
                candidates.push((exact, return_type.trim().to_string()));
            }
        }

        candidates
            .iter()
            .find_map(|(exact, return_type)| exact.then(|| return_type.clone()))
            .or_else(|| {
                candidates
                    .into_iter()
                    .next()
                    .map(|(_, return_type)| return_type)
            })
    }

    fn overload_signature_exact_literal_match(
        &self,
        parameters: &NodeList,
        arg_nodes: &[NodeIndex],
    ) -> bool {
        !arg_nodes.is_empty()
            && parameters
                .nodes
                .iter()
                .zip(arg_nodes.iter())
                .all(|(&param_idx, &arg_idx)| {
                    let Some(param_type) = self.overload_parameter_type_text(param_idx) else {
                        return false;
                    };
                    let Some(arg_type) = self.overload_argument_type_text(arg_idx) else {
                        return false;
                    };
                    Self::overload_type_text_is_single_literal(&arg_type)
                        && param_type.trim() == arg_type.trim()
                })
    }

    fn overload_signature_accepts_arguments(
        &self,
        parameters: &NodeList,
        arg_nodes: &[NodeIndex],
    ) -> bool {
        parameters
            .nodes
            .iter()
            .zip(arg_nodes.iter())
            .all(|(&param_idx, &arg_idx)| {
                let Some(param_type) = self.overload_parameter_type_text(param_idx) else {
                    return false;
                };
                let Some(arg_type) = self.overload_argument_type_text(arg_idx) else {
                    return false;
                };
                Self::overload_type_accepts_argument_type(&param_type, &arg_type)
            })
    }

    fn overload_parameter_type_text(&self, param_idx: NodeIndex) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        self.emit_type_node_text_from_arena(self.arena, param.type_annotation)
            .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))
            .map(|text| text.trim().to_string())
    }

    fn overload_argument_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        self.reference_declared_type_annotation_text(arg_idx)
            .or_else(|| self.const_literal_initializer_text(arg_idx))
            .or_else(|| self.preferred_expression_type_text(arg_idx))
            .filter(|text| text != "any" && text != "unknown")
            .map(|text| text.trim().to_string())
    }

    fn overload_type_accepts_argument_type(param_type: &str, arg_type: &str) -> bool {
        let param_parts = Self::split_top_level_union_type_parts(param_type);
        let arg_parts = Self::split_top_level_union_type_parts(arg_type);
        !arg_parts.is_empty()
            && arg_parts.iter().all(|arg_part| {
                param_parts.iter().any(|param_part| {
                    Self::overload_type_part_accepts_argument(param_part, arg_part)
                })
            })
    }

    fn overload_type_part_accepts_argument(param_part: &str, arg_part: &str) -> bool {
        let param_part = param_part.trim();
        let arg_part = arg_part.trim();
        param_part == arg_part
            || Self::overload_literal_primitive_name(arg_part)
                .is_some_and(|primitive| primitive == param_part)
    }

    fn overload_type_text_is_single_literal(type_text: &str) -> bool {
        let parts = Self::split_top_level_union_type_parts(type_text);
        parts.len() == 1 && Self::overload_literal_primitive_name(&parts[0]).is_some()
    }

    fn overload_literal_primitive_name(type_text: &str) -> Option<&'static str> {
        let trimmed = type_text.trim();
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return Some("string");
        }
        if matches!(trimmed, "true" | "false") {
            return Some("boolean");
        }
        trimmed.parse::<f64>().ok().map(|_| "number")
    }

    fn call_expression_function_variable_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        if call.type_arguments.is_some() {
            return None;
        }
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        if let Some(sym_id) = self.value_reference_symbol(callee_idx) {
            let binder = self.binder?;
            let sym_id = self
                .resolve_portability_import_alias(sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
            if self
                .with_symbol_declarations(sym_id, |source_arena, decl_idx| {
                    let decl_node = source_arena.get(decl_idx)?;
                    Some(
                        // A local function variable can call itself inside its
                        // own initializer; reusing that initializer type would
                        // recursively re-enter declaration inference.
                        std::ptr::eq(source_arena, self.arena)
                            && decl_node.pos <= expr_node.pos
                            && expr_node.end <= decl_node.end,
                    )
                })
                .unwrap_or(false)
            {
                return None;
            }
        }
        let type_text = self.local_variable_initializer_type_text(callee_idx)?;
        let parts = Self::parse_function_type_text(&type_text)?;
        let return_type = parts.return_type.trim();
        if return_type == "any" || return_type == "unknown" || !return_type.contains('"') {
            return None;
        }
        Some(return_type.to_string())
    }

    fn generic_call_returned_identity_callback_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        if call.type_arguments.is_some() {
            return None;
        }
        let arguments = call.arguments.as_ref()?;
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(call.expression).or_else(|| {
            let callee_idx = self.skip_parenthesized_expression(call.expression)?;
            let callee_name = self.get_identifier_text(callee_idx)?;
            binder.file_locals.get(&callee_name)
        })?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            // Do not infer from the declaration currently being emitted.
            if std::ptr::eq(source_arena, self.arena)
                && decl_node.pos <= expr_node.pos
                && expr_node.end <= decl_node.end
            {
                return None;
            }
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            let returned_param_index =
                self.returned_parameter_index_from_function_body(source_arena, func)?;
            let returned_param_node =
                source_arena.get(*func.parameters.nodes.get(returned_param_index)?)?;
            let returned_param = source_arena.get_parameter(returned_param_node)?;
            let param_type_text = self
                .emit_type_node_text_from_arena(source_arena, returned_param.type_annotation)
                .or_else(|| {
                    self.source_slice_from_arena(source_arena, returned_param.type_annotation)
                })?;
            let source_function_type = Self::parse_function_type_text(param_type_text.trim())?;

            let mut type_param_constraints = Vec::new();
            let type_param_names = func
                .type_parameters
                .as_ref()?
                .nodes
                .iter()
                .copied()
                .filter_map(|param_idx| {
                    let param_node = source_arena.get(param_idx)?;
                    let param = source_arena.get_type_parameter(param_node)?;
                    let name = identifier_text(source_arena, param.name)?;
                    if param.constraint.is_some()
                        && let Some(constraint) = self
                            .emit_type_node_text_from_arena(source_arena, param.constraint)
                            .or_else(|| {
                                self.source_slice_from_arena(source_arena, param.constraint)
                            })
                    {
                        type_param_constraints.push((name.clone(), constraint));
                    }
                    Some(name)
                })
                .collect::<Vec<_>>();
            let arg_idx = *arguments.nodes.get(returned_param_index)?;
            let (param_name, value_text) = self.infer_constrained_identity_callback_substitution(
                &source_function_type,
                arg_idx,
                &type_param_names,
                &type_param_constraints,
            )?;
            Some(Self::replace_whole_words_in_text(
                param_type_text.trim(),
                &[(param_name, value_text)],
            ))
        })
    }

    fn returned_parameter_index_from_function_body(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
    ) -> Option<usize> {
        let body_node = source_arena.get(func.body)?;
        let block = source_arena.get_block(body_node)?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_node = source_arena.get(*block.statements.nodes.first()?)?;
        let ret = source_arena.get_return_statement(stmt_node)?;
        let returned_name = identifier_text(source_arena, ret.expression)?;
        func.parameters
            .nodes
            .iter()
            .copied()
            .enumerate()
            .find_map(|(index, param_idx)| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                (identifier_text(source_arena, param.name).as_deref()
                    == Some(returned_name.as_str()))
                .then_some(index)
            })
    }

    fn bind_call_remaining_function_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_name = self.get_identifier_text(callee_idx)?;
        if callee_name != "bind" {
            return None;
        }
        let args = call.arguments.as_ref()?;
        if args.nodes.len() < 2 {
            return None;
        }
        let source_function = self.function_type_parts_for_expression(*args.nodes.first()?)?;
        let bound_count = args.nodes.len().saturating_sub(1);
        let remaining = source_function
            .parameters
            .iter()
            .skip(bound_count)
            .map(|param| {
                let type_text = param.type_text.trim();
                let name = param.name.as_deref().unwrap_or("arg");
                if param.rest {
                    return format!("...{name}: {type_text}");
                }
                if param.optional {
                    let type_text = if Self::contains_whole_word_in_text(type_text, "undefined") {
                        type_text.to_string()
                    } else {
                        format!("{type_text} | undefined")
                    };
                    return format!("{name}?: {type_text}");
                }
                format!("{name}: {type_text}")
            })
            .collect::<Vec<_>>();
        Some(format!(
            "({}) => {}",
            remaining.join(", "),
            source_function.return_type
        ))
    }

    fn normalize_constructor_arrow_return_object_text(type_text: String) -> String {
        let Some(arrow_pos) = type_text.find("=> {") else {
            return type_text;
        };
        let object_start = arrow_pos + "=> ".len();
        let Some(close_rel) = type_text[object_start + 1..].find('}') else {
            return type_text;
        };
        let object_end = object_start + 1 + close_rel;
        let member_text = type_text[object_start + 1..object_end].trim();
        if member_text.is_empty() || member_text.contains('\n') || !member_text.contains(':') {
            return type_text;
        }

        let member_text = member_text.trim_end_matches(';').trim();
        let replacement = format!("{{\n    {member_text};\n}}");
        let mut normalized = String::new();
        normalized.push_str(&type_text[..object_start]);
        normalized.push_str(&replacement);
        normalized.push_str(&type_text[object_end + 1..]);
        normalized
    }

    pub(in crate::declaration_emitter) fn generic_call_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if !self.call_expression_has_generic_callee(expr_idx) {
            return None;
        }

        if let Some(type_text) =
            self.generic_call_conditional_function_property_tuple_type_text(expr_idx)
        {
            return Some(type_text);
        }

        if let Some(type_text) = self.generic_call_object_property_literal_type_text(expr_idx) {
            return Some(type_text);
        }

        let type_id = self.get_node_type_or_names(&[expr_idx])?;
        if type_id == tsz_solver::types::TypeId::ANY || type_id == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }

        let interner = self.type_interner?;
        tsz_solver::type_queries::is_literal_or_literal_union_type(interner, type_id)
            .then(|| self.print_type_id_for_inferred_declaration(type_id))
    }

    fn generic_call_conditional_function_property_tuple_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[expr_idx])?;
        if type_id == tsz_solver::types::TypeId::ANY || type_id == tsz_solver::types::TypeId::ERROR
        {
            return None;
        }
        let inferred_type_text = self.print_type_id_for_inferred_declaration(type_id);
        let mut tuple_elements = Self::plain_tuple_type_text_elements(&inferred_type_text)?;

        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        let object_arg_idx = arguments.nodes.first().copied()?;

        let replacements = if self.function_expression_has_type_parameters(call.expression) {
            let callee_idx = self.skip_parenthesized_expression(call.expression)?;
            let callee_node = self.arena.get(callee_idx)?;
            let func = self.arena.get_function(callee_node)?;
            self.conditional_function_property_tuple_replacements(self.arena, func, object_arg_idx)
        } else {
            let sym_id = self.value_reference_symbol(call.expression)?;
            let binder = self.binder?;
            let sym_id = self
                .resolve_portability_import_alias(sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
            self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
                let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
                self.conditional_function_property_tuple_replacements(
                    source_arena,
                    func,
                    object_arg_idx,
                )
            })
        }?;

        let mut changed = false;
        for (index, type_text) in replacements {
            if let Some(element) = tuple_elements.get_mut(index) {
                if *element != type_text {
                    *element = type_text;
                    changed = true;
                }
            }
        }

        changed.then(|| format!("[{}]", tuple_elements.join(", ")))
    }

    fn conditional_function_property_tuple_replacements(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        object_arg_idx: NodeIndex,
    ) -> Option<Vec<(usize, String)>> {
        let return_type_params = function_return_tuple_type_parameter_names(source_arena, func)?;
        let param_idx = func.parameters.nodes.first().copied()?;
        let param_node = source_arena.get(param_idx)?;
        let param = source_arena.get_parameter(param_node)?;
        let property_return_type_params =
            type_literal_function_property_return_type_params(source_arena, param.type_annotation);

        let mut replacements = Vec::new();
        for (property_name, type_param_name) in property_return_type_params {
            let Some(tuple_index) = return_type_params
                .iter()
                .position(|name| name == &type_param_name)
            else {
                continue;
            };
            let Some(initializer) =
                self.object_literal_property_initializer(object_arg_idx, &property_name)
            else {
                continue;
            };
            let Some(type_text) =
                self.conditional_function_return_literal_union_type_text(initializer)
            else {
                continue;
            };
            replacements.push((tuple_index, type_text));
        }

        (!replacements.is_empty()).then_some(replacements)
    }

    fn object_literal_property_initializer(
        &self,
        object_idx: NodeIndex,
        property_name: &str,
    ) -> Option<NodeIndex> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            if self.object_literal_member_name_text(name_idx).as_deref() != Some(property_name) {
                continue;
            }
            return self.object_literal_member_initializer(member_node);
        }
        None
    }

    fn conditional_function_return_literal_union_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION {
            return None;
        }
        let conditional = self.arena.get_conditional_expr(expr_node)?;
        let left = self.function_expression_literal_return_type_text(conditional.when_true)?;
        let right = self.function_expression_literal_return_type_text(conditional.when_false)?;
        if left == right {
            Some(left)
        } else {
            Some(format!("{left} | {right}"))
        }
    }

    fn function_expression_literal_return_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(expr_node)?;
        let return_expr = if self
            .arena
            .get(func.body)
            .is_some_and(|node| node.kind == syntax_kind_ext::BLOCK)
        {
            self.function_body_single_return_expression(func.body)?
        } else {
            func.body
        };
        self.const_literal_initializer_text_deep(return_expr)
    }

    fn plain_tuple_type_text_elements(type_text: &str) -> Option<Vec<String>> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(str::trim)
                .map(str::to_string)
                .collect(),
        )
    }

    fn generic_call_object_property_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;

        if self.function_expression_has_type_parameters(call.expression) {
            let callee_idx = self.skip_parenthesized_expression(call.expression)?;
            let callee_node = self.arena.get(callee_idx)?;
            let func = self.arena.get_function(callee_node)?;
            return self.generic_call_object_property_literal_type_text_for_function(
                self.arena, func, arguments,
            );
        }

        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            self.generic_call_object_property_literal_type_text_for_function(
                source_arena,
                func,
                arguments,
            )
        })
    }

    fn generic_call_object_property_literal_type_text_for_function(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        arguments: &NodeList,
    ) -> Option<String> {
        let return_type_param =
            function_return_type_parameter_name(source_arena, func).filter(|type_param| {
                func.type_parameters.as_ref().is_some_and(|type_params| {
                    type_params.nodes.iter().copied().any(|param_idx| {
                        source_arena
                            .get(param_idx)
                            .and_then(|node| source_arena.get_type_parameter(node))
                            .and_then(|param| identifier_text(source_arena, param.name))
                            .is_some_and(|name| name == *type_param)
                    })
                })
            })?;
        func.parameters
            .nodes
            .iter()
            .copied()
            .zip(arguments.nodes.iter().copied())
            .find_map(|(param_idx, arg_idx)| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                parameter_type_has_property_type_parameter(
                    source_arena,
                    param.type_annotation,
                    "type",
                    &return_type_param,
                )
                .then(|| {
                    if arguments.nodes.len() > 1
                        && return_type_parameter_appears_in_other_parameters(
                            source_arena,
                            func,
                            param_idx,
                            &return_type_param,
                        )
                    {
                        return None;
                    }
                    self.object_literal_property_literal_type_text(arg_idx, "type")
                })
                .flatten()
            })
    }

    fn call_expression_has_generic_callee(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        if self.function_expression_has_type_parameters(call.expression) {
            return true;
        }

        if call
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return true;
        }

        if self
            .arena
            .get(call.expression)
            .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            return true;
        }

        let Some(sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            func.type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
                .then_some(())
        })
        .is_some()
    }

    fn function_expression_has_type_parameters(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        self.arena
            .get_function(expr_node)
            .and_then(|func| func.type_parameters.as_ref())
            .is_some_and(|params| !params.nodes.is_empty())
    }
}

fn function_return_type_parameter_name(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<String> {
    type_reference_identifier_name(source_arena, func.type_annotation)
}

fn function_return_tuple_type_parameter_names(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<Vec<String>> {
    let return_node = source_arena.get(func.type_annotation)?;
    if return_node.kind != syntax_kind_ext::TUPLE_TYPE {
        return None;
    }
    let tuple = source_arena.get_tuple_type(return_node)?;
    tuple
        .elements
        .nodes
        .iter()
        .copied()
        .map(|element_idx| type_reference_identifier_name(source_arena, element_idx))
        .collect()
}

fn type_literal_function_property_return_type_params(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
) -> Vec<(String, String)> {
    let Some(type_node) = source_arena.get(type_idx) else {
        return Vec::new();
    };
    if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
        return Vec::new();
    }
    let Some(type_literal) = source_arena.get_type_literal(type_node) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for &member_idx in &type_literal.members.nodes {
        let Some(member_node) = source_arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            continue;
        }
        let Some(signature) = source_arena.get_signature(member_node) else {
            continue;
        };
        let Some(property_name) = identifier_text(source_arena, signature.name) else {
            continue;
        };
        let Some(type_node) = source_arena.get(signature.type_annotation) else {
            continue;
        };
        if type_node.kind != syntax_kind_ext::FUNCTION_TYPE {
            continue;
        }
        let Some(function_type) = source_arena.get_function_type(type_node) else {
            continue;
        };
        let Some(return_type_param) =
            type_reference_identifier_name(source_arena, function_type.type_annotation)
        else {
            continue;
        };
        result.push((property_name, return_type_param));
    }
    result
}

fn parameter_type_has_property_type_parameter(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    property_name: &str,
    type_param_name: &str,
) -> bool {
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_LITERAL => source_arena
            .get_type_literal(type_node)
            .is_some_and(|literal| {
                literal.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                        return false;
                    }
                    let Some(signature) = source_arena.get_signature(member_node) else {
                        return false;
                    };
                    identifier_text(source_arena, signature.name).as_deref() == Some(property_name)
                        && type_reference_identifier_name(source_arena, signature.type_annotation)
                            .as_deref()
                            == Some(type_param_name)
                })
            }),
        k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        parameter_type_has_property_type_parameter(
                            source_arena,
                            part_idx,
                            property_name,
                            type_param_name,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE => source_arena
            .get_wrapped_type(type_node)
            .is_some_and(|wrapped| {
                parameter_type_has_property_type_parameter(
                    source_arena,
                    wrapped.type_node,
                    property_name,
                    type_param_name,
                )
            }),
        _ => false,
    }
}

fn return_type_parameter_appears_in_other_parameters(
    source_arena: &NodeArena,
    func: &FunctionData,
    selected_param_idx: NodeIndex,
    type_param_name: &str,
) -> bool {
    func.parameters
        .nodes
        .iter()
        .copied()
        .filter(|param_idx| *param_idx != selected_param_idx)
        .any(|param_idx| {
            source_arena
                .get(param_idx)
                .and_then(|node| source_arena.get_parameter(node))
                .is_some_and(|param| {
                    type_node_references_type_parameter(
                        source_arena,
                        param.type_annotation,
                        type_param_name,
                        0,
                    )
                })
        })
}

fn type_node_references_type_parameter(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    type_param_name: &str,
    depth: u8,
) -> bool {
    if depth > 32 {
        return false;
    }
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == SyntaxKind::Identifier as u16 => {
            identifier_text(source_arena, type_idx).as_deref() == Some(type_param_name)
        }
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            let Some(type_ref) = source_arena.get_type_ref(type_node) else {
                return false;
            };
            identifier_text(source_arena, type_ref.type_name).as_deref() == Some(type_param_name)
                || type_ref.type_arguments.as_ref().is_some_and(|type_args| {
                    type_args.nodes.iter().copied().any(|arg_idx| {
                        type_node_references_type_parameter(
                            source_arena,
                            arg_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::TYPE_LITERAL => source_arena
            .get_type_literal(type_node)
            .is_some_and(|literal| {
                literal.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    source_arena
                        .get_signature(member_node)
                        .is_some_and(|signature| {
                            type_node_references_type_parameter(
                                source_arena,
                                signature.type_annotation,
                                type_param_name,
                                depth + 1,
                            )
                        })
                })
            }),
        k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        type_node_references_type_parameter(
                            source_arena,
                            part_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            source_arena
                .get_wrapped_type(type_node)
                .is_some_and(|wrapped| {
                    type_node_references_type_parameter(
                        source_arena,
                        wrapped.type_node,
                        type_param_name,
                        depth + 1,
                    )
                })
        }
        k if k == syntax_kind_ext::ARRAY_TYPE => {
            source_arena.get_array_type(type_node).is_some_and(|array| {
                type_node_references_type_parameter(
                    source_arena,
                    array.element_type,
                    type_param_name,
                    depth + 1,
                )
            })
        }
        k if k == syntax_kind_ext::TUPLE_TYPE => {
            source_arena.get_tuple_type(type_node).is_some_and(|tuple| {
                tuple.elements.nodes.iter().copied().any(|element_idx| {
                    type_node_references_type_parameter(
                        source_arena,
                        element_idx,
                        type_param_name,
                        depth + 1,
                    )
                })
            })
        }
        k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
            source_arena
                .get_function_type(type_node)
                .is_some_and(|func_type| {
                    type_node_references_type_parameter(
                        source_arena,
                        func_type.type_annotation,
                        type_param_name,
                        depth + 1,
                    ) || func_type.parameters.nodes.iter().copied().any(|param_idx| {
                        source_arena
                            .get(param_idx)
                            .and_then(|node| source_arena.get_parameter(node))
                            .is_some_and(|param| {
                                type_node_references_type_parameter(
                                    source_arena,
                                    param.type_annotation,
                                    type_param_name,
                                    depth + 1,
                                )
                            })
                    })
                })
        }
        _ => false,
    }
}

fn type_reference_identifier_name(source_arena: &NodeArena, type_idx: NodeIndex) -> Option<String> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind == SyntaxKind::Identifier as u16 {
        return identifier_text(source_arena, type_idx);
    }
    let type_ref = source_arena.get_type_ref(type_node)?;
    identifier_text(source_arena, type_ref.type_name)
}

fn identifier_text(source_arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    source_arena
        .get(idx)
        .and_then(|node| source_arena.get_identifier(node))
        .map(|ident| ident.escaped_text.clone())
}

fn callable_function_from_symbol_decl(
    source_arena: &NodeArena,
    decl_idx: NodeIndex,
) -> Option<&FunctionData> {
    if let Some(func) = source_arena
        .get(decl_idx)
        .and_then(|node| source_arena.get_function(node))
    {
        return Some(func);
    }

    let mut current = decl_idx;
    for _ in 0..8 {
        let node = source_arena.get(current)?;
        if let Some(var_decl) = source_arena.get_variable_declaration(node) {
            let initializer_node = source_arena.get(var_decl.initializer)?;
            if initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return source_arena.get_function(initializer_node);
            }
        }
        current = source_arena.parent_of(current)?;
    }

    None
}
