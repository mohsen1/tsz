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
