//! Correlated union and generic call substitution helpers for DTS emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn event_like_correlated_alias_return_text(
        &self,
        source_arena: &NodeArena,
        type_text: &str,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> Option<String> {
        let (alias_name, name_type) = Self::single_string_literal_alias_application(type_text)?;
        let alias_type_node = self.find_type_alias_type_node_in_arena(source_arena, alias_name)?;
        let alias_text = self
            .source_slice_from_arena(source_arena, alias_type_node)
            .or_else(|| self.emit_type_node_text_from_arena(source_arena, alias_type_node))?;
        if !alias_text.contains("readonly name:")
            || !alias_text.contains("readonly callback:")
            || !alias_text.contains("DocumentEventMap")
        {
            return None;
        }
        let callback_param_type = self
            .call_object_callback_parameter_type_text(call)
            .or_else(|| {
                let event_name = name_type.trim_matches('"');
                self.global_interface_member_type_text("DocumentEventMap", event_name)
            })?;
        Some(format!(
            "{{\n    readonly name: {name_type};\n    readonly once?: boolean;\n    readonly callback: (ev: {callback_param_type}) => void;\n}}"
        ))
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

    pub(in crate::declaration_emitter) fn call_object_callback_parameter_type_text(
        &self,
        call: &tsz_parser::parser::node::CallExprData,
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
            if self.object_literal_member_name_text(name_idx)? != "callback" {
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
    ) -> Vec<(String, String)> {
        let Some(args) = call.arguments.as_ref() else {
            return Vec::new();
        };

        let mut substitutions: Vec<(String, String)> = Vec::new();
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
            let Some(rest_arg_texts) = rest_args
                .iter()
                .copied()
                .map(|arg_idx| self.call_argument_type_text_for_substitution(arg_idx))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
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
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(arg_idx) else {
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
            let Some(arg_type_text) = self.call_argument_type_text_for_substitution(arg_idx) else {
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
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            if let Some((param_name, value_text)) = self
                .infer_single_alias_discriminant_substitution(
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

        substitutions
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

    pub(in crate::declaration_emitter) fn infer_single_alias_discriminant_substitution(
        &self,
        param_type_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
    ) -> Option<(String, String)> {
        let param_name =
            Self::single_type_parameter_alias_argument(param_type_text, type_param_names)?;
        let value_text = self.object_literal_property_literal_type_text(arg_idx, "name")?;
        Some((param_name.to_string(), value_text))
    }

    pub(in crate::declaration_emitter) fn single_type_parameter_alias_argument<'b>(
        type_text: &'b str,
        type_param_names: &'b [String],
    ) -> Option<&'b str> {
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
            .map(String::as_str)
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
    ) -> Option<String> {
        if let Some(type_text) = self.reference_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }

        // Bare type-parameter inference widens literal arguments (`box(0)` ->
        // `Box<number>`, not `Box<0>`). Keep literal-preserving paths only for
        // explicit `as const` or local variable initializers that already carry
        // literal types.
        self.as_const_assertion_type_text(arg_idx)
            .or_else(|| self.local_variable_initializer_type_text(arg_idx))
            .or_else(|| {
                self.preferred_expression_type_text(arg_idx)
                    .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
            })
            .or_else(|| self.infer_fallback_type_text_at(arg_idx, 0))
    }
}
