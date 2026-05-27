//! Correlated union and generic call substitution helpers for DTS emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::{MappedTypeData, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

struct CorrelatedAliasShape {
    mapped_param_name: String,
    discriminant_property_name: String,
    callback_property_name: String,
    callback_parameter_name: String,
    callback_map_type_name: String,
    callback_return_type_text: String,
    member_indices: Vec<NodeIndex>,
}

enum MappedArgumentInference {
    PartialRequired,
    IsomorphicWrapper(String),
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_correlated_alias_return_text(
        &self,
        expr_idx: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, _decl_idx| {
            self.event_like_correlated_alias_return_text(source_arena, type_text, call)
        })
    }

    pub(in crate::declaration_emitter) fn event_like_correlated_alias_return_text(
        &self,
        source_arena: &NodeArena,
        type_text: &str,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> Option<String> {
        let (alias_name, name_type) = Self::single_string_literal_alias_application(type_text)?;
        let alias_type_node = self.find_type_alias_type_node_in_arena(source_arena, alias_name)?;
        let shape = self.correlated_alias_shape(source_arena, alias_type_node)?;
        let callback_param_type = self
            .call_object_function_property_first_parameter_type_text(
                call,
                &shape.callback_property_name,
            )
            .or_else(|| {
                let event_name = name_type.trim_matches('"');
                self.interface_member_type_text_from_arena(
                    source_arena,
                    &shape.callback_map_type_name,
                    event_name,
                )
                .or_else(|| {
                    self.global_interface_member_type_text(
                        &shape.callback_map_type_name,
                        event_name,
                    )
                })
            })?;
        let mut members = Vec::new();
        for &member_idx in &shape.member_indices {
            members.push(self.format_correlated_alias_member(
                source_arena,
                member_idx,
                &shape,
                name_type,
                &callback_param_type,
            )?);
        }
        Some(format!("{{\n{}\n}}", members.join("\n")))
    }

    pub(in crate::declaration_emitter) fn single_string_literal_alias_application(
        type_text: &str,
    ) -> Option<(&str, &str)> {
        let trimmed = type_text.trim();
        let open = trimmed.find('<')?;
        let alias_name = trimmed.get(..open)?.trim();
        let arg = trimmed.get(open + 1..)?.trim().strip_suffix('>')?.trim();
        if alias_name.is_empty()
            || !alias_name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            || !arg.starts_with('"')
            || !arg.ends_with('"')
        {
            return None;
        }
        Some((alias_name, arg))
    }

    pub(in crate::declaration_emitter) fn call_object_function_property_first_parameter_type_text(
        &self,
        call: &tsz_parser::parser::node::CallExprData,
        property_name: &str,
    ) -> Option<String> {
        let args = call.arguments.as_ref()?;
        let object_idx = *args.nodes.first()?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            if self.object_literal_member_name_text(name_idx)?.as_str() != property_name {
                continue;
            }
            let initializer = self.object_literal_member_initializer(member_node)?;
            let initializer_node = self.arena.get(initializer)?;
            let function = self.arena.get_function(initializer_node)?;
            let param_idx = *function.parameters.nodes.first()?;
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let type_id = self.get_node_type_or_names(&[param.name, param_idx])?;
            if type_id == tsz_solver::types::TypeId::ANY
                || type_id == tsz_solver::types::TypeId::ERROR
                || type_id == tsz_solver::types::TypeId::UNKNOWN
            {
                return None;
            }
            return Some(self.print_type_id_for_inferred_declaration(type_id));
        }
        None
    }

    fn correlated_alias_shape(
        &self,
        source_arena: &NodeArena,
        alias_type_node: NodeIndex,
    ) -> Option<CorrelatedAliasShape> {
        let alias_type_node = source_arena.skip_parenthesized(alias_type_node);
        let alias_node = source_arena.get(alias_type_node)?;
        let indexed = source_arena.get_indexed_access_type(alias_node)?;
        let indexed_name =
            self.simple_type_node_name_from_arena(source_arena, indexed.index_type)?;
        let mapped_node = source_arena.get(source_arena.skip_parenthesized(indexed.object_type))?;
        let mapped = source_arena.get_mapped_type(mapped_node)?;
        let type_param_node = source_arena.get(mapped.type_parameter)?;
        let type_param = source_arena.get_type_parameter(type_param_node)?;
        let mapped_param_name = self.identifier_text_from_arena(source_arena, type_param.name)?;
        if self
            .simple_type_node_name_from_arena(source_arena, type_param.constraint)
            .as_deref()
            != Some(indexed_name.as_str())
        {
            return None;
        }

        let value_node_idx = source_arena.skip_parenthesized(mapped.type_node);
        let value_node = source_arena.get(value_node_idx)?;
        let type_literal = source_arena.get_type_literal(value_node)?;
        let mut discriminant_property_name = None;
        let mut callback_property_name = None;
        let mut callback_parameter_name = None;
        let mut callback_map_type_name = None;
        let mut callback_return_type_text = None;

        for &member_idx in &type_literal.members.nodes {
            let Some(member_node) = source_arena.get(member_idx) else {
                continue;
            };
            let Some(member) = source_arena.get_signature(member_node) else {
                continue;
            };
            let Some(member_name) = self.property_name_text_from_arena(source_arena, member.name)
            else {
                continue;
            };
            if self.type_node_is_name_from_arena(
                source_arena,
                member.type_annotation,
                &mapped_param_name,
            ) {
                discriminant_property_name = Some(member_name.clone());
            }
            if let Some((param_name, map_type_name, return_type_text)) = self
                .correlated_alias_callback_function_parts(
                    source_arena,
                    member.type_annotation,
                    &mapped_param_name,
                )
            {
                callback_property_name = Some(member_name);
                callback_parameter_name = Some(param_name);
                callback_map_type_name = Some(map_type_name);
                callback_return_type_text = Some(return_type_text);
            }
        }

        Some(CorrelatedAliasShape {
            mapped_param_name,
            discriminant_property_name: discriminant_property_name?,
            callback_property_name: callback_property_name?,
            callback_parameter_name: callback_parameter_name?,
            callback_map_type_name: callback_map_type_name?,
            callback_return_type_text: callback_return_type_text?,
            member_indices: type_literal.members.nodes.clone(),
        })
    }

    fn interface_member_type_text_from_arena(
        &self,
        source_arena: &NodeArena,
        interface_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let source_file = self.arena_source_file(source_arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            let Some(interface) = source_arena.get_interface(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(source_arena, interface.name)
                .as_deref()
                != Some(interface_name)
            {
                continue;
            }
            for &member_idx in &interface.members.nodes {
                let member_node = source_arena.get(member_idx)?;
                let Some(member) = source_arena.get_signature(member_node) else {
                    continue;
                };
                if self
                    .property_name_text_from_arena(source_arena, member.name)
                    .as_deref()
                    != Some(member_name)
                {
                    continue;
                }
                return self
                    .source_slice_from_arena(source_arena, member.type_annotation)
                    .or_else(|| {
                        self.emit_type_node_text_from_arena(source_arena, member.type_annotation)
                    });
            }
        }
        None
    }

    fn correlated_alias_callback_function_parts(
        &self,
        source_arena: &NodeArena,
        type_node_idx: NodeIndex,
        mapped_param_name: &str,
    ) -> Option<(String, String, String)> {
        let type_node = source_arena.get(source_arena.skip_parenthesized(type_node_idx))?;
        let function = source_arena.get_function_type(type_node)?;
        let param_idx = *function.parameters.nodes.first()?;
        let param_node = source_arena.get(param_idx)?;
        let param = source_arena.get_parameter(param_node)?;
        let param_name = self.identifier_text_from_arena(source_arena, param.name)?;
        let param_type_node =
            source_arena.get(source_arena.skip_parenthesized(param.type_annotation))?;
        let indexed = source_arena.get_indexed_access_type(param_type_node)?;
        if !self.type_node_is_name_from_arena(source_arena, indexed.index_type, mapped_param_name) {
            return None;
        }
        let map_type_name =
            self.simple_type_node_name_from_arena(source_arena, indexed.object_type)?;
        let return_type_text = self
            .source_slice_from_arena(source_arena, function.type_annotation)
            .or_else(|| {
                self.emit_type_node_text_from_arena(source_arena, function.type_annotation)
            })?;
        Some((param_name, map_type_name, return_type_text))
    }

    fn format_correlated_alias_member(
        &self,
        source_arena: &NodeArena,
        member_idx: NodeIndex,
        shape: &CorrelatedAliasShape,
        discriminant_type_text: &str,
        callback_parameter_type_text: &str,
    ) -> Option<String> {
        let member_node = source_arena.get(member_idx)?;
        let member = source_arena.get_signature(member_node)?;
        let member_name = self.property_name_text_from_arena(source_arena, member.name)?;
        let rendered_name = if Self::is_simple_identifier_text(&member_name) {
            member_name.clone()
        } else {
            format!("{member_name:?}")
        };
        let readonly = if source_arena.has_modifier(&member.modifiers, SyntaxKind::ReadonlyKeyword)
        {
            "readonly "
        } else {
            ""
        };
        let optional = if member.question_token { "?" } else { "" };
        let type_text = if member_name == shape.discriminant_property_name {
            discriminant_type_text.to_string()
        } else if member_name == shape.callback_property_name {
            format!(
                "({}: {}) => {}",
                shape.callback_parameter_name,
                callback_parameter_type_text,
                shape.callback_return_type_text
            )
        } else {
            let source_type = self
                .source_slice_from_arena(source_arena, member.type_annotation)
                .or_else(|| {
                    self.emit_type_node_text_from_arena(source_arena, member.type_annotation)
                })?;
            Self::replace_whole_words_in_text(
                &source_type,
                &[(
                    shape.mapped_param_name.clone(),
                    discriminant_type_text.to_string(),
                )],
            )
        };
        Some(format!(
            "    {readonly}{rendered_name}{optional}: {type_text};"
        ))
    }

    fn type_node_is_name_from_arena(
        &self,
        source_arena: &NodeArena,
        type_node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        self.simple_type_node_name_from_arena(source_arena, type_node_idx)
            .as_deref()
            == Some(name)
    }

    pub(in crate::declaration_emitter) fn expand_tuple_item_lookup_mapped_type_text(
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim().trim_end_matches(';').trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        let tuple_start = inner.find("readonly [")?;
        let tuple_body_start = tuple_start + "readonly [".len();
        let tuple_body_end = inner.get(tuple_body_start..)?.find("][number]")? + tuple_body_start;
        let tuple_inner = inner.get(tuple_body_start..tuple_body_end)?;
        let after_number = inner
            .get(tuple_body_end + "][number]".len()..)?
            .trim_start();
        let after_as = after_number.strip_prefix("as")?.trim_start();
        let after_as = after_as.strip_prefix("Item[")?;
        let attr_end = after_as.find(']')?;
        let mut attr_name = after_as
            .get(..attr_end)?
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        if attr_name == "string" {
            attr_name = Self::tuple_items_common_string_literal_property(tuple_inner)?;
        }
        let value_suffix = after_as.get(attr_end + 1..)?.trim_start();
        if !value_suffix.starts_with("]: Item") {
            return None;
        }
        let mut members = Vec::new();
        for item in Self::split_top_level_commas(tuple_inner) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let key = Self::type_literal_property_string_literal_value(item, &attr_name)?;
            members.push(Self::format_mapped_tuple_member(&key, item));
        }
        (!members.is_empty()).then(|| format!("{{\n{}\n}}", members.join("\n")))
    }

    fn tuple_items_common_string_literal_property(tuple_inner: &str) -> Option<String> {
        let mut candidates: Option<Vec<String>> = None;
        for item in Self::split_top_level_commas(tuple_inner) {
            let names = Self::type_literal_string_literal_property_names(item.trim());
            if names.is_empty() {
                return None;
            }
            if let Some(existing) = &mut candidates {
                existing.retain(|name| names.iter().any(|candidate| candidate == name));
            } else {
                candidates = Some(names);
            }
        }
        let candidates = candidates?;
        (candidates.len() == 1).then(|| candidates[0].clone())
    }

    fn type_literal_string_literal_property_names(type_text: &str) -> Vec<String> {
        type_text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim().trim_end_matches(';').trim();
                let trimmed = trimmed.strip_prefix("readonly ").unwrap_or(trimmed);
                let (name, value) = trimmed.split_once(':')?;
                let value = value.trim();
                (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
                    .then(|| name.trim().trim_matches('"').trim_matches('\'').to_string())
            })
            .collect()
    }

    fn type_literal_property_string_literal_value(
        type_text: &str,
        property_name: &str,
    ) -> Option<String> {
        for line in type_text.lines() {
            let trimmed = line.trim().trim_end_matches(';').trim();
            let trimmed = trimmed.strip_prefix("readonly ").unwrap_or(trimmed);
            if let Some(value) = trimmed.strip_prefix(property_name) {
                let value = value.trim_start();
                let value = value.strip_prefix(':')?.trim();
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    return Some(value.trim_matches('"').to_string());
                }
            }
        }
        None
    }

    fn format_mapped_tuple_member(key: &str, value_type: &str) -> String {
        let mut lines = value_type.lines();
        let first = lines.next().unwrap_or(value_type).trim();
        let mut result = format!("    {key}: {first}");
        for line in lines {
            result.push('\n');
            result.push_str("    ");
            result.push_str(line);
        }
        result.push(';');
        result
    }

    fn global_interface_member_type_text(
        &self,
        interface_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let binder = self.binder?;
        for sym_id in binder.symbols.find_all_by_name(interface_name) {
            if let Some(type_text) =
                self.type_member_declared_type_annotation_text(*sym_id, member_name)
                && type_text != "any"
            {
                return Some(type_text);
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn infer_call_type_param_substitutions_from_arguments(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_names: &[String],
        type_param_constraints: &[(String, String)],
    ) -> Vec<(String, String)> {
        let Some(args) = call.arguments.as_ref() else {
            return Vec::new();
        };

        let mut substitutions: Vec<(String, String)> = Vec::new();
        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            if param.dot_dot_dot_token {
                continue;
            }
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
            {
                continue;
            }
            if substitutions
                .iter()
                .any(|(name, _)| name.as_str() == param_type_text)
            {
                continue;
            }
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(
                arg_idx,
                Self::type_param_constraint_text(type_param_constraints, param_type_text),
            ) else {
                continue;
            };
            substitutions.push((
                param_type_text.to_string(),
                Self::parenthesize_generic_function_type_argument(&arg_type_text),
            ));
        }

        for (param_pos, &param_idx) in parameters.nodes.iter().enumerate() {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            if !param.dot_dot_dot_token {
                continue;
            }
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
                || substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == param_type_text)
            {
                continue;
            }

            let rest_args = args.nodes.get(param_pos..).unwrap_or_default();
            let constraint =
                Self::type_param_constraint_text(type_param_constraints, param_type_text).map(
                    |constraint| Self::replace_whole_words_in_text(constraint, &substitutions),
                );
            let mut rest_arg_texts = Vec::new();
            let mut missing_rest_arg = false;
            for &arg_idx in rest_args {
                let Some(mut arg_texts) = self
                    .call_argument_type_texts_for_rest_substitution(arg_idx, constraint.as_deref())
                else {
                    missing_rest_arg = true;
                    break;
                };
                rest_arg_texts.append(&mut arg_texts);
            }
            if missing_rest_arg {
                continue;
            }
            substitutions.push((
                param_type_text.to_string(),
                format!("[{}]", rest_arg_texts.join(", ")),
            ));
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            if param.dot_dot_dot_token {
                continue;
            }
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
            {
                continue;
            }
            if substitutions
                .iter()
                .any(|(name, _)| name.as_str() == param_type_text)
            {
                continue;
            }
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(
                arg_idx,
                Self::type_param_constraint_text(type_param_constraints, param_type_text),
            ) else {
                continue;
            };
            substitutions.push((
                param_type_text.to_string(),
                Self::parenthesize_generic_function_type_argument(&arg_type_text),
            ));
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let Some((param_wrapper, param_inner)) =
                Self::single_generic_type_argument_text(param_type_text.trim())
            else {
                continue;
            };
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_inner)
                || substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == param_inner)
            {
                continue;
            }
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(
                arg_idx,
                Self::type_param_constraint_text(type_param_constraints, param_inner),
            ) else {
                continue;
            };
            let Some((arg_wrapper, arg_inner)) =
                Self::single_generic_type_argument_text(arg_type_text.trim())
            else {
                continue;
            };
            if param_wrapper != arg_wrapper {
                continue;
            }
            substitutions.push((
                param_inner.to_string(),
                Self::parenthesize_generic_function_type_argument(arg_inner),
            ));
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some((param_inner, inference)) =
                self.mapped_argument_inference_from_param_type(source_arena, param.type_annotation)
            else {
                continue;
            };
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_inner.as_str())
                || substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == param_inner.as_str())
            {
                continue;
            }
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(
                arg_idx,
                Self::type_param_constraint_text(type_param_constraints, &param_inner),
            ) else {
                continue;
            };

            let inferred = match inference {
                MappedArgumentInference::PartialRequired => {
                    Self::infer_required_from_partial_argument_text(&arg_type_text)
                }
                MappedArgumentInference::IsomorphicWrapper(wrapper) => {
                    Self::infer_unwrapped_isomorphic_mapped_argument_text(&arg_type_text, &wrapper)
                }
            };
            if let Some(value_text) = inferred {
                substitutions.push((
                    param_inner,
                    Self::parenthesize_generic_function_type_argument(&value_text),
                ));
            }
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            if let Some((param_name, value_text)) = self
                .infer_single_alias_discriminant_substitution(
                    source_arena,
                    param_type_text.trim(),
                    arg_idx,
                    type_param_names,
                )
                && !substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == param_name)
            {
                substitutions.push((param_name, value_text));
            }
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            if !type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&param_type_text, name))
            {
                continue;
            }
            let Some(source_function_type) = Self::parse_function_type_text(&param_type_text)
            else {
                continue;
            };
            if let Some((param_name, value_text)) = self
                .infer_constrained_identity_callback_substitution(
                    &source_function_type,
                    arg_idx,
                    type_param_names,
                    type_param_constraints,
                )
                && !substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == param_name)
            {
                substitutions.push((param_name, value_text));
                continue;
            }
            let Some(argument_function_type) = self.function_type_parts_for_expression(arg_idx)
            else {
                continue;
            };
            Self::infer_function_type_substitutions(
                &source_function_type,
                &argument_function_type,
                type_param_names,
                &mut substitutions,
            );
        }

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            if !param.type_annotation.is_some() {
                continue;
            }
            self.infer_object_argument_substitutions_from_type_node(
                source_arena,
                param.type_annotation,
                arg_idx,
                type_param_names,
                &[],
                &mut substitutions,
                0,
            );
        }

        substitutions
    }

    fn mapped_argument_inference_from_param_type(
        &self,
        source_arena: &NodeArena,
        param_type_idx: NodeIndex,
    ) -> Option<(String, MappedArgumentInference)> {
        let param_type_idx = source_arena.skip_parenthesized(param_type_idx);
        let param_type_node = source_arena.get(param_type_idx)?;
        if param_type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let param_type = source_arena.get_type_ref(param_type_node)?;
        let type_args = param_type.type_arguments.as_ref()?;
        let [type_arg_idx] = type_args.nodes.as_slice() else {
            return None;
        };
        let param_inner = self.simple_type_node_name_from_arena(source_arena, *type_arg_idx)?;
        let sym_id = self
            .declaration_type_symbol_from_type_node(source_arena, param_type_idx)
            .or_else(|| {
                let name = self.simple_type_node_name_from_arena(source_arena, param_type_idx)?;
                self.binder?.get_global_type(&name)
            })?;
        let inference = self.with_symbol_declarations(sym_id, |alias_arena, decl_idx| {
            let alias_node = alias_arena.get(decl_idx)?;
            let alias = alias_arena.get_type_alias(alias_node)?;
            self.mapped_argument_inference_from_alias(alias_arena, alias)
        })?;
        Some((param_inner, inference))
    }

    fn mapped_argument_inference_from_alias(
        &self,
        alias_arena: &NodeArena,
        alias: &TypeAliasData,
    ) -> Option<MappedArgumentInference> {
        let type_params = alias.type_parameters.as_ref()?;
        let [type_param_idx] = type_params.nodes.as_slice() else {
            return None;
        };
        let type_param = alias_arena
            .get(*type_param_idx)
            .and_then(|node| alias_arena.get_type_parameter(node))?;
        let type_param_name = self.identifier_text_from_arena(alias_arena, type_param.name)?;
        let mapped = Self::mapped_type_from_type_node(alias_arena, alias.type_node)?;
        let (mapped_param_name, source_type_name) =
            self.mapped_keyof_source_type_name(alias_arena, mapped)?;
        if source_type_name != type_param_name {
            return None;
        }
        if mapped.name_type.is_some()
            || mapped.members.as_ref().is_some_and(|m| !m.nodes.is_empty())
        {
            return None;
        }
        if mapped.question_token.is_some()
            && self.mapped_value_is_indexed_access(
                alias_arena,
                mapped.type_node,
                &type_param_name,
                &mapped_param_name,
            )
        {
            return Some(MappedArgumentInference::PartialRequired);
        }
        self.mapped_value_isomorphic_wrapper(
            alias_arena,
            mapped.type_node,
            &type_param_name,
            &mapped_param_name,
        )
        .map(MappedArgumentInference::IsomorphicWrapper)
    }

    fn mapped_type_from_type_node(
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<&MappedTypeData> {
        let type_idx = arena.skip_parenthesized(type_idx);
        let type_node = arena.get(type_idx)?;
        if type_node.kind == syntax_kind_ext::MAPPED_TYPE {
            return arena.get_mapped_type(type_node);
        }
        if type_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let literal = arena.get_type_literal(type_node)?;
            let [member_idx] = literal.members.nodes.as_slice() else {
                return None;
            };
            let member_node = arena.get(*member_idx)?;
            if member_node.kind == syntax_kind_ext::MAPPED_TYPE {
                return arena.get_mapped_type(member_node);
            }
        }
        None
    }

    fn mapped_keyof_source_type_name(
        &self,
        arena: &NodeArena,
        mapped: &MappedTypeData,
    ) -> Option<(String, String)> {
        let mapped_param = arena
            .get(mapped.type_parameter)
            .and_then(|node| arena.get_type_parameter(node))?;
        let mapped_param_name = self.identifier_text_from_arena(arena, mapped_param.name)?;
        let constraint_idx = arena.skip_parenthesized(mapped_param.constraint.into_option()?);
        let constraint_node = arena.get(constraint_idx)?;
        let type_op = arena.get_type_operator(constraint_node)?;
        if type_op.operator != SyntaxKind::KeyOfKeyword as u16 {
            return None;
        }
        let source_type_name = self.simple_type_node_name_from_arena(arena, type_op.type_node)?;
        Some((mapped_param_name, source_type_name))
    }

    fn mapped_value_is_indexed_access(
        &self,
        arena: &NodeArena,
        value_idx: NodeIndex,
        object_name: &str,
        index_name: &str,
    ) -> bool {
        self.indexed_access_names(arena, value_idx)
            .is_some_and(|(object, index)| object == object_name && index == index_name)
    }

    fn mapped_value_isomorphic_wrapper(
        &self,
        arena: &NodeArena,
        value_idx: NodeIndex,
        object_name: &str,
        index_name: &str,
    ) -> Option<String> {
        let value_idx = arena.skip_parenthesized(value_idx);
        let value_node = arena.get(value_idx)?;
        if value_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let value_type = arena.get_type_ref(value_node)?;
        let wrapper = self.simple_type_node_name_from_arena(arena, value_idx)?;
        if !Self::is_simple_identifier_text(&wrapper) {
            return None;
        }
        let type_args = value_type.type_arguments.as_ref()?;
        let [inner_idx] = type_args.nodes.as_slice() else {
            return None;
        };
        self.mapped_value_is_indexed_access(arena, *inner_idx, object_name, index_name)
            .then_some(wrapper)
    }

    fn indexed_access_names(
        &self,
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<(String, String)> {
        let type_idx = arena.skip_parenthesized(type_idx);
        let type_node = arena.get(type_idx)?;
        if type_node.kind != syntax_kind_ext::INDEXED_ACCESS_TYPE {
            return None;
        }
        let indexed = arena.get_indexed_access_type(type_node)?;
        let object_name = self.simple_type_node_name_from_arena(arena, indexed.object_type)?;
        let index_name = self.simple_type_node_name_from_arena(arena, indexed.index_type)?;
        Some((object_name, index_name))
    }

    pub(in crate::declaration_emitter) fn infer_unwrapped_isomorphic_mapped_argument_text(
        arg_type_text: &str,
        wrapper: &str,
    ) -> Option<String> {
        let trimmed = arg_type_text.trim();
        if let Some(elements) = Self::tuple_type_text_elements_preserving_rest(trimmed) {
            let mut inferred = Vec::new();
            for element in elements {
                inferred.push(Self::unwrap_mapped_tuple_element(&element, wrapper)?);
            }
            return Some(format!("[{}]", inferred.join(", ")));
        }

        if let Some(inner) = Self::strip_array_suffix(trimmed)
            && let Some(unwrapped) = Self::unwrap_single_wrapper_type(inner, wrapper)
        {
            return Some(format!("{unwrapped}[]"));
        }

        Self::object_type_members(trimmed).and_then(|members| {
            let mut lines = Vec::new();
            for member in members {
                let (name, optional, type_text) = Self::object_member_parts(&member)?;
                let unwrapped = Self::unwrap_single_wrapper_type(type_text, wrapper)?;
                let optional = if optional { "?" } else { "" };
                lines.push(format!("    {name}{optional}: {unwrapped};"));
            }
            (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
        })
    }

    pub(in crate::declaration_emitter) fn infer_required_from_partial_argument_text(
        arg_type_text: &str,
    ) -> Option<String> {
        let trimmed = arg_type_text.trim();
        if let Some(elements) = Self::tuple_type_text_elements_preserving_rest(trimmed) {
            let mut inferred = Vec::new();
            for element in elements {
                inferred.push(Self::required_tuple_element_text(&element));
            }
            return Some(format!("[{}]", inferred.join(", ")));
        }

        if let Some(inner) = Self::strip_array_suffix(trimmed) {
            let inner = Self::remove_undefined_union_member(inner);
            return Some(format!("{inner}[]"));
        }

        Self::object_type_members(trimmed).and_then(|members| {
            let mut lines = Vec::new();
            for member in members {
                let (name, _, type_text) = Self::object_member_parts(&member)?;
                let required = Self::remove_undefined_union_member(type_text);
                lines.push(format!("    {name}: {required};"));
            }
            (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
        })
    }

    fn tuple_type_text_elements_preserving_rest(type_text: &str) -> Option<Vec<String>> {
        let mut text = type_text.trim();
        if let Some(rest) = text.strip_prefix("readonly ") {
            text = rest.trim();
        }
        if !text.starts_with('[') || !text.ends_with(']') {
            return None;
        }
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(|part| part.trim().to_string())
                .collect(),
        )
    }

    fn unwrap_mapped_tuple_element(element: &str, wrapper: &str) -> Option<String> {
        let trimmed = element.trim();
        if let Some(rest) = trimmed.strip_prefix("...") {
            let rest = rest.trim();
            let array_inner = Self::strip_array_suffix(rest).unwrap_or(rest);
            let unwrapped = Self::unwrap_single_wrapper_type(array_inner, wrapper)?;
            return Some(format!("...{unwrapped}[]"));
        }
        Self::unwrap_single_wrapper_type(trimmed.trim_end_matches('?').trim(), wrapper)
            .map(str::to_string)
    }

    fn required_tuple_element_text(element: &str) -> String {
        let trimmed = element.trim();
        if let Some(rest) = trimmed.strip_prefix("...") {
            let rest = rest.trim();
            let array_inner = Self::strip_array_suffix(rest).unwrap_or(rest);
            let required = Self::remove_undefined_union_member(array_inner);
            return format!("...{required}[]");
        }
        Self::remove_undefined_union_member(trimmed.trim_end_matches('?').trim())
    }

    fn strip_array_suffix(type_text: &str) -> Option<&str> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_suffix("[]")?.trim();
        Some(
            inner
                .strip_prefix('(')
                .and_then(|text| text.strip_suffix(')'))
                .unwrap_or(inner)
                .trim(),
        )
    }

    fn unwrap_single_wrapper_type<'b>(type_text: &'b str, wrapper: &str) -> Option<&'b str> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix(wrapper)?.trim_start();
        let inner = inner.strip_prefix('<')?.strip_suffix('>')?.trim();
        (!inner.is_empty()).then_some(inner)
    }

    fn object_type_members(type_text: &str) -> Option<Vec<String>> {
        let inner = type_text
            .trim()
            .strip_prefix('{')?
            .strip_suffix('}')?
            .trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        Some(
            Self::split_top_level_semicolon_members(inner)
                .into_iter()
                .map(|member| member.trim().to_string())
                .filter(|member| !member.is_empty())
                .collect(),
        )
    }

    fn split_top_level_semicolon_members(text: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut angle_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        for (idx, ch) in text.char_indices() {
            match ch {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                ';' if angle_depth == 0
                    && brace_depth == 0
                    && bracket_depth == 0
                    && paren_depth == 0 =>
                {
                    parts.push(&text[start..idx]);
                    start = idx + 1;
                }
                _ => {}
            }
        }
        parts.push(&text[start..]);
        parts
    }

    fn object_member_parts(member: &str) -> Option<(&str, bool, &str)> {
        let colon = Self::find_top_level_byte(member, b':')?;
        let name = member[..colon].trim();
        let type_text = member[colon + 1..].trim();
        if name.is_empty() || type_text.is_empty() {
            return None;
        }
        let (name, optional) = name
            .strip_suffix('?')
            .map(|name| (name.trim_end(), true))
            .unwrap_or((name, false));
        Some((name, optional, type_text))
    }

    fn remove_undefined_union_member(type_text: &str) -> String {
        let parts = Self::split_top_level_union_type_parts(type_text)
            .into_iter()
            .map(|part| part.trim().to_string())
            .filter(|part| part != "undefined")
            .collect::<Vec<_>>();
        if parts.is_empty() {
            "undefined".to_string()
        } else {
            parts.join(" | ")
        }
    }

    pub(super) fn infer_constrained_identity_callback_substitution(
        &self,
        source_function_type: &super::type_inference_function_text::FunctionTypeTextParts,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        type_param_constraints: &[(String, String)],
    ) -> Option<(String, String)> {
        if source_function_type.parameters.len() != 1 {
            return None;
        }
        let source_param = source_function_type.parameters.first()?;
        let param_type = source_param.type_text.trim();
        if source_param.rest
            || !type_param_names.iter().any(|name| name == param_type)
            || source_function_type.return_type.trim() != param_type
        {
            return None;
        }
        let constraint = Self::type_param_constraint_text(type_param_constraints, param_type)?;

        let arg_idx = self.skip_parenthesized_non_null_and_comma(arg_idx);
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(arg_node)?;
        if func.type_annotation.is_some() || func.parameters.nodes.len() != 1 {
            return None;
        }
        let arg_param_idx = *func.parameters.nodes.first()?;
        let arg_param_node = self.arena.get(arg_param_idx)?;
        let arg_param = self.arena.get_parameter(arg_param_node)?;
        if arg_param.type_annotation.is_some() {
            return None;
        }
        let arg_param_name = self.get_identifier_text(arg_param.name)?;
        let body_idx = self.skip_parenthesized_expression(func.body)?;
        let body_node = self.arena.get(body_idx)?;
        if body_node.kind != SyntaxKind::Identifier as u16
            || self.get_identifier_text(body_idx).as_deref() != Some(arg_param_name.as_str())
        {
            return None;
        }

        Some((param_type.to_string(), constraint.to_string()))
    }

    fn single_generic_type_argument_text(type_text: &str) -> Option<(&str, &str)> {
        let type_text = type_text.trim();
        let open = type_text.find('<')?;
        if !type_text.ends_with('>') {
            return None;
        }
        let wrapper = type_text[..open].trim();
        if wrapper.is_empty()
            || wrapper
                .chars()
                .any(|ch| !(ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric()))
        {
            return None;
        }
        let inner = &type_text[open + 1..type_text.len() - 1];
        let mut depth = 0usize;
        for ch in inner.chars() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.checked_sub(1)?;
                }
                ',' if depth == 0 => return None,
                _ => {}
            }
        }
        (depth == 0).then_some((wrapper, inner.trim()))
    }

    fn type_param_constraint_text<'b>(
        type_param_constraints: &'b [(String, String)],
        type_param_name: &str,
    ) -> Option<&'b str> {
        type_param_constraints
            .iter()
            .find_map(|(name, constraint)| (name == type_param_name).then_some(constraint.as_str()))
    }

    pub(in crate::declaration_emitter) fn infer_single_alias_discriminant_substitution(
        &self,
        source_arena: &NodeArena,
        param_type_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
    ) -> Option<(String, String)> {
        let (alias_name, param_name) =
            Self::single_type_parameter_alias_argument(param_type_text, type_param_names)?;
        let alias_type_node = self.find_type_alias_type_node_in_arena(source_arena, alias_name)?;
        let shape = self.correlated_alias_shape(source_arena, alias_type_node)?;
        let value_text = self.object_literal_property_literal_type_text(
            arg_idx,
            &shape.discriminant_property_name,
        )?;
        Some((param_name.to_string(), value_text))
    }

    pub(in crate::declaration_emitter) fn single_type_parameter_alias_argument<'b>(
        type_text: &'b str,
        type_param_names: &'b [String],
    ) -> Option<(&'b str, &'b str)> {
        let trimmed = type_text.trim();
        let open = trimmed.find('<')?;
        let alias_name = trimmed.get(..open)?.trim();
        if alias_name.is_empty()
            || !alias_name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        let inner = trimmed.get(open + 1..)?.trim().strip_suffix('>')?.trim();
        type_param_names
            .iter()
            .find(|name| name.as_str() == inner)
            .map(|name| (alias_name, name.as_str()))
    }

    pub(in crate::declaration_emitter) fn object_literal_property_literal_type_text(
        &self,
        object_idx: NodeIndex,
        property_name: &str,
    ) -> Option<String> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            if self.object_literal_member_name_text(name_idx)? != property_name {
                continue;
            }
            let initializer = self.object_literal_member_initializer(member_node)?;
            return self
                .const_literal_initializer_text(initializer)
                .or_else(|| self.infer_fallback_type_text_at(initializer, 0));
        }
        None
    }

    pub(in crate::declaration_emitter) fn call_argument_type_text_for_substitution(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<String> {
        if let Some(type_text) = self.referenced_parameter_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }
        if let Some(type_text) = self.reference_declared_source_type_annotation_text(arg_idx) {
            return Some(type_text);
        }
        if let Some(type_text) = self.reference_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }

        if let Some(type_text) =
            self.contextual_function_argument_type_text(arg_idx, type_param_constraint)
        {
            return Some(type_text);
        }

        // Bare type-parameter inference widens literal arguments (`box(0)` ->
        // `Box<number>`, not `Box<0>`). Keep literal-preserving paths only for
        // explicit `as const`, local variable initializers that already carry
        // literal types, or primitive literals inferred into primitive-constrained
        // type parameters.
        self.as_const_assertion_type_text(arg_idx)
            .or_else(|| self.local_variable_initializer_type_text(arg_idx))
            .or_else(|| {
                type_param_constraint
                    .is_some_and(Self::constraint_preserves_primitive_literal)
                    .then(|| self.primitive_literal_argument_type_text(arg_idx))
                    .flatten()
            })
            .or_else(|| {
                self.preferred_expression_type_text(arg_idx)
                    .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
            })
            .or_else(|| self.infer_fallback_type_text_at(arg_idx, 0))
    }

    fn call_argument_type_texts_for_rest_substitution(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<Vec<String>> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return self
                .call_argument_type_text_for_substitution(arg_idx, type_param_constraint)
                .map(|text| vec![text]);
        }

        let spread = self.arena.get_spread(arg_node)?;
        let spread_expr = self.skip_parenthesized_expression(spread.expression)?;
        let spread_type_text = self
            .get_node_type(spread_expr)
            .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            .filter(|text| Self::tuple_type_text_elements(text).is_some())
            .or_else(|| self.reference_declared_type_annotation_text(spread_expr))
            .or_else(|| self.local_variable_initializer_type_text(spread_expr))
            .or_else(|| self.preferred_expression_type_text(spread_expr))
            .or_else(|| {
                self.get_node_type(spread_expr)
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            })
            .or_else(|| self.infer_fallback_type_text_at(spread_expr, 0))?;

        if let Some(elements) = Self::tuple_type_text_elements(&spread_type_text) {
            return Some(elements);
        }

        Some(vec![spread_type_text])
    }

    fn tuple_type_text_elements(type_text: &str) -> Option<Vec<String>> {
        let mut text = type_text.trim();
        if let Some(rest) = text.strip_prefix("readonly ") {
            text = rest.trim();
        }
        if !text.starts_with('[') || !text.ends_with(']') {
            return None;
        }
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(|part| {
                    let mut part = part.trim();
                    if let Some(rest) = part.strip_prefix("...") {
                        part = rest.trim();
                    }
                    if let Some(colon) = Self::find_top_level_byte(part, b':') {
                        part = part[colon + 1..].trim();
                    }
                    part.trim_end_matches('?').trim().to_string()
                })
                .collect(),
        )
    }

    fn contextual_function_argument_type_text(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<String> {
        let expected = type_param_constraint?.trim();
        let expected = expected.strip_suffix("[]").unwrap_or(expected).trim();
        let expected = expected
            .strip_prefix('(')
            .and_then(|text| text.strip_suffix(')'))
            .unwrap_or(expected)
            .trim();
        let expected_parts = Self::parse_function_type_text(expected)?;
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(arg_node)?;
        let params = func
            .parameters
            .nodes
            .iter()
            .copied()
            .enumerate()
            .map(|(position, param_idx)| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name = self.get_identifier_text(param.name)?;
                let type_text = self
                    .emit_type_node_text(param.type_annotation)
                    .or_else(|| {
                        expected_parts
                            .parameters
                            .get(position)
                            .map(|param| param.type_text.clone())
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            })
            .collect::<Option<Vec<_>>>()?;
        let return_text = self
            .emit_type_node_text(func.type_annotation)
            .or_else(|| self.contextual_function_body_return_type_text(func, &expected_parts))
            .unwrap_or_else(|| expected_parts.return_type.clone());
        Some(format!("({}) => {return_text}", params.join(", ")))
    }

    fn contextual_function_body_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        expected_parts: &super::type_inference_function_text::FunctionTypeTextParts,
    ) -> Option<String> {
        let body = self.skip_parenthesized_expression(func.body)?;
        let body_text = self
            .source_file_text
            .as_deref()
            .and_then(|text| {
                let node = self.arena.get(body)?;
                let start = usize::try_from(node.pos).ok()?;
                let end = usize::try_from(node.end).ok()?;
                text.get(start..end)
            })
            .unwrap_or_default();
        if body_text.contains("\"\"") || body_text.contains("''") || body_text.contains('`') {
            return Some("string".to_string());
        }
        if expected_parts
            .parameters
            .iter()
            .any(|param| param.type_text.trim() == "number")
            && body_text.contains('+')
        {
            return Some("number".to_string());
        }
        self.infer_fallback_type_text_at(body, 0)
    }

    fn constraint_preserves_primitive_literal(constraint: &str) -> bool {
        Self::contains_whole_word_in_text(constraint, "string")
            || Self::contains_whole_word_in_text(constraint, "number")
            || Self::contains_whole_word_in_text(constraint, "boolean")
            || Self::contains_whole_word_in_text(constraint, "bigint")
    }

    fn primitive_literal_argument_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        (arg_node.kind == SyntaxKind::StringLiteral as u16
            || arg_node.kind == SyntaxKind::NumericLiteral as u16
            || arg_node.kind == SyntaxKind::BigIntLiteral as u16
            || arg_node.kind == SyntaxKind::TrueKeyword as u16
            || arg_node.kind == SyntaxKind::FalseKeyword as u16)
            .then(|| self.js_literal_type_text(arg_idx))
            .flatten()
    }

    fn referenced_parameter_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let mut current = decl_idx;
            for _ in 0..12 {
                let node = source_arena.get(current)?;
                if let Some(param) = source_arena.get_parameter(node) {
                    let type_annotation = param.type_annotation;
                    if !type_annotation.is_some() {
                        return None;
                    }
                    let type_text = self
                        .emit_type_node_text_from_arena(source_arena, type_annotation)
                        .or_else(|| self.source_slice_from_arena(source_arena, type_annotation))?;
                    let trimmed = type_text.trim_end();
                    let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                    return Some(trimmed.to_string());
                }
                let parent = source_arena.parent_of(current)?;
                if parent.is_none() {
                    break;
                }
                current = parent;
            }
            None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    #[test]
    fn correlated_alias_shape_detects_renamed_discriminant_and_callback() {
        let mut parser = ParserState::new(
            "shape.ts".to_string(),
            r#"
interface Registry {
    alpha: AlphaEvent;
}
interface AlphaEvent {
    alpha: true;
}
type Entry<Key extends keyof Registry> = { [Choice in Key]: {
    readonly kind: Choice;
    readonly enabled?: boolean;
    readonly handler: (payload: Registry[Choice]) => void;
}}[Key];
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let alias_type_node = emitter
            .find_type_alias_type_node_in_arena(arena, "Entry")
            .expect("alias type node");
        let shape = emitter
            .correlated_alias_shape(arena, alias_type_node)
            .expect("correlated alias shape");

        assert_eq!(shape.mapped_param_name, "Choice");
        assert_eq!(shape.discriminant_property_name, "kind");
        assert_eq!(shape.callback_property_name, "handler");
        assert_eq!(shape.callback_parameter_name, "payload");
        assert_eq!(shape.callback_map_type_name, "Registry");
        assert_eq!(shape.callback_return_type_text, "void");
        assert_eq!(
            emitter
                .interface_member_type_text_from_arena(arena, "Registry", "alpha")
                .as_deref(),
            Some("AlphaEvent")
        );
    }
}
