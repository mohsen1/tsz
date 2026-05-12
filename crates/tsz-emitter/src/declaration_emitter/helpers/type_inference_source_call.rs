//! Source-call type-parameter substitution helpers for declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
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

        let substitutions = self.infer_call_type_param_substitutions_from_arguments(
            source_arena,
            &func.parameters,
            call,
            &type_param_names,
            &type_param_constraints,
        );
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
