//! Recovery for returned local function initializer signatures.

use super::super::DeclarationEmitter;
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NullishGuard {
    Null,
    Undefined,
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn async_returned_function_initializer_promise_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let is_async = outer_func.is_async
            || self
                .arena
                .has_modifier(&outer_func.modifiers, SyntaxKind::AsyncKeyword);
        if !is_async {
            return None;
        }

        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        let returned_name = self.identifier_text_or_source(returned_identifier)?;
        let annotation =
            self.local_variable_type_annotation_text_by_name(body_idx, &returned_name)?;
        let target_name = Self::type_query_identifier_name(&annotation)?;
        let type_text =
            self.local_function_initializer_type_text_by_name(outer_func, body_idx, &target_name)?;
        Some(format!("Promise<({type_text})>"))
    }

    pub(in crate::declaration_emitter) fn returned_function_initializer_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(identifier_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_idx = self.variable_declaration_from_symbol_decl(decl_idx)?;
            let decl_node = self.arena.get(decl_idx)?;
            let var_decl = self.arena.get_variable_declaration(decl_node)?;
            let init_node = self.arena.get(var_decl.initializer)?;
            if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
                && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            {
                continue;
            }
            let inner_func = self.arena.get_function(init_node)?;
            if let Some(type_text) = self.source_function_initializer_type_text(
                outer_func,
                var_decl.initializer,
                inner_func,
            ) {
                return Some(type_text);
            }
        }

        None
    }

    fn type_query_identifier_name(type_text: &str) -> Option<String> {
        let start = type_text.find("typeof ")? + "typeof ".len();
        let rest = &type_text[start..];
        let mut end = 0usize;
        for (idx, ch) in rest.char_indices() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        (end > 0).then(|| rest[..end].to_string())
    }

    fn local_variable_type_annotation_text_by_name(
        &self,
        scope_stmt_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let scope_node = self.arena.get(scope_stmt_idx)?;
        if scope_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(scope_node)
        {
            return self.local_variable_type_annotation_text_in_statements(&block.statements, name);
        }
        self.variable_type_annotation_text_from_statement(scope_stmt_idx, name)
    }

    fn local_variable_type_annotation_text_in_statements(
        &self,
        statements: &NodeList,
        name: &str,
    ) -> Option<String> {
        for &stmt_idx in &statements.nodes {
            if let Some(type_text) =
                self.variable_type_annotation_text_from_statement(stmt_idx, name)
            {
                return Some(type_text);
            }
        }
        None
    }

    fn variable_type_annotation_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let stmt = self.arena.get_variable(stmt_node)?;
        for &decl_list_idx in &stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            let decl_list = self.arena.get_variable(decl_list_node)?;
            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;
                if self.identifier_text_or_source(decl.name).as_deref() == Some(name)
                    && decl.type_annotation.is_some()
                {
                    return self.emit_type_node_text(decl.type_annotation);
                }
            }
        }
        None
    }

    fn local_function_initializer_type_text_by_name(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        scope_stmt_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let scope_node = self.arena.get(scope_stmt_idx)?;
        if scope_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(scope_node)
        {
            return self.local_function_initializer_type_text_in_statements(
                outer_func,
                &block.statements,
                name,
            );
        }
        self.function_initializer_type_text_from_statement(outer_func, scope_stmt_idx, name)
    }

    fn local_function_initializer_type_text_in_statements(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        statements: &NodeList,
        name: &str,
    ) -> Option<String> {
        for &stmt_idx in &statements.nodes {
            if let Some(type_text) =
                self.function_initializer_type_text_from_statement(outer_func, stmt_idx, name)
            {
                return Some(type_text);
            }
        }
        None
    }

    fn function_initializer_type_text_from_statement(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let stmt = self.arena.get_variable(stmt_node)?;
        for &decl_list_idx in &stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            let decl_list = self.arena.get_variable(decl_list_node)?;
            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;
                if self.identifier_text_or_source(decl.name).as_deref() == Some(name) {
                    let init_node = self.arena.get(decl.initializer)?;
                    if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
                        && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        return None;
                    }
                    let inner_func = self.arena.get_function(init_node)?;
                    return self.source_function_initializer_type_text(
                        outer_func,
                        decl.initializer,
                        inner_func,
                    );
                }
            }
        }
        None
    }

    fn variable_declaration_from_symbol_decl(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = self.arena.get(current)?;
            if self.arena.get_variable_declaration(node).is_some() {
                return Some(current);
            }
            current = self.arena.parent_of(current)?;
        }
        None
    }

    fn source_function_initializer_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let outer_type_param_names = outer_func
            .type_parameters
            .as_ref()
            .map(|type_params| self.collect_type_param_names(type_params))
            .unwrap_or_default();
        self.source_nested_function_type_text(
            Some(outer_func),
            inner_idx,
            inner_func,
            &outer_type_param_names,
        )
    }

    pub(in crate::declaration_emitter) fn source_nested_function_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
        outer_type_param_names: &[String],
    ) -> Option<String> {
        let outer_type_param_names = if outer_type_param_names.is_empty() {
            outer_func
                .and_then(|func| func.type_parameters.as_ref())
                .map(|type_params| self.collect_type_param_names(type_params))
                .unwrap_or_default()
        } else {
            outer_type_param_names.to_vec()
        };
        let inner_type_params = inner_func.type_parameters.as_ref();
        let inner_renames = inner_type_params.map_or_else(Vec::new, |type_params| {
            self.shadowed_function_type_param_renames(type_params, &outer_type_param_names)
        });

        let type_params_text = inner_type_params
            .filter(|type_params| !type_params.nodes.is_empty())
            .and_then(|type_params| {
                let params = type_params
                    .nodes
                    .iter()
                    .copied()
                    .map(|param_idx| {
                        self.source_function_type_parameter_text(param_idx, &inner_renames)
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("<{}>", params.join(", ")))
            })
            .unwrap_or_default();

        let jsdoc = self.returned_function_expression_jsdoc(inner_idx, inner_func);
        let jsdoc_function_parts = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_type_text)
            .and_then(|type_text| Self::parse_function_type_text(&type_text));
        let mut used_param_names = Vec::new();
        let mut params = Vec::with_capacity(inner_func.parameters.nodes.len());
        for (position, param_idx) in inner_func.parameters.nodes.iter().copied().enumerate() {
            let text = self.source_function_parameter_text(
                param_idx,
                position,
                &inner_renames,
                jsdoc.as_deref(),
                jsdoc_function_parts.as_ref(),
                &mut used_param_names,
            )?;
            params.push(text);
        }
        let params_text = params.join(", ");
        let return_text = self.source_function_initializer_return_type_text(
            outer_func,
            inner_idx,
            inner_func,
            &inner_renames,
        )?;

        Some(format!(
            "{type_params_text}({params_text}) => {return_text}"
        ))
    }

    pub(in crate::declaration_emitter) fn direct_returned_function_expression_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.arena.get(outer_func.body)?;
        let block = self.arena.get_block(body_node)?;
        let mut returned_function = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    continue;
                }
                return None;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                return None;
            }
            let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
            let expr_node = self.arena.get(expr_idx)?;
            if expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                && expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            {
                return None;
            }
            if returned_function.replace(expr_idx).is_some() {
                return None;
            }
        }
        let inner_idx = returned_function?;
        let inner_node = self.arena.get(inner_idx)?;
        let inner_func = self.arena.get_function(inner_node)?;
        self.source_nested_function_type_text(Some(outer_func), inner_idx, inner_func, &[])
    }

    pub(in crate::declaration_emitter) fn function_body_return_hint(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> (Option<String>, bool) {
        let direct_function_return = func_body
            .is_some()
            .then(|| self.direct_returned_function_expression_type_text(func))
            .flatten();
        let has_direct_function_return = direct_function_return.is_some();
        let return_text = direct_function_return
            .or_else(|| {
                self.function_body_returned_local_function_object_type_text(func, func_body)
            })
            .or_else(|| self.function_body_guarded_parameter_return_text(func, func_body))
            .or_else(|| self.function_body_composed_nullish_guard_return_text(func, func_body))
            .or_else(|| self.function_body_nonnullable_short_circuit_return_text(func, func_body))
            .or_else(|| self.function_body_preferred_return_type_text(func_body))
            .map(|type_text| {
                self.expand_rest_tuple_parameters_in_function_type_text(func_body, &type_text)
                    .unwrap_or(type_text)
            });
        (return_text, has_direct_function_return)
    }

    fn function_body_returned_local_function_object_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> Option<String> {
        let return_expr = self.function_body_unique_return_expression(func_body)?;
        self.returned_local_function_object_type_text(func, return_expr, &[], 0)
    }

    fn returned_local_function_object_type_text(
        &self,
        owner_func: &tsz_parser::parser::node::FunctionData,
        object_expr_idx: NodeIndex,
        owner_type_param_renames: &[(String, String)],
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        if object.elements.nodes.is_empty() {
            return None;
        }

        let owner_type_param_names = owner_func
            .type_parameters
            .as_ref()
            .map(|type_params| {
                let mut names = Vec::new();
                for name in self.collect_type_param_names(type_params) {
                    names.push(name.clone());
                    if let Some(renamed) = owner_type_param_renames
                        .iter()
                        .find_map(|(from, to)| (from == &name).then(|| to.clone()))
                        && !names.contains(&renamed)
                    {
                        names.push(renamed);
                    }
                }
                names
            })
            .unwrap_or_default();

        let mut members = Vec::new();
        for member_idx in object.elements.nodes.iter().copied() {
            let member_node = self.arena.get(member_idx)?;
            let shorthand = self.arena.get_shorthand_property(member_node)?;
            if shorthand.object_assignment_initializer.is_some() {
                return None;
            }
            let name = self.get_identifier_text(shorthand.name)?;
            let (func_idx, func) = self.local_function_declaration(owner_func, &name)?;
            let type_text = self.source_nested_function_type_text(
                Some(owner_func),
                func_idx,
                func,
                &owner_type_param_names,
            )?;
            let type_text =
                Self::rename_type_text_identifiers(&type_text, owner_type_param_renames);
            members.push(Self::format_object_member_type_text(
                &name, &type_text, depth,
            ));
        }

        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let formatted_members = members
            .iter()
            .map(|member| Self::format_object_member_entry(&member_indent, member))
            .collect::<Vec<_>>();
        Some(format!(
            "{{\n{}\n{closing_indent}}}",
            formatted_members.join("\n")
        ))
    }

    fn local_function_declaration(
        &self,
        owner_func: &tsz_parser::parser::node::FunctionData,
        name: &str,
    ) -> Option<(NodeIndex, &tsz_parser::parser::node::FunctionData)> {
        let body_node = self.arena.get(owner_func.body)?;
        let block = self.arena.get_block(body_node)?;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let func = self.arena.get_function(stmt_node)?;
            if self.get_identifier_text(func.name).as_deref() == Some(name) {
                return Some((stmt_idx, func));
            }
        }
        None
    }

    fn function_body_composed_nullish_guard_return_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(func_body)?;
        let block = self.arena.get_block(body_node)?;
        let [return_idx] = block.statements.nodes.as_slice() else {
            return None;
        };
        let return_node = self.arena.get(*return_idx)?;
        let ret = self.arena.get_return_statement(return_node)?;
        let outer_call_node = self.arena.get(ret.expression)?;
        let outer_call = self.arena.get_call_expr(outer_call_node)?;
        let outer_excluded = self.callee_nullish_guard_kind(outer_call.expression)?;
        let [inner_expr] = outer_call.arguments.as_ref()?.nodes.as_slice() else {
            return None;
        };

        let inner_call_node = self.arena.get(*inner_expr)?;
        let inner_call = self.arena.get_call_expr(inner_call_node)?;
        let inner_excluded = self.callee_nullish_guard_kind(inner_call.expression)?;
        if outer_excluded == inner_excluded {
            return None;
        }
        let [arg_idx] = inner_call.arguments.as_ref()?.nodes.as_slice() else {
            return None;
        };
        let param_name = self.get_identifier_text(*arg_idx)?;

        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(param_name.as_str()) {
                continue;
            }
            let param_text = self
                .emit_type_node_text(param.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))?;
            return Some(format!("{param_text} & {{}}"));
        }

        None
    }

    fn callee_nullish_guard_kind(&self, callee_idx: NodeIndex) -> Option<NullishGuard> {
        let sym_id = self.value_reference_symbol(callee_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.arena.get_function(decl_node) else {
                continue;
            };
            let Some((_, excluded, _)) = self.guarded_parameter_return_info(func, func.body) else {
                continue;
            };
            return Some(excluded);
        }
        None
    }

    fn function_body_guarded_parameter_return_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> Option<String> {
        let (_, excluded, param_text) = self.guarded_parameter_return_info(func, func_body)?;
        match excluded {
            NullishGuard::Null => Some(format!("{param_text} & ({{}} | undefined)")),
            NullishGuard::Undefined => Some(format!("{param_text} & ({{}} | null)")),
        }
    }

    fn guarded_parameter_return_info(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> Option<(String, NullishGuard, String)> {
        let body_node = self.arena.get(func_body)?;
        let block = self.arena.get_block(body_node)?;
        let [guard_idx, return_idx] = block.statements.nodes.as_slice() else {
            return None;
        };

        let guard_node = self.arena.get(*guard_idx)?;
        let guard = self.arena.get_if_statement(guard_node)?;
        if guard.else_statement.is_some() || !self.statement_always_throws(guard.then_statement) {
            return None;
        }

        let (param_name, excluded) = self.nullish_equality_guard_parameter(guard.expression)?;
        let return_node = self.arena.get(*return_idx)?;
        let ret = self.arena.get_return_statement(return_node)?;
        if self.get_identifier_text(ret.expression).as_deref() != Some(param_name.as_str()) {
            return None;
        }

        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(param_name.as_str()) {
                continue;
            }
            let param_text = self
                .emit_type_node_text(param.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))?;
            return Some((param_name, excluded, param_text));
        }

        None
    }

    fn statement_always_throws(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind == syntax_kind_ext::THROW_STATEMENT {
            return true;
        }
        if stmt_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(stmt_node)
        {
            return block.statements.nodes.iter().copied().any(|stmt_idx| {
                self.arena
                    .get(stmt_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::THROW_STATEMENT)
            });
        }
        false
    }

    fn nullish_equality_guard_parameter(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, NullishGuard)> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsEqualsEqualsToken as u16 {
            return None;
        }
        self.nullish_equality_side(binary.left, binary.right)
            .or_else(|| self.nullish_equality_side(binary.right, binary.left))
    }

    fn nullish_equality_side(
        &self,
        name_idx: NodeIndex,
        nullish_idx: NodeIndex,
    ) -> Option<(String, NullishGuard)> {
        let nullish_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(nullish_idx);
        let nullish_node = self.arena.get(nullish_idx)?;
        let excluded = if nullish_node.kind == SyntaxKind::NullKeyword as u16 {
            NullishGuard::Null
        } else if nullish_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.get_identifier_text(nullish_idx).as_deref() == Some("undefined")
        {
            NullishGuard::Undefined
        } else {
            return None;
        };
        let name_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(name_idx);
        let name = self.get_identifier_text(name_idx)?;
        Some((name, excluded))
    }

    fn function_body_nonnullable_short_circuit_return_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        func_body: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(func_body)?;
        let block = self.arena.get_block(body_node)?;
        let [return_idx] = block.statements.nodes.as_slice() else {
            return None;
        };
        let return_node = self.arena.get(*return_idx)?;
        let ret = self.arena.get_return_statement(return_node)?;
        let expr_idx = self.skip_parenthesized_expression(ret.expression)?;
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16
            && binary.operator_token != SyntaxKind::QuestionQuestionToken as u16
        {
            return None;
        }
        if !self.expression_type_is_never_for_decl_emit(binary.right) {
            return None;
        }
        let left_name = self.get_identifier_text(binary.left)?;
        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(left_name.as_str()) {
                continue;
            }
            let param_text = self
                .emit_type_node_text(param.type_annotation)
                .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))?;
            let non_undefined = Self::remove_undefined_from_union_text(param_text.trim())?;
            if Self::is_simple_identifier_text(&non_undefined) {
                return Some(format!("NonNullable<{non_undefined}>"));
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn class_property_function_initializer_type_text(
        &self,
        prop_idx: NodeIndex,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && init_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return None;
        }
        let inner_func = self.arena.get_function(init_node)?;
        let outer_type_param_names = self.enclosing_class_type_param_names(prop_idx);
        self.source_nested_function_type_text(
            None,
            initializer,
            inner_func,
            &outer_type_param_names,
        )
    }

    fn enclosing_class_type_param_names(&self, from_idx: NodeIndex) -> Vec<String> {
        let mut current = from_idx;
        while let Some(parent_idx) = self.arena.parent_of(current) {
            let Some(parent_node) = self.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return self
                    .arena
                    .get_class(parent_node)
                    .and_then(|class| class.type_parameters.as_ref())
                    .map(|type_params| self.collect_type_param_names(type_params))
                    .unwrap_or_default();
            }
            current = parent_idx;
        }
        Vec::new()
    }

    fn shadowed_function_type_param_renames(
        &self,
        type_params: &NodeList,
        outer_names: &[String],
    ) -> Vec<(String, String)> {
        let mut names_in_scope = outer_names.to_vec();
        let mut renames = Vec::new();
        for param_idx in type_params.nodes.iter().copied() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name) = self.get_identifier_text(param.name) else {
                continue;
            };
            if names_in_scope.contains(&name) {
                let mut suffix = 1u32;
                loop {
                    let candidate = format!("{name}_{suffix}");
                    if !names_in_scope.contains(&candidate) {
                        renames.push((name.clone(), candidate.clone()));
                        names_in_scope.push(candidate);
                        break;
                    }
                    suffix += 1;
                }
            } else {
                names_in_scope.push(name);
            }
        }
        renames
    }

    fn source_function_type_parameter_text(
        &self,
        param_idx: NodeIndex,
        type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_type_parameter(param_node)?;
        let name = self.identifier_text_or_source(param.name)?;
        let mut text = String::new();

        if let Some(ref modifiers) = param.modifiers {
            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.arena.get(modifier_idx) else {
                    continue;
                };
                match modifier_node.kind {
                    k if k == SyntaxKind::InKeyword as u16 => text.push_str("in "),
                    k if k == SyntaxKind::OutKeyword as u16 => text.push_str("out "),
                    k if k == SyntaxKind::ConstKeyword as u16 => text.push_str("const "),
                    _ => {}
                }
            }
        }

        text.push_str(&Self::renamed_type_param_name(&name, type_param_renames));

        if param.constraint.is_some() {
            let constraint_text = self
                .preferred_annotation_name_text(param.constraint)
                .or_else(|| self.emit_type_node_text(param.constraint))?;
            text.push_str(" extends ");
            text.push_str(&Self::rename_type_text_identifiers(
                &constraint_text,
                type_param_renames,
            ));
        }

        if param.default.is_some() {
            let default_text = self
                .preferred_annotation_name_text(param.default)
                .or_else(|| self.emit_type_node_text(param.default))?;
            text.push_str(" = ");
            text.push_str(&Self::rename_type_text_identifiers(
                &default_text,
                type_param_renames,
            ));
        }

        Some(text)
    }

    fn source_function_parameter_text(
        &self,
        param_idx: NodeIndex,
        position: usize,
        type_param_renames: &[(String, String)],
        function_jsdoc: Option<&str>,
        jsdoc_function_parts: Option<&super::type_inference_function_text::FunctionTypeTextParts>,
        used_param_names: &mut Vec<String>,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let name = self.identifier_text_or_source(param.name)?;
        let raw_type_text = self
            .preferred_annotation_name_text(param.type_annotation)
            .or_else(|| self.emit_type_node_text(param.type_annotation))
            .or_else(|| {
                self.source_is_js_file
                    .then(|| {
                        self.jsdoc_returned_function_parameter_type_text(
                            param_idx,
                            position,
                            function_jsdoc,
                            jsdoc_function_parts,
                        )
                    })
                    .flatten()
            })
            .unwrap_or_else(|| "any".to_string());
        let type_text = Self::simple_type_reference_name(&raw_type_text)
            .and_then(|alias_name| self.local_type_alias_annotation_text(param_idx, &alias_name))
            .unwrap_or_else(|| {
                Self::rename_type_text_identifiers(&raw_type_text, type_param_renames)
            });
        if param.dot_dot_dot_token
            && let Some(params) =
                self.expand_rest_tuple_parameter_text(param_idx, &type_text, used_param_names)
        {
            return Some(params);
        }
        if param.dot_dot_dot_token {
            used_param_names.push(name.clone());
            return Some(format!("...{name}: {type_text}"));
        }
        used_param_names.push(name.clone());
        Some(format!("{name}: {type_text}"))
    }

    fn jsdoc_returned_function_parameter_type_text(
        &self,
        param_idx: NodeIndex,
        position: usize,
        function_jsdoc: Option<&str>,
        jsdoc_function_parts: Option<&super::type_inference_function_text::FunctionTypeTextParts>,
    ) -> Option<String> {
        if let Some(part) = jsdoc_function_parts.and_then(|parts| parts.parameters.get(position)) {
            return Some(part.type_text.clone());
        }

        let params = function_jsdoc.map(Self::parse_jsdoc_param_decls)?;
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(found.type_text.clone());
        }

        params.into_iter().nth(position).map(|decl| decl.type_text)
    }

    fn expand_rest_tuple_parameter_text(
        &self,
        from_idx: NodeIndex,
        type_text: &str,
        used_param_names: &mut Vec<String>,
    ) -> Option<String> {
        let elements = self.expand_tuple_type_elements(from_idx, type_text, 0)?;

        Some(
            elements
                .into_iter()
                .map(|(name, ty, optional)| {
                    let unique = Self::unique_parameter_name(&name, used_param_names);
                    if optional {
                        let ty = if Self::contains_whole_word_in_text(&ty, "undefined") {
                            ty
                        } else {
                            format!("{ty} | undefined")
                        };
                        return format!("{unique}?: {ty}");
                    }
                    format!("{unique}: {ty}")
                })
                .collect::<Vec<_>>()
                .join(", "),
        )
    }

    pub(in crate::declaration_emitter) fn expand_rest_tuple_parameters_in_function_type_text(
        &self,
        scope_idx: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        let arrow_idx = Self::find_top_level_arrow(trimmed)?;
        let head = trimmed.get(..arrow_idx)?.trim_end();
        let return_text = trimmed.get(arrow_idx + 2..)?.trim();
        let open_idx = head.rfind('(')?;
        let prefix = head.get(..open_idx)?;
        let params_text = head.get(open_idx + 1..)?.strip_suffix(')')?;

        let mut changed = false;
        let mut used_param_names = Vec::new();
        let params = Self::split_top_level_commas(params_text)
            .into_iter()
            .map(|param_text| {
                let param_text = param_text.trim();
                let Some(rest_text) = param_text.strip_prefix("...").map(str::trim) else {
                    Self::track_existing_parameter_name(param_text, &mut used_param_names);
                    return Some(param_text.to_string());
                };
                let colon_idx = Self::find_top_level_byte(rest_text, b':')?;
                let type_text = rest_text.get(colon_idx + 1..)?.trim();
                let expanded = self.expand_rest_tuple_parameter_text(
                    scope_idx,
                    type_text,
                    &mut used_param_names,
                )?;
                changed = true;
                Some(expanded)
            })
            .collect::<Option<Vec<_>>>()?;
        changed.then(|| format!("{prefix}({}) => {return_text}", params.join(", ")))
    }

    fn expand_tuple_type_elements(
        &self,
        from_idx: NodeIndex,
        type_text: &str,
        depth: usize,
    ) -> Option<Vec<(String, String, bool)>> {
        if depth > 8 {
            return None;
        }
        let inner = type_text
            .trim()
            .trim_end_matches(';')
            .trim()
            .strip_prefix('[')?
            .strip_suffix(']')?
            .trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }

        let mut elements = Vec::new();
        for part in Self::split_top_level_commas(inner) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(alias_name) = part.strip_prefix("...").map(str::trim) {
                let alias_text = self.local_type_alias_annotation_text(from_idx, alias_name)?;
                elements.extend(self.expand_tuple_type_elements(
                    from_idx,
                    &alias_text,
                    depth + 1,
                )?);
                continue;
            }
            if let Some((name, ty)) = part.split_once(':') {
                let name = name.trim().trim_start_matches("...");
                let optional = name.ends_with('?');
                let name = name.strip_suffix('?').unwrap_or(name).trim();
                let ty = ty.trim();
                if name.is_empty() || ty.is_empty() {
                    return None;
                }
                elements.push((name.to_string(), ty.to_string(), optional));
                continue;
            }

            // Unlabeled tuple elements are valid TypeScript (e.g. `[string, number]`).
            // Synthesize stable parameter names so tuple rest expansion still works.
            let optional = part.ends_with('?');
            let ty = part.strip_suffix('?').unwrap_or(part).trim();
            if ty.is_empty() {
                return None;
            }
            let synthesized = format!("arg{}", elements.len());
            elements.push((synthesized, ty.to_string(), optional));
        }
        Some(elements)
    }

    fn unique_parameter_name(name: &str, seen: &mut Vec<String>) -> String {
        if !seen.iter().any(|existing| existing == name) {
            seen.push(name.to_string());
            return name.to_string();
        }

        let mut suffix = 1usize;
        loop {
            let candidate = format!("{name}_{suffix}");
            if !seen.iter().any(|existing| existing == &candidate) {
                seen.push(candidate.clone());
                return candidate;
            }
            suffix += 1;
        }
    }

    fn track_existing_parameter_name(param_text: &str, seen: &mut Vec<String>) {
        let Some(colon_idx) = Self::find_top_level_byte(param_text, b':') else {
            return;
        };
        let raw_name = param_text.get(..colon_idx).unwrap_or_default().trim();
        let raw_name = raw_name.strip_prefix("...").unwrap_or(raw_name).trim();
        let raw_name = raw_name.strip_suffix('?').unwrap_or(raw_name).trim();
        if !raw_name.is_empty() {
            seen.push(raw_name.to_string());
        }
    }

    fn source_function_initializer_return_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        if inner_func.type_annotation.is_some() {
            let type_text = self
                .preferred_annotation_name_text(inner_func.type_annotation)
                .or_else(|| self.emit_type_node_text(inner_func.type_annotation))?;
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if self.source_is_js_file
            && let Some(type_text) =
                self.jsdoc_returned_function_return_type_text(inner_idx, inner_func)
        {
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if inner_func.body.is_none() {
            return None;
        }

        let return_text = if self.body_returns_void(inner_func.body) {
            "void".to_string()
        } else {
            let outer_func = outer_func?;
            let return_expr = self
                .function_body_unique_return_expression(inner_func.body)
                .filter(|idx| idx.is_some())
                .unwrap_or(inner_func.body);
            let return_expr = self
                .const_asserted_expression(return_expr)
                .unwrap_or(return_expr);
            let return_node = self.arena.get(return_expr)?;
            if let Some(type_text) = self.returned_call_expression_type_text_from_outer_parameter(
                outer_func,
                return_expr,
                inner_type_param_renames,
            ) {
                type_text
            } else if let Some(type_text) = self.returned_local_function_object_type_text(
                inner_func,
                return_expr,
                inner_type_param_renames,
                1,
            ) {
                type_text
            } else if return_node.kind == SyntaxKind::Identifier as u16 {
                self.function_scope_identifier_type_text(
                    outer_func,
                    inner_func,
                    return_expr,
                    inner_type_param_renames,
                )?
            } else {
                if return_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    return None;
                }
                let array = self.arena.get_literal_expr(return_node)?;
                let elements = array
                    .elements
                    .nodes
                    .iter()
                    .copied()
                    .map(|elem_idx| {
                        self.function_scope_identifier_type_text(
                            outer_func,
                            inner_func,
                            elem_idx,
                            inner_type_param_renames,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                format!("readonly [{}]", elements.join(", "))
            }
        };

        if inner_func.is_async
            || self
                .arena
                .has_modifier(&inner_func.modifiers, SyntaxKind::AsyncKeyword)
        {
            Some(format!("Promise<{return_text}>"))
        } else {
            Some(return_text)
        }
    }

    fn returned_call_expression_type_text_from_outer_parameter(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        return_expr: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let return_node = self.arena.get(return_expr)?;
        if return_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(return_node)?;
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(member_type) =
                self.property_access_declared_type_annotation_text(callee_idx)
                && let Some(parts) = Self::parse_function_type_text(&member_type)
            {
                if parts.return_type.trim() == "unknown" {
                    return self.outer_parameter_property_function_return_type_text(
                        outer_func,
                        callee_idx,
                        inner_type_param_renames,
                    );
                }
                return Some(Self::rename_type_text_identifiers(
                    &parts.return_type,
                    inner_type_param_renames,
                ));
            }
            return self.outer_parameter_property_function_return_type_text(
                outer_func,
                callee_idx,
                inner_type_param_renames,
            );
        }
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let parameter_type = self.function_parameter_type_text(outer_func, callee_idx)?;
        let parts = Self::parse_function_type_text(&parameter_type)?;
        Some(Self::rename_type_text_identifiers(
            &parts.return_type,
            inner_type_param_renames,
        ))
    }

    fn outer_parameter_property_function_return_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        callee_idx: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let access_node = self.arena.get(callee_idx)?;
        let access = self.arena.get_access_expr(access_node)?;
        let expression_idx = self.skip_parenthesized_expression(access.expression)?;
        let property_name =
            self.property_name_text_from_arena(self.arena, access.name_or_argument)?;
        let parameter_type = self.function_parameter_type_text(outer_func, expression_idx)?;
        let (target_name, target_args) = Self::parse_type_reference_text(&parameter_type)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|idx| self.arena.get(idx))
            .and_then(|node| self.arena.get_source_file(node))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(interface.name).as_deref() != Some(&target_name) {
                continue;
            }

            let type_param_names = interface
                .type_parameters
                .as_ref()
                .map(|type_params| self.collect_type_param_names(type_params))
                .unwrap_or_default();
            if type_param_names.len() != target_args.len() {
                continue;
            }
            let substitutions: Vec<(String, String)> = type_param_names
                .into_iter()
                .zip(target_args.iter().cloned())
                .collect();

            for &member_idx in &interface.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(signature) = self.arena.get_signature(member_node) else {
                    continue;
                };
                if self
                    .property_name_text_from_arena(self.arena, signature.name)
                    .as_deref()
                    != Some(&property_name)
                {
                    continue;
                }
                let member_type = self.emit_type_node_text(signature.type_annotation)?;
                let parts = Self::parse_function_type_text(&member_type)?;
                let substituted =
                    Self::rename_type_text_identifiers(&parts.return_type, &substitutions);
                return Some(Self::rename_type_text_identifiers(
                    &substituted,
                    inner_type_param_renames,
                ));
            }
        }

        None
    }

    fn parse_type_reference_text(type_text: &str) -> Option<(String, Vec<String>)> {
        let trimmed = type_text.trim();
        let Some(lt_idx) = trimmed.find('<') else {
            return Self::is_simple_type_reference_name(trimmed)
                .then(|| (trimmed.to_string(), Vec::new()));
        };
        let name = trimmed.get(..lt_idx)?.trim();
        if !Self::is_simple_type_reference_name(name) || !trimmed.ends_with('>') {
            return None;
        }
        let args_text = trimmed.get(lt_idx + 1..trimmed.len() - 1)?;
        let args = Self::split_top_level_commas(args_text)
            .into_iter()
            .map(str::trim)
            .filter(|arg| !arg.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        Some((name.to_string(), args))
    }

    fn is_simple_type_reference_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if first != '_' && first != '$' && !first.is_ascii_alphabetic() {
            return false;
        }
        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn function_body_unique_return_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(body_idx);
        }
        let block = self.arena.get_block(body_node)?;
        let mut result = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() || result.replace(ret.expression).is_some() {
                return None;
            }
        }
        result
    }

    fn jsdoc_returned_function_return_type_text(
        &self,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let jsdoc = self.returned_function_expression_jsdoc(inner_idx, inner_func)?;
        if let Some(type_text) = Self::parse_jsdoc_type_text(&jsdoc)
            && let Some(parts) = Self::parse_function_type_text(&type_text)
        {
            return Some(parts.return_type);
        }
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    fn returned_function_expression_jsdoc(
        &self,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        if let Some(jsdoc) = self.function_like_jsdoc_for_node(inner_idx) {
            return Some(jsdoc);
        }
        if inner_func.body.is_some()
            && let Some(jsdoc) = self.function_like_jsdoc_for_node(inner_func.body)
        {
            return Some(jsdoc);
        }
        if let Some(return_idx) = self.return_statement_ancestor(inner_idx)
            && let Some(return_node) = self.arena.get(return_idx)
            && let Some(jsdoc) = self.leading_jsdoc_comment_for_pos(return_node.pos)
        {
            return Some(jsdoc);
        }
        inner_func
            .parameters
            .nodes
            .first()
            .and_then(|param_idx| self.function_like_jsdoc_for_node(*param_idx))
    }

    fn return_statement_ancestor(&self, from_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = from_idx;
        for _ in 0..8 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn leading_jsdoc_comment_for_pos(&self, pos: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        self.all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .rev()
            .find_map(|comment| {
                let between = text.get(comment.end as usize..actual_start)?;
                if !between
                    .bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                {
                    return None;
                }
                is_jsdoc_comment(comment, text).then(|| get_jsdoc_content(comment, text))
            })
    }

    fn const_asserted_expression(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION
            && expr_node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        if self
            .arena
            .get(assertion.type_node)
            .is_some_and(|node| node.kind == SyntaxKind::ConstKeyword as u16)
        {
            return Some(assertion.expression);
        }
        let type_name = self
            .get_identifier_text(assertion.type_node)
            .or_else(|| self.emit_type_node_text(assertion.type_node))?;
        (type_name == "const").then_some(assertion.expression)
    }

    fn function_scope_identifier_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        inner_func: &tsz_parser::parser::node::FunctionData,
        expr_idx: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let name = self.get_identifier_text(expr_idx)?;
        if let Some(type_text) = self.function_parameter_annotation_text(inner_func, &name) {
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if let Some(type_text) = self.function_parameter_annotation_text(outer_func, &name) {
            return Some(type_text);
        }
        self.get_node_type_or_names(&[expr_idx])
            .map(|type_id| self.print_type_id(type_id))
    }

    fn function_parameter_annotation_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        name: &str,
    ) -> Option<String> {
        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.identifier_text_or_source(param.name).as_deref() != Some(name) {
                continue;
            }
            return self
                .preferred_annotation_name_text(param.type_annotation)
                .or_else(|| self.emit_type_node_text(param.type_annotation));
        }
        None
    }

    fn local_type_alias_annotation_text(&self, from_idx: NodeIndex, name: &str) -> Option<String> {
        if let Some(from_node) = self.arena.get(from_idx)
            && from_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(from_node)
            && let Some(type_text) =
                self.local_type_alias_annotation_text_in_statements(&block.statements, name)
        {
            return Some(type_text);
        }

        let mut current_idx = from_idx;
        while let Some(parent_idx) = self.arena.parent_of(current_idx) {
            let Some(parent_node) = self.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::BLOCK
                && let Some(block) = self.arena.get_block(parent_node)
                && let Some(type_text) =
                    self.local_type_alias_annotation_text_in_statements(&block.statements, name)
            {
                return Some(type_text);
            }
            current_idx = parent_idx;
        }
        None
    }

    fn local_type_alias_annotation_text_in_statements(
        &self,
        statements: &NodeList,
        name: &str,
    ) -> Option<String> {
        for stmt_idx in statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(alias) = self.arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(alias.name).as_deref() == Some(name) {
                return self
                    .local_type_annotation_text(alias.type_node)
                    .or_else(|| {
                        self.preferred_annotation_name_text(alias.type_node)
                            .or_else(|| self.emit_type_node_text(alias.type_node))
                    });
            }
        }
        None
    }

    fn renamed_type_param_name(name: &str, renames: &[(String, String)]) -> String {
        renames
            .iter()
            .find_map(|(from, to)| (from == name).then(|| to.clone()))
            .unwrap_or_else(|| name.to_string())
    }

    fn identifier_text_or_source(&self, idx: NodeIndex) -> Option<String> {
        self.get_identifier_text(idx).or_else(|| {
            let node = self.arena.get(idx)?;
            (node.kind == SyntaxKind::Identifier as u16)
                .then(|| self.get_source_slice_no_semi(node.pos, node.end))?
        })
    }

    fn rename_type_text_identifiers(text: &str, renames: &[(String, String)]) -> String {
        if renames.is_empty() {
            return text.to_string();
        }

        let mut result = String::with_capacity(text.len());
        let mut ident_start = None;
        for (idx, ch) in text.char_indices() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                ident_start.get_or_insert(idx);
                continue;
            }
            if let Some(start) = ident_start.take() {
                let ident = &text[start..idx];
                result.push_str(&Self::renamed_type_param_name(ident, renames));
            }
            result.push(ch);
        }
        if let Some(start) = ident_start {
            let ident = &text[start..];
            result.push_str(&Self::renamed_type_param_name(ident, renames));
        }
        result
    }
}
