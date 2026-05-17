//! Recovery for returned local function initializer signatures.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

struct SourceFunctionSignatureText {
    type_params_text: String,
    params_text: String,
    return_text: String,
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

    pub(in crate::declaration_emitter) fn local_function_declaration_identifier_type_text(
        &self,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        let outer_func = self.enclosing_function_data(identifier_idx)?;
        let outer_type_param_names = outer_func
            .type_parameters
            .as_ref()
            .map(|type_params| self.collect_type_param_names(type_params))
            .unwrap_or_default();
        self.function_declaration_identifier_type_text(
            identifier_idx,
            Some(outer_func),
            &outer_type_param_names,
            &[],
        )
    }

    pub(in crate::declaration_emitter) fn function_declaration_identifier_type_text(
        &self,
        identifier_idx: NodeIndex,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        outer_type_param_names: &[String],
        type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let identifier_name = self.identifier_text_or_source(identifier_idx)?;
        if let Some(func_decls) = outer_func
            .and_then(|func| self.local_function_declarations_in_body(func.body, &identifier_name))
        {
            let type_text = self.function_declaration_type_text_from_declarations(
                &func_decls,
                outer_func,
                outer_type_param_names,
            )?;
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                type_param_renames,
            ));
        }

        let sym_id = self.value_reference_symbol(identifier_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        let func_decls = symbol
            .declarations
            .iter()
            .copied()
            .filter_map(|decl_idx| self.function_declaration_from_symbol_decl(decl_idx))
            .collect::<Vec<_>>();
        if func_decls.is_empty() {
            return None;
        }

        let type_text = self.function_declaration_type_text_from_declarations(
            &func_decls,
            outer_func,
            outer_type_param_names,
        )?;
        Some(Self::rename_type_text_identifiers(
            &type_text,
            type_param_renames,
        ))
    }

    fn function_declaration_type_text_from_declarations(
        &self,
        func_decls: &[NodeIndex],
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        outer_type_param_names: &[String],
    ) -> Option<String> {
        let overload_decls = func_decls
            .iter()
            .copied()
            .filter(|&func_idx| {
                self.arena
                    .get(func_idx)
                    .and_then(|node| self.arena.get_function(node))
                    .is_some_and(|func| func.body.is_none())
            })
            .collect::<Vec<_>>();
        if !overload_decls.is_empty() {
            return self.source_nested_function_overload_set_type_text(
                outer_func,
                &overload_decls,
                outer_type_param_names,
            );
        }

        let func_idx = func_decls
            .iter()
            .copied()
            .find(|&func_idx| {
                self.arena
                    .get(func_idx)
                    .and_then(|node| self.arena.get_function(node))
                    .is_some_and(|func| func.body.is_some())
            })
            .or_else(|| func_decls.first().copied())?;
        let func_node = self.arena.get(func_idx)?;
        let func = self.arena.get_function(func_node)?;
        self.source_nested_function_type_text(outer_func, func_idx, func, outer_type_param_names)
    }

    fn local_function_declarations_in_body(
        &self,
        body_idx: NodeIndex,
        name: &str,
    ) -> Option<Vec<NodeIndex>> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut declarations = Vec::new();
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let func = self.arena.get_function(stmt_node)?;
            if self.identifier_text_or_source(func.name).as_deref() == Some(name) {
                declarations.push(stmt_idx);
            }
        }
        (!declarations.is_empty()).then_some(declarations)
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

    fn function_declaration_from_symbol_decl(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && self.arena.get_function(node).is_some()
            {
                return Some(current);
            }
            current = self.arena.parent_of(current)?;
        }
        None
    }

    fn enclosing_function_data(
        &self,
        from_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let mut current = from_idx;
        while let Some(parent_idx) = self.arena.parent_of(current) {
            let node = self.arena.get(parent_idx)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                return self.arena.get_function(node);
            }
            current = parent_idx;
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
        let signature = self.source_nested_function_signature_text(
            outer_func,
            inner_idx,
            inner_func,
            outer_type_param_names,
        )?;
        Some(format!(
            "{}({}) => {}",
            signature.type_params_text, signature.params_text, signature.return_text
        ))
    }

    fn source_nested_function_call_signature_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
        outer_type_param_names: &[String],
    ) -> Option<String> {
        let signature = self.source_nested_function_signature_text(
            outer_func,
            inner_idx,
            inner_func,
            outer_type_param_names,
        )?;
        Some(format!(
            "{}({}): {}",
            signature.type_params_text, signature.params_text, signature.return_text
        ))
    }

    fn source_nested_function_overload_set_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        overload_decls: &[NodeIndex],
        outer_type_param_names: &[String],
    ) -> Option<String> {
        let signatures = overload_decls
            .iter()
            .copied()
            .map(|func_idx| {
                let func_node = self.arena.get(func_idx)?;
                let func = self.arena.get_function(func_node)?;
                self.source_nested_function_call_signature_text(
                    outer_func,
                    func_idx,
                    func,
                    outer_type_param_names,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        if signatures.is_empty() {
            return None;
        }

        let mut text = String::from("{");
        for signature in signatures {
            text.push_str("\n        ");
            text.push_str(&signature);
            text.push(';');
        }
        text.push_str("\n    }");
        Some(text)
    }

    fn source_nested_function_signature_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
        outer_type_param_names: &[String],
    ) -> Option<SourceFunctionSignatureText> {
        let mut outer_type_param_names = outer_type_param_names.to_vec();
        if let Some(type_params) = outer_func.and_then(|func| func.type_parameters.as_ref()) {
            for name in self.collect_type_param_names(type_params) {
                if !outer_type_param_names.contains(&name) {
                    outer_type_param_names.push(name);
                }
            }
        }
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

        Some(SourceFunctionSignatureText {
            type_params_text,
            params_text,
            return_text,
        })
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
            .or_else(|| self.function_body_preferred_return_type_text(func_body))
            .map(|type_text| {
                self.expand_rest_tuple_parameters_in_function_type_text(func_body, &type_text)
                    .unwrap_or(type_text)
            });
        (return_text, has_direct_function_return)
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

    pub(in crate::declaration_emitter) fn preserve_call_argument_single_rest_parameter_text(
        &self,
        call_idx: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let call_node = self.arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(call_node)?;
        let args = call.arguments.as_ref()?;
        let first_arg = args.nodes.first().copied()?;
        let first_arg = self.skip_parenthesized_expression(first_arg)?;
        let arg_node = self.arena.get(first_arg)?;
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(arg_node)?;
        let [param_idx] = func.parameters.nodes.as_slice() else {
            return None;
        };
        let param_node = self.arena.get(*param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if !param.dot_dot_dot_token {
            return None;
        }

        let trimmed = type_text.trim();
        let arrow_idx = Self::find_top_level_arrow(trimmed)?;
        let head = trimmed.get(..arrow_idx)?.trim_end();
        let return_text = trimmed.get(arrow_idx + 2..)?.trim();
        let open_idx = head.rfind('(')?;
        let prefix = head.get(..open_idx)?;
        let params_text = head.get(open_idx + 1..)?.strip_suffix(')')?.trim();
        if params_text.starts_with("...") || Self::split_top_level_commas(params_text).len() != 1 {
            return None;
        }
        let colon_idx = Self::find_top_level_byte(params_text, b':')?;
        let name = params_text.get(..colon_idx)?.trim();
        let param_type = params_text.get(colon_idx + 1..)?.trim();
        if name.is_empty()
            || param_type.is_empty()
            || !(param_type.ends_with("[]") || param_type.starts_with("Array<"))
        {
            return None;
        }

        Some(format!(
            "{prefix}(...{name}: {param_type}) => {return_text}"
        ))
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
}
