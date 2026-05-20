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
        if let Some(evaluated) = self.evaluate_source_template_infer_conditional_call(
            source_arena,
            func,
            call,
            &type_text,
        ) {
            return Some(evaluated);
        }

        let Some(type_params) = func.type_parameters.as_ref() else {
            return Some(type_text);
        };
        if type_params.nodes.is_empty() {
            return Some(type_text);
        }

        let mut type_param_names = Vec::new();
        let mut type_param_constraints = Vec::new();
        let mut type_param_defaults = Vec::new();
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
            if param.default.is_some()
                && let Some(default_text) = self
                    .emit_type_node_text_from_arena(source_arena, param.default)
                    .or_else(|| self.source_slice_from_arena(source_arena, param.default))
            {
                type_param_defaults.push((name_text.clone(), default_text));
            }
            type_param_names.push(name_text);
        }

        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        let mut substitutions = if explicit_type_args.is_empty() {
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
        for (name_text, default_text) in type_param_defaults {
            if substitutions
                .iter()
                .any(|(substituted, _)| substituted == &name_text)
                || !Self::contains_whole_word_in_text(&type_text, &name_text)
            {
                continue;
            }
            let default_text = Self::replace_whole_words_in_text(&default_text, &substitutions);
            substitutions.push((name_text, default_text));
        }
        if substitutions.is_empty()
            && type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
        {
            return None;
        }
        type_text = Self::replace_whole_words_in_text(&type_text, &substitutions);
        type_text = Self::simplify_string_literal_template_type_text(&type_text);
        type_text = Self::expand_literal_key_mapped_type_text(&type_text).unwrap_or(type_text);
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

    fn expand_literal_key_mapped_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        let mapped = inner.strip_prefix('[')?;
        let in_pos = mapped.find(" in ")?;
        let after_in = mapped.get(in_pos + " in ".len()..)?;
        let end_bracket = after_in.find(']')?;
        let keys_text = after_in.get(..end_bracket)?.trim();
        let after_bracket = after_in.get(end_bracket + 1..)?.trim();
        let value_text = after_bracket
            .strip_prefix(':')?
            .trim()
            .trim_end_matches(';')
            .trim();
        if value_text.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        for key in Self::split_top_level_union_type_parts(keys_text) {
            let key = key.trim();
            let key = Self::unquoted_string_literal_text(key)?;
            if !Self::is_simple_identifier_text(&key) {
                return None;
            }
            lines.push(format!("    {key}: {value_text};"));
        }
        (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
    }

    fn simplify_string_literal_template_type_text(type_text: &str) -> String {
        let mut output = String::with_capacity(type_text.len());
        let bytes = type_text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'`' {
                output.push(bytes[i] as char);
                i += 1;
                continue;
            }
            if let Some((replacement, next)) = Self::try_simplify_template_literal_at(type_text, i)
            {
                output.push_str(&replacement);
                i = next;
            } else if let Some(end) = type_text.get(i + 1..).and_then(|text| text.find('`')) {
                let end = i + 1 + end + 1;
                output.push_str(type_text.get(i..end).unwrap_or("`"));
                i = end;
            } else {
                output.push('`');
                i += 1;
            }
        }
        output
    }

    fn try_simplify_template_literal_at(type_text: &str, start: usize) -> Option<(String, usize)> {
        let bytes = type_text.as_bytes();
        let mut i = start + 1;
        let mut value = String::new();
        while i < bytes.len() {
            match bytes[i] {
                b'`' => return Some((format!("{value:?}"), i + 1)),
                b'$' if bytes.get(i + 1) == Some(&b'{') => {
                    let expr_start = i + 2;
                    let expr_end = type_text.get(expr_start..)?.find('}')? + expr_start;
                    let literal = type_text.get(expr_start..expr_end)?.trim();
                    let literal = Self::unquoted_string_literal_text(literal)?;
                    value.push_str(&literal);
                    i = expr_end + 1;
                }
                b'\\' => return None,
                byte => {
                    value.push(byte as char);
                    i += 1;
                }
            }
        }
        None
    }

    fn unquoted_string_literal_text(literal: &str) -> Option<String> {
        let quote = literal.as_bytes().first().copied()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        if literal.as_bytes().last().copied() != Some(quote) {
            return None;
        }
        Some(literal.get(1..literal.len() - 1)?.to_string())
    }

    pub(in crate::declaration_emitter) fn evaluate_source_template_infer_conditional_call(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        type_text: &str,
    ) -> Option<String> {
        let (type_param_name, prefix, suffix, false_branch) =
            Self::parse_template_infer_conditional_text(type_text)?;
        if false_branch != "unknown" {
            return None;
        }

        let arguments = call.arguments.as_ref()?;
        let param_index = func.parameters.nodes.iter().position(|&param_idx| {
            let Some(param_node) = source_arena.get(param_idx) else {
                return false;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                return false;
            };
            self.emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
                .is_some_and(|text| text.trim() == type_param_name)
        })?;
        let arg_idx = *arguments.nodes.get(param_index)?;

        self.evaluate_template_infer_argument(arg_idx, &prefix, &suffix)
    }

    fn parse_template_infer_conditional_text(
        type_text: &str,
    ) -> Option<(String, String, String, String)> {
        let trimmed = type_text.trim();
        let (check_type, rest) = trimmed.split_once(" extends ")?;
        let (pattern_text, branches) = rest.split_once(" ? ")?;
        let (true_branch, false_branch) = branches.split_once(" : ")?;

        let pattern = pattern_text.trim().strip_prefix('`')?.strip_suffix('`')?;
        let infer_marker = "${infer ";
        let infer_start = pattern.find(infer_marker)?;
        let infer_name_start = infer_start + infer_marker.len();
        let infer_name_end = pattern.get(infer_name_start..)?.find('}')? + infer_name_start;
        let infer_name = pattern.get(infer_name_start..infer_name_end)?.trim();
        if infer_name.is_empty() || true_branch.trim() != infer_name {
            return None;
        }

        let prefix = pattern.get(..infer_start)?.to_string();
        let suffix = pattern.get(infer_name_end + 1..)?.to_string();
        Some((
            check_type.trim().to_string(),
            prefix,
            suffix,
            false_branch.trim().to_string(),
        ))
    }

    fn evaluate_template_infer_argument(
        &self,
        arg_idx: tsz_parser::parser::NodeIndex,
        prefix: &str,
        suffix: &str,
    ) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        match arg_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(arg_node)?;
                Some(Self::template_infer_capture_text(
                    &literal.text,
                    prefix,
                    suffix,
                ))
            }
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(literal) = self.const_string_literal_initializer_for_identifier(arg_idx)
                {
                    return Some(Self::template_infer_capture_text(&literal, prefix, suffix));
                }
                Some("unknown".to_string())
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.template_expression_infer_capture_text(arg_idx, prefix, suffix)
            }
            _ => None,
        }
    }

    fn const_string_literal_initializer_for_identifier(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !self.arena.is_const_variable_declaration(decl_idx) {
                continue;
            }
            let Some(init_node) = self.arena.get(decl.initializer) else {
                continue;
            };
            if init_node.kind == SyntaxKind::StringLiteral as u16
                || init_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                return self
                    .arena
                    .get_literal(init_node)
                    .map(|lit| lit.text.clone());
            }
        }
        None
    }

    fn template_expression_infer_capture_text(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
        prefix: &str,
        suffix: &str,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let template = self.arena.get_template_expr(expr_node)?;
        let spans = &template.template_spans.nodes;
        if spans.len() != 1 || !suffix.is_empty() {
            return None;
        }
        let head_node = self.arena.get(template.head)?;
        let head_text = self.arena.get_literal(head_node)?.text.as_str();
        if head_text != prefix {
            return Some("unknown".to_string());
        }
        let span_node = self.arena.get(spans[0])?;
        let span = self.arena.get_template_span(span_node)?;
        let tail_node = self.arena.get(span.literal)?;
        if self.arena.get_literal(tail_node)?.text.as_str() != suffix {
            return Some("unknown".to_string());
        }

        self.template_expression_hole_type_text(span.expression)
            .map(|text| Self::normalize_string_literal_union_quotes(&text))
    }

    fn template_expression_hole_type_text(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        self.reference_declared_type_annotation_text(expr_idx)
            .or_else(|| self.const_literal_initializer_text(expr_idx))
            .or_else(|| {
                self.get_node_type_or_names(&[expr_idx])
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            })
            .filter(|text| text != "any" && text != "unknown")
    }

    fn template_infer_capture_text(value: &str, prefix: &str, suffix: &str) -> String {
        let Some(captured) = value
            .strip_prefix(prefix)
            .and_then(|text| text.strip_suffix(suffix))
        else {
            return "unknown".to_string();
        };
        let escaped = super::escape_string_for_double_quote(captured);
        format!("\"{escaped}\"")
    }

    fn normalize_string_literal_union_quotes(type_text: &str) -> String {
        let parts = Self::split_top_level_union_type_parts(type_text);
        if parts.len() <= 1 {
            return Self::normalize_string_literal_quotes(type_text.trim());
        }
        parts
            .iter()
            .map(|part| Self::normalize_string_literal_quotes(part))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn normalize_string_literal_quotes(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.len() >= 2
            && trimmed.starts_with('\'')
            && trimmed.ends_with('\'')
            && !trimmed[1..trimmed.len() - 1].contains('\'')
        {
            let inner = &trimmed[1..trimmed.len() - 1];
            let escaped = super::escape_string_for_double_quote(inner);
            format!("\"{escaped}\"")
        } else {
            trimmed.to_string()
        }
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
