//! Source-call type-parameter substitution helpers for declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn source_function_body_contains_direct_call_to_name(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        name: &str,
    ) -> bool {
        if name.is_empty() {
            return false;
        }
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return false;
        };
        let Some(body_node) = source_arena.get(func.body) else {
            return false;
        };
        let Ok(start) = usize::try_from(body_node.pos) else {
            return false;
        };
        let Ok(end) = usize::try_from(body_node.end) else {
            return false;
        };
        let Some(body_text) = source_file.text.get(start..end) else {
            return false;
        };

        let mut search = body_text;
        while let Some(offset) = search.find(name) {
            let after_name = &search[offset + name.len()..];
            let after_ws = after_name.trim_start();
            if after_ws.starts_with('(') || after_ws.starts_with('<') {
                return true;
            }
            search = after_name;
        }
        false
    }

    pub(in crate::declaration_emitter) fn function_body_returned_parameter_call_return_type_text(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = source_arena.get(func.body)?;
        let block = source_arena.get_block(body_node)?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_node = source_arena.get(*block.statements.nodes.first()?)?;
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return None;
        }
        let ret = source_arena.get_return_statement(stmt_node)?;
        let return_expr = self.skip_parenthesized_expression(ret.expression)?;
        let call_node = source_arena.get(return_expr)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = source_arena.get_call_expr(call_node)?;
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_node = source_arena.get(callee_idx)?;
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let callee_name = self.identifier_text_from_arena(source_arena, callee_idx)?;
        for &param_idx in &func.parameters.nodes {
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            if self
                .identifier_text_from_arena(source_arena, param.name)
                .as_deref()
                != Some(callee_name.as_str())
            {
                continue;
            }
            let param_type_text = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))?;
            let parts = Self::parse_function_type_text(&param_type_text)?;
            return Some(parts.return_type);
        }
        None
    }

    pub(in crate::declaration_emitter) fn source_return_type_mentions_type_parameter(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        type_text: &str,
    ) -> bool {
        let Some(type_params) = func.type_parameters.as_ref() else {
            return false;
        };
        type_params.nodes.iter().copied().any(|param_idx| {
            source_arena
                .get(param_idx)
                .and_then(|param_node| source_arena.get_type_parameter(param_node))
                .and_then(|param| self.identifier_text_from_arena(source_arena, param.name))
                .is_some_and(|name| Self::contains_whole_word_in_text(type_text, &name))
        })
    }

    pub(in crate::declaration_emitter) fn substitute_source_call_type_parameters(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        mut type_text: String,
    ) -> Option<String> {
        let Some(type_params) = func.type_parameters.as_ref() else {
            return Some(type_text);
        };
        if type_params.nodes.is_empty() {
            return Some(type_text);
        }

        let mut type_param_names = Vec::new();
        let mut type_param_constraints = Vec::new();
        for &param_idx in &type_params.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_text) = self.identifier_text_from_arena(source_arena, param.name) else {
                continue;
            };
            if param.constraint.is_some()
                && let Some(constraint) = self
                    .emit_type_node_text_from_arena(source_arena, param.constraint)
                    .or_else(|| self.source_slice_from_arena(source_arena, param.constraint))
            {
                type_param_constraints.push((name_text.clone(), constraint));
            }
            type_param_names.push(name_text);
        }

        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        let substitutions = if explicit_type_args.is_empty() {
            self.infer_call_type_param_substitutions_from_arguments(
                source_arena,
                &func.parameters,
                call,
                &type_param_names,
                &type_param_constraints,
            )
        } else {
            type_param_names
                .iter()
                .zip(explicit_type_args.iter())
                .map(|(name_text, arg_text)| (name_text.clone(), arg_text.clone()))
                .collect()
        };
        if substitutions.is_empty()
            && type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
        {
            return None;
        }
        type_text = Self::replace_whole_words_in_text(&type_text, &substitutions);
        if type_param_names
            .iter()
            .any(|name| Self::contains_whole_word_in_text(&type_text, name))
        {
            return None;
        }
        if type_text.contains("unknown") {
            return None;
        }
        Some(type_text)
    }

    pub(in crate::declaration_emitter) fn source_call_return_reuse_allows_inferred_type_parameters(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> bool {
        let (Some(type_params), Some(args)) =
            (func.type_parameters.as_ref(), call.arguments.as_ref())
        else {
            return false;
        };
        let type_param_names = type_params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_type_parameter(param_node)?;
                self.identifier_text_from_arena(source_arena, param.name)
            })
            .collect::<Vec<_>>();
        if type_param_names.is_empty() {
            return false;
        }

        for (&param_idx, &arg_idx) in func.parameters.nodes.iter().zip(args.nodes.iter()) {
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
            let param_parts = Self::split_top_level_intersection_parts(param_type_text.trim());
            if param_parts.len() < 2 {
                continue;
            }

            let bare_type_param_count = param_parts
                .iter()
                .filter(|part| type_param_names.iter().any(|name| name == *part))
                .count();
            if bare_type_param_count != 1 {
                continue;
            }

            let wrapper_parts = param_parts
                .iter()
                .filter_map(|part| {
                    let (wrapper, inner) = Self::single_generic_type_argument_text(part)?;
                    type_param_names
                        .iter()
                        .any(|name| name == inner)
                        .then_some(wrapper.to_string())
                })
                .collect::<Vec<_>>();
            if wrapper_parts.is_empty() {
                continue;
            }

            let Some(arg_type_text) = self
                .get_node_type_or_names(&[arg_idx])
                .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                .filter(|type_text| {
                    type_text != "any"
                        && type_text != "unknown"
                        && !type_text.contains("any")
                        && !type_text.contains("unknown")
                })
                .or_else(|| self.call_argument_type_text_for_substitution(arg_idx, None))
            else {
                continue;
            };
            let arg_parts = Self::split_top_level_intersection_parts(&arg_type_text);
            if arg_parts.len() < 2 {
                continue;
            }
            if wrapper_parts.iter().all(|wrapper| {
                arg_parts.iter().any(|arg_part| {
                    Self::single_generic_type_argument_text(arg_part)
                        .is_some_and(|(arg_wrapper, _)| arg_wrapper == wrapper)
                })
            }) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn substitute_call_result_parameter_type_queries(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        source_type_text: &str,
    ) -> String {
        if !source_type_text.contains("typeof ") {
            return source_type_text.to_string();
        }

        let mut text = source_type_text.to_string();
        for param_idx in func.parameters.nodes.iter().copied() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_name) = self.get_identifier_text(param.name) else {
                continue;
            };
            let Some(param_type_text) = self.function_parameter_type_text(func, param.name) else {
                continue;
            };
            if !Self::type_text_can_substitute_type_query_parameter(&param_type_text) {
                continue;
            }
            text = Self::replace_typeof_identifier(&text, &param_name, &param_type_text).0;
        }
        text
    }

    fn type_text_can_substitute_type_query_parameter(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        if Self::simple_type_reference_name(trimmed).is_some() {
            return true;
        }
        if matches!(trimmed, "true" | "false" | "null" | "undefined") {
            return true;
        }
        if trimmed.parse::<f64>().is_ok() {
            return true;
        }
        if trimmed.len() >= 2 {
            let bytes = trimmed.as_bytes();
            return (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'');
        }
        false
    }
}
