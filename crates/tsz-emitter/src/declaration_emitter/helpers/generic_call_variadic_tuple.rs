//! Generic call variadic tuple inference helpers for declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn generic_variadic_tuple_call_return_type_text(
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

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(callable) = Self::callable_decl_parts_from_node(self.arena, decl_node) else {
                continue;
            };
            let [param_idx] = callable.parameters.nodes.as_slice() else {
                continue;
            };
            let Some(param_node) = self.arena.get(*param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.dot_dot_dot_token || !callable.type_annotation.is_some() {
                continue;
            }

            let return_text = self
                .emit_type_node_text(callable.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, callable.type_annotation))?;
            let Some((type_param_name, return_tail)) =
                Self::variadic_tuple_return_parts(&return_text)
            else {
                continue;
            };
            let param_text = self
                .emit_type_node_text(param.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))?;
            let Some(fixed_tail_count) =
                Self::variadic_tuple_param_tail_count(&param_text, &type_param_name)
            else {
                continue;
            };
            let constraint_text =
                self.type_parameter_constraint_text(callable.type_parameters, &type_param_name)?;
            let constraint_elements = Self::tuple_type_elements_text(&constraint_text)?;
            let prefix_args_len = args.nodes.len().saturating_sub(fixed_tail_count);
            let prefix_args = &args.nodes[..prefix_args_len];
            let prefix_elements = self
                .variadic_tuple_literal_prefix_elements(prefix_args, &constraint_elements)
                .unwrap_or(constraint_elements);
            let mut elements = prefix_elements;
            elements.extend(return_tail);
            return Some(format!("[{}]", elements.join(", ")));
        }

        None
    }

    pub(super) fn flatten_tuple_spread_substitutions_text(text: &str) -> String {
        let trimmed = text.trim();
        let readonly_prefix = "readonly ";
        let (prefix, tuple_text) = trimmed
            .strip_prefix(readonly_prefix)
            .map(|rest| (readonly_prefix, rest.trim()))
            .unwrap_or(("", trimmed));
        if !tuple_text.starts_with('[') || !tuple_text.ends_with(']') {
            return text.to_string();
        }
        let inner = &tuple_text[1..tuple_text.len() - 1];
        let mut changed = false;
        let mut flattened = Vec::new();
        for element in Self::split_top_level_commas(inner) {
            let element = element.trim();
            let Some(rest) = element.strip_prefix("...").map(str::trim) else {
                flattened.push(element.to_string());
                continue;
            };
            let rest = rest.strip_prefix("readonly ").unwrap_or(rest).trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                changed = true;
                let nested = &rest[1..rest.len() - 1];
                flattened.extend(
                    Self::split_top_level_commas(nested)
                        .into_iter()
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty()),
                );
            } else {
                flattened.push(element.to_string());
            }
        }
        if changed {
            format!("{prefix}[{}]", flattened.join(", "))
        } else {
            text.to_string()
        }
    }

    pub(super) fn expand_tuple_index_substitutions_text(
        text: &str,
        substitutions: &[(String, String)],
    ) -> String {
        if let Some(expanded) =
            Self::expand_numeric_tuple_index_array_union_text(text, substitutions)
        {
            return expanded;
        }

        let mut expanded = text.to_string();
        for (name, value) in substitutions {
            let Some(union_text) = Self::tuple_number_index_union_text(value) else {
                continue;
            };
            expanded =
                Self::replace_type_parameter_number_index_access(&expanded, name, &union_text);
        }
        expanded
    }

    fn tuple_number_index_union_text(type_text: &str) -> Option<String> {
        let elements = Self::tuple_type_text_elements_preserving_rest(type_text)?;
        let mut members = Vec::new();
        for element in elements {
            let element = element.trim();
            if element.starts_with("...") || element.is_empty() {
                return None;
            }
            let element = Self::find_top_level_byte(element, b':')
                .and_then(|idx| element.get(idx + 1..))
                .unwrap_or(element)
                .trim()
                .trim_end_matches('?')
                .trim();
            if element.is_empty() {
                return None;
            }
            members.push(element.to_string());
        }
        (!members.is_empty()).then(|| members.join(" | "))
    }

    fn expand_numeric_tuple_index_array_union_text(
        text: &str,
        substitutions: &[(String, String)],
    ) -> Option<String> {
        let trimmed = text.trim();
        let array_inner = trimmed.strip_suffix("[]")?.trim();
        let union_inner = array_inner
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
            .unwrap_or(array_inner)
            .trim();
        let parts = Self::split_top_level_union_type_parts(union_inner);
        if parts.len() < 2 {
            return None;
        }

        let mut groups = Vec::with_capacity(parts.len());
        for part in parts {
            let name = Self::tuple_number_index_type_parameter_name(&part)?;
            let (_, value) = substitutions
                .iter()
                .find(|(candidate, _)| candidate == &name)?;
            let members = Self::tuple_number_index_member_texts(value)?;
            if members.len() < 2
                || !members.iter().all(|member| {
                    tsz_common::numeric::parse_numeric_literal_value(member).is_some()
                })
            {
                return None;
            }
            groups.push(members);
        }

        let members = Self::legacy_numeric_tuple_union_members(&groups);
        (!members.is_empty()).then(|| format!("({})[]", members.join(" | ")))
    }

    fn tuple_number_index_type_parameter_name(text: &str) -> Option<String> {
        let name = text.trim().strip_suffix("[number]")?.trim();
        Self::is_simple_identifier_text(name).then(|| name.to_string())
    }

    fn tuple_number_index_member_texts(type_text: &str) -> Option<Vec<String>> {
        let elements = Self::tuple_type_text_elements_preserving_rest(type_text)?;
        let mut members = Vec::new();
        for element in elements {
            let element = element.trim();
            if element.starts_with("...") || element.is_empty() {
                return None;
            }
            let element = Self::find_top_level_byte(element, b':')
                .and_then(|idx| element.get(idx + 1..))
                .unwrap_or(element)
                .trim()
                .trim_end_matches('?')
                .trim();
            if element.is_empty() {
                return None;
            }
            members.push(element.to_string());
        }
        (!members.is_empty()).then_some(members)
    }

    fn legacy_numeric_tuple_union_members(groups: &[Vec<String>]) -> Vec<String> {
        let mut members = Vec::new();
        // Full-check declaration baselines preserve a numeric literal union insertion
        // order from generic tuple inference here. This source-summary path has no
        // usable `TypeId`s for the argument tuple elements, so mirror that insertion
        // walk over the substituted fixed tuple texts rather than falling back to
        // raw tuple source order.
        if let Some(first_group) = groups.first()
            && let Some(member) = first_group.get(1)
        {
            Self::push_unique_union_member(&mut members, member);
        }
        for group in groups.iter().skip(1) {
            if let Some(member) = group.first() {
                Self::push_unique_union_member(&mut members, member);
            }
        }
        if let Some(first_group) = groups.first() {
            for (index, member) in first_group.iter().enumerate() {
                if index != 1 {
                    Self::push_unique_union_member(&mut members, member);
                }
            }
        }
        for group in groups.iter().skip(1) {
            for member in group.iter().skip(2) {
                Self::push_unique_union_member(&mut members, member);
            }
            if let Some(member) = group.get(1) {
                Self::push_unique_union_member(&mut members, member);
            }
        }
        members
    }

    fn push_unique_union_member(members: &mut Vec<String>, member: &str) {
        if members.iter().any(|known| known == member) {
            return;
        }
        members.push(member.to_string());
    }

    fn replace_type_parameter_number_index_access(
        text: &str,
        name: &str,
        replacement: &str,
    ) -> String {
        let needle = format!("{name}[number]");
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0usize;
        while let Some(relative_idx) = text[cursor..].find(&needle) {
            let idx = cursor + relative_idx;
            let end = idx + needle.len();
            let before_ok = idx == 0
                || !text.as_bytes()[idx - 1].is_ascii_alphanumeric()
                    && text.as_bytes()[idx - 1] != b'_'
                    && text.as_bytes()[idx - 1] != b'$';
            let after_ok = end == text.len()
                || !text.as_bytes()[end].is_ascii_alphanumeric()
                    && text.as_bytes()[end] != b'_'
                    && text.as_bytes()[end] != b'$';
            result.push_str(&text[cursor..idx]);
            if before_ok && after_ok {
                result.push_str(replacement);
            } else {
                result.push_str(&text[idx..end]);
            }
            cursor = end;
        }
        result.push_str(&text[cursor..]);
        result
    }

    pub(super) fn infer_tuple_spread_argument_substitutions(
        &self,
        param_type_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        type_param_constraints: &[(String, String)],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(param_elements) = Self::tuple_type_text_elements_preserving_rest(param_type_text)
        else {
            return;
        };
        let spread_params = param_elements
            .iter()
            .filter_map(|element| {
                let name = element.trim().strip_prefix("...")?.trim();
                type_param_names
                    .iter()
                    .any(|known| known.as_str() == name)
                    .then_some(name)
            })
            .collect::<Vec<_>>();
        let [type_param_name] = spread_params.as_slice() else {
            return;
        };
        if substitutions
            .iter()
            .any(|(name, _)| name.as_str() == *type_param_name)
        {
            return;
        }
        let Some(spread_index) = param_elements
            .iter()
            .position(|element| element.trim() == format!("...{type_param_name}"))
        else {
            return;
        };
        let fixed_prefix = spread_index;
        let fixed_suffix = param_elements.len().saturating_sub(spread_index + 1);
        let Some(argument_text) = self.tuple_spread_argument_type_text(
            arg_idx,
            Self::type_param_constraint_text(type_param_constraints, type_param_name),
        ) else {
            return;
        };
        let value_text = if let Some(argument_elements) =
            Self::tuple_type_text_elements_preserving_rest(&argument_text)
        {
            if argument_elements.len() < fixed_prefix + fixed_suffix {
                return;
            }
            let end = argument_elements.len() - fixed_suffix;
            format!("[{}]", argument_elements[fixed_prefix..end].join(", "))
        } else if fixed_prefix == 0 && fixed_suffix == 0 {
            argument_text
        } else {
            return;
        };
        substitutions.push(((*type_param_name).to_string(), value_text));
    }

    fn tuple_spread_argument_type_text(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<String> {
        self.array_literal_tuple_argument_type_text(arg_idx)
            .or_else(|| {
                self.call_argument_type_text_for_substitution(arg_idx, type_param_constraint)
            })
    }

    pub(in crate::declaration_emitter) fn array_literal_tuple_argument_type_text(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(arg_idx);
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = self.arena.get_literal_expr(arg_node)?;
        let mut elements = Vec::new();
        for &element_idx in &array.elements.nodes {
            let element_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(element_idx);
            let element_node = self.arena.get(element_idx)?;
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let spread = self.arena.get_spread(element_node)?;
                let spread_text =
                    self.call_argument_type_text_for_substitution(spread.expression, None)?;
                elements.push(format!("...{spread_text}"));
                continue;
            }
            elements.push(
                self.widened_tuple_literal_element_type_text(element_idx)
                    .or_else(|| self.call_argument_type_text_for_substitution(element_idx, None))?,
            );
        }
        Some(format!("[{}]", elements.join(", ")))
    }

    fn widened_tuple_literal_element_type_text(&self, element_idx: NodeIndex) -> Option<String> {
        let element_node = self.arena.get(element_idx)?;
        match element_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            _ => None,
        }
    }

    pub(super) fn infer_variadic_function_type_substitutions(
        source: &super::type_inference_function_text::FunctionTypeTextParts,
        argument: &super::type_inference_function_text::FunctionTypeTextParts,
        type_param_names: &[String],
        known_substitutions: &[(String, String)],
        substitutions: &mut Vec<(String, String)>,
    ) {
        for source_param in &source.parameters {
            if !source_param.rest {
                continue;
            }
            let Some(source_elements) =
                Self::tuple_type_text_elements_preserving_rest(&source_param.type_text)
            else {
                continue;
            };
            let mut argument_index = 0usize;
            for (element_index, element) in source_elements.iter().enumerate() {
                let Some(type_param_name) = element.trim().strip_prefix("...").map(str::trim)
                else {
                    argument_index += 1;
                    continue;
                };
                if !type_param_names
                    .iter()
                    .any(|name| name.as_str() == type_param_name)
                {
                    continue;
                }
                if let Some((_, known_text)) = known_substitutions
                    .iter()
                    .chain(substitutions.iter())
                    .find(|(name, _)| name.as_str() == type_param_name)
                    && let Some(known_elements) =
                        Self::tuple_type_text_elements_preserving_rest(known_text)
                {
                    argument_index = Self::advance_variadic_argument_index(
                        argument,
                        argument_index,
                        known_elements.len(),
                    );
                    continue;
                }
                if substitutions
                    .iter()
                    .any(|(name, _)| name.as_str() == type_param_name)
                {
                    continue;
                }
                let remaining_spreads = source_elements[element_index + 1..]
                    .iter()
                    .filter(|candidate| candidate.trim().starts_with("..."))
                    .count();
                if remaining_spreads != 0 {
                    continue;
                }
                let tuple_items = argument
                    .parameters
                    .iter()
                    .skip(argument_index)
                    .map(Self::variadic_tuple_item_text_for_function_param)
                    .collect::<Vec<_>>();
                substitutions.push((
                    type_param_name.to_string(),
                    format!("[{}]", tuple_items.join(", ")),
                ));
            }
        }
    }

    fn advance_variadic_argument_index(
        argument: &super::type_inference_function_text::FunctionTypeTextParts,
        mut argument_index: usize,
        known_element_count: usize,
    ) -> usize {
        for _ in 0..known_element_count {
            let Some(argument_param) = argument.parameters.get(argument_index) else {
                break;
            };
            if argument_param.rest {
                break;
            }
            argument_index += 1;
        }
        argument_index
    }

    fn variadic_tuple_item_text_for_function_param(
        param: &super::type_inference_function_text::FunctionTypeParamText,
    ) -> String {
        let type_text = param.type_text.trim();
        if param.rest {
            if let Some(name) = param.name.as_deref() {
                return format!("...{name}: {type_text}");
            }
            return format!("...{type_text}");
        }
        if let Some(name) = param.name.as_deref() {
            if param.optional {
                return format!("{name}?: {type_text}");
            }
            return format!("{name}: {type_text}");
        }
        type_text.to_string()
    }

    fn variadic_tuple_return_parts(type_text: &str) -> Option<(String, Vec<String>)> {
        let elements = Self::tuple_type_elements_text(type_text)?;
        let first = elements.first()?.trim();
        let type_param_name = first.strip_prefix("...")?.trim();
        if !Self::is_simple_identifier_text(type_param_name) {
            return None;
        }
        Some((
            type_param_name.to_string(),
            elements.into_iter().skip(1).collect(),
        ))
    }

    fn variadic_tuple_param_tail_count(type_text: &str, type_param_name: &str) -> Option<usize> {
        let elements = Self::tuple_type_elements_text(type_text)?;
        let spread_text = format!("...{type_param_name}");
        let spread_index = elements
            .iter()
            .position(|element| element.trim() == spread_text)?;
        Some(elements.len().saturating_sub(spread_index + 1))
    }

    fn tuple_type_elements_text(type_text: &str) -> Option<Vec<String>> {
        let trimmed = type_text
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or(type_text.trim());
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    fn type_parameter_constraint_text(
        &self,
        type_parameters: Option<&NodeList>,
        type_param_name: &str,
    ) -> Option<String> {
        let type_parameters = type_parameters?;
        for &param_idx in &type_parameters.nodes {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_type_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(type_param_name) {
                continue;
            }
            return self
                .emit_type_node_text(param.constraint)
                .or_else(|| self.source_slice_from_arena(self.arena, param.constraint));
        }
        None
    }

    fn variadic_tuple_literal_prefix_elements(
        &self,
        args: &[NodeIndex],
        constraint_elements: &[String],
    ) -> Option<Vec<String>> {
        let required_prefix = constraint_elements
            .iter()
            .take_while(|element| !element.trim_start().starts_with("..."))
            .count();
        if args.len() < required_prefix {
            return None;
        }
        let rest_constraint = constraint_elements
            .iter()
            .find_map(|element| element.trim().strip_prefix("..."))
            .and_then(|rest| rest.trim().strip_suffix("[]"))
            .map(str::trim);
        args.iter()
            .enumerate()
            .map(|(index, arg_idx)| {
                let expected = if index < required_prefix {
                    constraint_elements
                        .get(index)
                        .map(|element| element.trim())?
                } else {
                    rest_constraint?
                };
                let actual = self.literal_argument_type_text(*arg_idx)?;
                self.literal_type_matches_constraint(&actual, expected)
                    .then_some(actual)
            })
            .collect()
    }

    fn literal_argument_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(arg_idx);
        let node = self.arena.get(arg_idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.arena.get_literal(node)?;
                Some(format!(
                    "\"{}\"",
                    super::escape_string_for_double_quote(&lit.text)
                ))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .get_source_slice(node.pos, node.end)
                .map(|text| text.trim().to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            _ => None,
        }
    }

    fn literal_type_matches_constraint(&self, literal_type: &str, constraint: &str) -> bool {
        match constraint {
            "string" => literal_type.starts_with('"') && literal_type.ends_with('"'),
            "number" => literal_type.parse::<f64>().is_ok(),
            "boolean" => matches!(literal_type, "true" | "false"),
            _ => literal_type == constraint,
        }
    }
}
