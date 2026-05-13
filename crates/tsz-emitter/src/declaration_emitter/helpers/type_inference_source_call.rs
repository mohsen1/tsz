//! Source call return type recovery for declaration emit.

use super::super::DeclarationEmitter;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::{CallExprData, FunctionData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_expression_source_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.arena);

        let mut function_decl_count = 0usize;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(_func) = self.callable_function_from_symbol_decl(source_arena, decl_idx)
            else {
                continue;
            };
            function_decl_count += 1;
            if function_decl_count > 1 {
                return None;
            }
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(func) = self.callable_function_from_symbol_decl(source_arena, decl_idx) else {
                continue;
            };
            if func.type_annotation.is_some() {
                if let Some(type_text) =
                    self.source_slice_from_arena(source_arena, func.type_annotation)
                    && self.source_return_type_annotation_is_reusable(
                        source_arena,
                        func.type_annotation,
                    )
                {
                    let type_text = type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string();
                    if call.type_arguments.is_none()
                        && self.source_return_type_mentions_type_parameter(
                            source_arena,
                            func,
                            &type_text,
                        )
                    {
                        continue;
                    }
                    return self.substitute_source_call_type_parameters(
                        source_arena,
                        func,
                        call,
                        type_text,
                    );
                }
            } else if func.body.is_some()
                && !self.source_function_body_contains_direct_call_to_name(
                    source_arena,
                    func,
                    &symbol.escaped_name,
                )
                && let Some(type_text) = {
                    let mut scratch = if std::ptr::eq(source_arena, self.arena)
                        && let (Some(type_cache), Some(type_interner), Some(binder)) =
                            (&self.type_cache, self.type_interner, self.binder)
                    {
                        DeclarationEmitter::with_type_info(
                            source_arena,
                            type_cache.clone(),
                            type_interner,
                            binder,
                        )
                    } else {
                        DeclarationEmitter::new(source_arena)
                    };
                    let source_file = self.arena_source_file(source_arena)?;
                    scratch.source_is_declaration_file = source_file.is_declaration_file;
                    scratch.source_is_js_file = scratch.source_file_is_js(source_file);
                    scratch.current_source_file_idx = self.current_source_file_idx;
                    scratch.source_file_text = Some(source_file.text.clone());
                    scratch.current_file_path = self.current_file_path.clone();
                    scratch.current_arena = self.current_arena.clone();
                    scratch.arena_to_path = self.arena_to_path.clone();
                    scratch.indent_level = self.indent_level;
                    let generic_source_func = func
                        .type_parameters
                        .as_ref()
                        .is_some_and(|params| !params.nodes.is_empty());
                    let mut type_text = scratch.source_function_return_type_text(func)?;
                    let source_return_text = scratch
                        .function_body_returned_parameter_call_return_type_text(source_arena, func);
                    if generic_source_func {
                        type_text = source_return_text?;
                    } else if type_text.contains("unknown")
                        && let Some(source_return_text) = source_return_text
                    {
                        type_text = source_return_text;
                    }
                    let type_text =
                        scratch.substitute_call_result_parameter_type_queries(func, &type_text);
                    let (type_text, _) =
                        scratch.function_return_type_text_for_declaration_scope(func, &type_text);
                    scratch.substitute_source_call_type_parameters(
                        source_arena,
                        func,
                        call,
                        type_text,
                    )
                }
            {
                return Some(Self::strip_synthetic_anonymous_object_members(&type_text));
            }
        }

        None
    }

    fn source_function_body_contains_direct_call_to_name(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
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

    fn source_return_type_mentions_type_parameter(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
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

    fn function_body_returned_parameter_call_return_type_text(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
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

    fn substitute_source_call_type_parameters(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        call: &CallExprData,
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

    fn substitute_call_result_parameter_type_queries(
        &self,
        func: &FunctionData,
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

    fn source_function_return_type_text(&self, func: &FunctionData) -> Option<String> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            return self.function_body_preferred_return_type_text(func.body);
        }

        self.preferred_expression_type_text(func.body)
            .or_else(|| self.infer_fallback_type_text_at(func.body, 0))
            .filter(|text| !text.is_empty() && text != "any")
    }

    fn source_return_type_annotation_is_reusable(
        &self,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(binder) = self.binder else {
            return true;
        };
        let Some(type_node) = source_arena.get(type_annotation) else {
            return true;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return true;
        }
        let Some(type_ref) = source_arena.get_type_ref(type_node) else {
            return true;
        };
        let Some(name_node) = source_arena.get(type_ref.type_name) else {
            return true;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return true;
        }

        let Some(sym_id) = binder
            .get_node_symbol(type_ref.type_name)
            .or_else(|| binder.resolve_identifier(source_arena, type_ref.type_name))
        else {
            return true;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return true;
        };
        let parent_id = symbol.parent;
        if parent_id == SymbolId::NONE
            || self.enclosing_namespace_symbol == Some(parent_id)
            || symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return true;
        }
        let Some(parent) = binder.symbols.get(parent_id) else {
            return true;
        };
        if !parent.has_any_flags(symbol_flags::NAMESPACE | symbol_flags::ENUM) {
            return true;
        }
        if !symbol.is_exported && !symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
            return false;
        }
        parent.is_exported || parent.has_any_flags(symbol_flags::EXPORT_VALUE)
    }
}
