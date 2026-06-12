//! Variadic tuple surface reconstruction for generic call declaration emit.

use super::super::DeclarationEmitter;
use super::generic_call_literal::{
    callable_function_from_symbol_decl, function_declares_type_parameter,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn generic_spread_array_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            let return_expr = self.function_body_single_return_expression(func.body)?;
            let return_node = source_arena.get(return_expr)?;
            if return_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                return None;
            }
            let literal = source_arena.get_literal_expr(return_node)?;
            if literal.elements.nodes.is_empty() {
                return None;
            }

            let mut substitutions = Vec::new();
            let mut return_parts = Vec::new();
            for &element_idx in &literal.elements.nodes {
                let element_node = source_arena.get(element_idx)?;
                if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                    return None;
                }
                let spread = source_arena.get_spread(element_node)?;
                let spread_name =
                    self.identifier_text_from_arena(source_arena, spread.expression)?;
                let param_index = func.parameters.nodes.iter().position(|&param_idx| {
                    source_arena
                        .get(param_idx)
                        .and_then(|node| source_arena.get_parameter(node))
                        .and_then(|param| self.identifier_text_from_arena(source_arena, param.name))
                        .is_some_and(|name| name == spread_name)
                })?;
                let param_idx = func.parameters.nodes[param_index];
                let param = source_arena
                    .get(param_idx)
                    .and_then(|node| source_arena.get_parameter(node))?;
                let param_type_text = self
                    .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                    .or_else(|| {
                        self.source_slice_from_arena(source_arena, param.type_annotation)
                    })?;
                let param_type_text = param_type_text.trim();
                if !function_declares_type_parameter(source_arena, func, param_type_text) {
                    return None;
                }
                let arg_idx = arguments.nodes.get(param_index).copied()?;
                let arg_type_text = self.call_argument_type_text_for_substitution(arg_idx, None)?;
                substitutions.push((param_type_text.to_string(), arg_type_text));
                return_parts.push(format!("{param_type_text}[number]"));
            }

            let return_text = format!("({})[]", return_parts.join(" | "));
            let return_text =
                Self::expand_tuple_index_substitutions_text(&return_text, &substitutions);
            let return_text = Self::replace_whole_words_in_text(&return_text, &substitutions);
            (!return_text.contains("unknown")).then_some(return_text)
        })
    }

    pub(super) fn generic_mapped_tuple_rest_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            let return_text = self
                .emit_type_node_text_from_arena(source_arena, func.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, func.type_annotation))?;
            let return_text = return_text.trim();
            let type_param_names = func
                .type_parameters
                .as_ref()?
                .nodes
                .iter()
                .copied()
                .filter_map(|param_idx| {
                    source_arena
                        .get(param_idx)
                        .and_then(|node| source_arena.get_type_parameter(node))
                        .and_then(|param| self.identifier_text_from_arena(source_arena, param.name))
                })
                .collect::<Vec<_>>();
            if !type_param_names.iter().any(|name| name == return_text) {
                return None;
            }

            func.parameters
                .nodes
                .iter()
                .copied()
                .zip(arguments.nodes.iter().copied())
                .find_map(|(param_idx, arg_idx)| {
                    let param = source_arena
                        .get(param_idx)
                        .and_then(|node| source_arena.get_parameter(node))?;
                    let (name, value) = self.infer_mapped_tuple_spread_argument_substitution(
                        source_arena,
                        param.type_annotation,
                        arg_idx,
                        &type_param_names,
                    )?;
                    (name == return_text).then_some(value)
                })
        })
    }

    pub(super) fn generic_curried_variadic_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;
        let first_arg = arguments.nodes.first().copied()?;
        let argument_parts = self.function_type_parts_for_expression(first_arg)?;

        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            if func.parameters.nodes.len() < 2 {
                return None;
            }

            let first_param_idx = func.parameters.nodes[0];
            let first_param = source_arena
                .get(first_param_idx)
                .and_then(|node| source_arena.get_parameter(node))?;
            let first_param_text = self
                .emit_type_node_text_from_arena(source_arena, first_param.type_annotation)
                .or_else(|| {
                    self.source_slice_from_arena(source_arena, first_param.type_annotation)
                })?;
            let first_param_parts = Self::parse_function_type_text(&first_param_text)?;
            let [source_rest_param] = first_param_parts.parameters.as_slice() else {
                return None;
            };
            if !source_rest_param.rest {
                return None;
            }
            let source_rest_elements =
                Self::tuple_type_text_elements_preserving_rest(&source_rest_param.type_text)?;
            if source_rest_elements.len() != 2
                || !source_rest_elements.iter().all(|element| {
                    element.trim().strip_prefix("...").is_some_and(|name| {
                        function_declares_type_parameter(source_arena, func, name.trim())
                    })
                })
            {
                return None;
            }

            let rest_param_idx = func.parameters.nodes[1];
            let rest_param = source_arena
                .get(rest_param_idx)
                .and_then(|node| source_arena.get_parameter(node))?;
            if !rest_param.dot_dot_dot_token {
                return None;
            }
            let rest_param_type = self
                .emit_type_node_text_from_arena(source_arena, rest_param.type_annotation)
                .or_else(|| {
                    self.source_slice_from_arena(source_arena, rest_param.type_annotation)
                })?;
            let first_tuple_param = source_rest_elements[0].trim().strip_prefix("...")?.trim();
            if rest_param_type.trim() != first_tuple_param {
                return None;
            }
            let return_type_param = first_param_parts.return_type.trim();
            if !function_declares_type_parameter(source_arena, func, return_type_param) {
                return None;
            }

            let return_expr = self.function_body_single_return_expression(func.body)?;
            let return_node = source_arena.get(return_expr)?;
            let return_func = source_arena.get_function(return_node)?;
            let [return_param_idx] = return_func.parameters.nodes.as_slice() else {
                return None;
            };
            let return_param = source_arena
                .get(*return_param_idx)
                .and_then(|node| source_arena.get_parameter(node))?;
            if !return_param.dot_dot_dot_token {
                return None;
            }
            let return_rest_name = self
                .identifier_text_from_arena(source_arena, return_param.name)
                .unwrap_or_else(|| "args".to_string());

            let consumed = arguments.nodes.len().saturating_sub(1);
            let fixed_count = argument_parts
                .parameters
                .iter()
                .take_while(|param| !param.rest)
                .count();
            let start = consumed.min(fixed_count);
            let mut params = argument_parts
                .parameters
                .iter()
                .skip(start)
                .map(|param| {
                    let type_text = param.type_text.trim();
                    if param.rest {
                        let name = if start >= fixed_count {
                            return_rest_name.as_str()
                        } else {
                            param.name.as_deref().unwrap_or(return_rest_name.as_str())
                        };
                        format!("...{name}: {type_text}")
                    } else if let Some(name) = param.name.as_deref() {
                        if param.optional {
                            format!("{name}?: {type_text}")
                        } else {
                            format!("{name}: {type_text}")
                        }
                    } else {
                        type_text.to_string()
                    }
                })
                .collect::<Vec<_>>();
            if consumed > fixed_count && argument_parts.parameters.last().is_some_and(|p| p.rest) {
                params = vec![format!(
                    "...{}: {}",
                    return_rest_name,
                    argument_parts.parameters.last()?.type_text.trim()
                )];
            }
            Some(format!(
                "({}) => {}",
                params.join(", "),
                argument_parts.return_type.trim()
            ))
        })
    }
}
