//! Recovery for returned local function initializer signatures.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

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
            if let Some(type_text) =
                self.source_function_initializer_type_text(outer_func, inner_func)
            {
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
                    return self.source_function_initializer_type_text(outer_func, inner_func);
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
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let outer_type_param_names = outer_func
            .type_parameters
            .as_ref()
            .map(|type_params| self.collect_type_param_names(type_params))
            .unwrap_or_default();
        self.source_nested_function_type_text(Some(outer_func), inner_func, &outer_type_param_names)
    }

    pub(in crate::declaration_emitter) fn source_nested_function_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_func: &tsz_parser::parser::node::FunctionData,
        outer_type_param_names: &[String],
    ) -> Option<String> {
        let outer_type_param_names = outer_func
            .and_then(|func| func.type_parameters.as_ref())
            .map(|type_params| self.collect_type_param_names(type_params))
            .unwrap_or_else(|| outer_type_param_names.to_vec());
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

        let params_text = inner_func
            .parameters
            .nodes
            .iter()
            .copied()
            .map(|param_idx| self.source_function_parameter_text(param_idx, &inner_renames))
            .collect::<Option<Vec<_>>>()?
            .join(", ");
        let return_text = self.source_function_initializer_return_type_text(
            outer_func,
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
        self.source_nested_function_type_text(Some(outer_func), inner_func, &[])
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
        (
            direct_function_return
                .or_else(|| self.function_body_preferred_return_type_text(func_body)),
            has_direct_function_return,
        )
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
        self.source_nested_function_type_text(None, inner_func, &outer_type_param_names)
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
        type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let name = self.identifier_text_or_source(param.name)?;
        let raw_type_text = self
            .preferred_annotation_name_text(param.type_annotation)
            .or_else(|| self.emit_type_node_text(param.type_annotation))
            .unwrap_or_else(|| "any".to_string());
        let type_text = Self::simple_type_reference_name(&raw_type_text)
            .and_then(|alias_name| self.local_type_alias_annotation_text(param_idx, &alias_name))
            .unwrap_or_else(|| {
                Self::rename_type_text_identifiers(&raw_type_text, type_param_renames)
            });
        Some(format!("{name}: {type_text}"))
    }

    fn source_function_initializer_return_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
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
        if inner_func.body.is_none() {
            return None;
        }

        let return_text = if self.body_returns_void(inner_func.body) {
            "void".to_string()
        } else {
            let outer_func = outer_func?;
            let return_expr = self
                .const_asserted_expression(inner_func.body)
                .unwrap_or(inner_func.body);
            let return_node = self.arena.get(return_expr)?;
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
        let mut current_idx = from_idx;
        while let Some(parent_idx) = self.arena.parent_of(current_idx) {
            let Some(parent_node) = self.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::BLOCK
                && let Some(block) = self.arena.get_block(parent_node)
            {
                for stmt_idx in block.statements.nodes.iter().copied() {
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
            }
            current_idx = parent_idx;
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
