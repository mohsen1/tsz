//! Source callable and new-expression helpers for declaration type inference.

use super::super::DeclarationEmitter;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::type_queries;

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
                        if let Some(evaluated) = self
                            .evaluate_source_template_infer_conditional_call(
                                source_arena,
                                func,
                                call,
                                &type_text,
                            )
                        {
                            return Some(evaluated);
                        }
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
                    scratch.strict_null_checks = self.strict_null_checks;
                    let generic_source_func = func
                        .type_parameters
                        .as_ref()
                        .is_some_and(|params| !params.nodes.is_empty());
                    let mut type_text =
                        scratch.source_function_return_type_text(func).or_else(|| {
                            scratch.source_function_cached_generic_return_type_text(decl_idx, func)
                        })?;
                    let source_return_text = scratch
                        .function_body_returned_parameter_call_return_type_text(source_arena, func);
                    if generic_source_func && let Some(source_return_text) = source_return_text {
                        type_text = source_return_text;
                    } else if type_text.contains("unknown")
                        && let Some(source_return_text) = source_return_text
                    {
                        type_text = source_return_text;
                    }
                    let type_text =
                        scratch.substitute_call_result_parameter_type_queries(func, &type_text);
                    let (type_text, _) =
                        scratch.function_return_type_text_for_declaration_scope(func, &type_text);
                    let type_text = scratch.substitute_source_call_type_parameters(
                        source_arena,
                        func,
                        call,
                        type_text,
                    )?;
                    Some(
                        scratch
                            .expand_inexact_optional_alias_reference_text(source_arena, &type_text)
                            .unwrap_or(type_text),
                    )
                }
            {
                return Some(Self::strip_synthetic_anonymous_object_members(&type_text));
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn callable_function_from_symbol_decl<'b>(
        &self,
        source_arena: &'b NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<&'b tsz_parser::parser::node::FunctionData> {
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

    fn source_function_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(return_expr) = self.single_return_expression(func.body)
                && let Some(type_text) = self.as_const_assertion_type_text(return_expr)
            {
                return Some(type_text);
            }
            return self.function_body_preferred_return_type_text(func.body);
        }

        self.preferred_expression_type_text(func.body)
            .or_else(|| self.infer_fallback_type_text_at(func.body, 0))
            .filter(|text| !text.is_empty() && text != "any")
    }

    fn source_function_cached_generic_return_type_text(
        &self,
        func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let type_params = func
            .type_parameters
            .as_ref()
            .filter(|type_params| !type_params.nodes.is_empty())?;
        let interner = self.type_interner?;
        let func_type_id = self
            .get_node_type_or_names(&[func_idx, func.name])
            .or_else(|| self.get_type_via_symbol_for_func(func_idx, func.name))?;
        let return_type_id = type_queries::get_return_type(interner, func_type_id)?;
        if matches!(
            return_type_id,
            tsz_solver::types::TypeId::ANY
                | tsz_solver::types::TypeId::UNKNOWN
                | tsz_solver::types::TypeId::ERROR
        ) {
            return None;
        }

        let type_text = self.print_type_id_with_outer_type_params(return_type_id, type_params);
        if type_text.is_empty() || matches!(type_text.as_str(), "any" | "unknown") {
            return None;
        }

        let type_param_names = self.collect_type_param_names(type_params);
        type_param_names
            .iter()
            .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            .then_some(type_text)
    }

    fn single_return_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_node = self.arena.get(block.statements.nodes[0])?;
        let ret = self.arena.get_return_statement(stmt_node)?;
        self.skip_parenthesized_expression(ret.expression)
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

    pub(in crate::declaration_emitter) fn tagged_template_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            return None;
        }

        let tagged = self.arena.get_tagged_template(expr_node)?;
        let sym_id = self.value_reference_symbol(tagged.tag)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            if let Some(signature) = source_arena.get_signature(decl_node)
                && signature.type_annotation.is_some()
                && let Some(type_text) =
                    self.source_slice_from_arena(source_arena.as_ref(), signature.type_annotation)
            {
                let type_text = type_text
                    .trim_end()
                    .trim_end_matches(';')
                    .trim_end()
                    .to_string();
                if signature.parameters.is_some() {
                    return Some(type_text);
                }
                if let Some((_, return_text)) = type_text.rsplit_once("=>") {
                    return Some(return_text.trim().to_string());
                }
            }
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn nameable_new_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let base_text = self.declaration_constructor_expression_text(new_expr.expression)?;
        let base_text = self.rewrite_exported_import_equals_type_text(base_text);
        let type_args = self.type_argument_list_source_text(new_expr.type_arguments.as_ref());
        if type_args.is_empty() {
            if let Some(inferred) =
                self.inherited_generic_class_new_expression_type_text(new_expr, &base_text)
            {
                return Some(inferred);
            }
            if let Some(type_id) = self.get_node_type_or_names(&[expr_idx]) {
                let inferred = self.print_type_id_for_inferred_declaration(type_id);
                if inferred.starts_with(&format!("{base_text}<")) {
                    return Some(inferred);
                }
            }
            if let Some(ident) = self.get_identifier_text(new_expr.expression)
                && let Some(sym_id) = self.resolve_identifier_symbol(new_expr.expression, &ident)
                && let Some(symbol) = self.binder.and_then(|binder| binder.symbols.get(sym_id))
                && symbol.flags & symbol_flags::CLASS != 0
            {
                for &decl_idx in &symbol.declarations {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(class_data) = self.arena.get_class(decl_node) else {
                        continue;
                    };
                    let Some(type_parameters) = class_data.type_parameters.as_ref() else {
                        continue;
                    };
                    if type_parameters.nodes.is_empty() {
                        continue;
                    }
                    let args = type_parameters
                        .nodes
                        .iter()
                        .map(|&param_idx| {
                            self.arena
                                .get(param_idx)
                                .and_then(|param_node| self.arena.get_type_parameter(param_node))
                                .and_then(|param| {
                                    let default_node = self.arena.get(param.default)?;
                                    self.get_source_slice_no_semi(
                                        default_node.pos,
                                        default_node.end,
                                    )
                                })
                                .unwrap_or_else(|| "unknown".to_string())
                        })
                        .collect::<Vec<_>>();
                    return Some(format!("{base_text}<{}>", args.join(", ")));
                }
            }
            Some(base_text)
        } else {
            Some(format!("{base_text}<{}>", type_args.join(", ")))
        }
    }

    fn inherited_generic_class_new_expression_type_text(
        &self,
        new_expr: &tsz_parser::parser::node::CallExprData,
        base_text: &str,
    ) -> Option<String> {
        let args = new_expr.arguments.as_ref()?;
        if args.nodes.is_empty() {
            return None;
        }
        let ident = self.get_identifier_text(new_expr.expression)?;
        let sym_id = self.resolve_identifier_symbol(new_expr.expression, &ident)?;
        let symbol = self.binder.and_then(|binder| binder.symbols.get(sym_id))?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }

        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            let class_data = self.arena.get_class(decl_node)?;
            let type_parameters = class_data.type_parameters.as_ref()?;
            if type_parameters.nodes.is_empty()
                || class_data.members.nodes.iter().copied().any(|member_idx| {
                    self.arena
                        .get(member_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
                })
            {
                continue;
            }
            let own_type_param_names = self.collect_type_param_names(type_parameters);
            let inherited_type_param_names =
                self.inherited_base_type_argument_names(class_data, &own_type_param_names)?;
            let mut inferred_args = Vec::with_capacity(own_type_param_names.len());
            for type_param_name in &own_type_param_names {
                if inherited_type_param_names
                    .first()
                    .is_some_and(|name| name == type_param_name)
                {
                    let first_arg_type = self
                        .preferred_expression_type_text(args.nodes[0])
                        .or_else(|| self.infer_fallback_type_text_at(args.nodes[0], 0))?;
                    inferred_args.push(first_arg_type);
                    continue;
                }
                inferred_args.push(
                    self.class_type_parameter_default_text(type_param_name, type_parameters)
                        .unwrap_or_else(|| "unknown".to_string()),
                );
            }
            if inferred_args
                .iter()
                .any(|arg| arg == "any" || arg.is_empty())
            {
                return None;
            }
            return Some(format!("{base_text}<{}>", inferred_args.join(", ")));
        }

        None
    }

    fn inherited_base_type_argument_names(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        own_type_param_names: &[String],
    ) -> Option<Vec<String>> {
        let heritage = class_data.heritage_clauses.as_ref()?;
        for clause_idx in heritage.nodes.iter().copied() {
            let clause_node = self.arena.get(clause_idx)?;
            let clause = self.arena.get_heritage_clause(clause_node)?;
            for type_idx in clause.types.nodes.iter().copied() {
                let type_node = self.arena.get(type_idx)?;
                let expr_with_type_args = self.arena.get_expr_type_args(type_node)?;
                let type_args = expr_with_type_args.type_arguments.as_ref()?;
                let names = type_args
                    .nodes
                    .iter()
                    .copied()
                    .map(|arg_idx| self.simple_type_argument_source_text(arg_idx))
                    .collect::<Option<Vec<_>>>()?;
                if names
                    .iter()
                    .any(|name| own_type_param_names.iter().any(|own| own == name))
                {
                    return Some(names);
                }
            }
        }
        None
    }

    fn simple_type_argument_source_text(&self, arg_idx: NodeIndex) -> Option<String> {
        if let Some(identifier) = self.get_identifier_text(arg_idx)
            && Self::is_simple_identifier_text(&identifier)
        {
            return Some(identifier);
        }
        let node = self.arena.get(arg_idx)?;
        let mut text = self.get_source_slice_no_semi(node.pos, node.end)?;
        Self::strip_type_argument_overshoot(&mut text);
        let text = text.trim().to_string();
        Self::is_simple_identifier_text(&text).then_some(text)
    }

    fn class_type_parameter_default_text(
        &self,
        type_param_name: &str,
        type_parameters: &NodeList,
    ) -> Option<String> {
        for &param_idx in &type_parameters.nodes {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_type_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(type_param_name) {
                continue;
            }
            let default_node = self.arena.get(param.default)?;
            return self.get_source_slice_no_semi(default_node.pos, default_node.end);
        }
        None
    }
}
