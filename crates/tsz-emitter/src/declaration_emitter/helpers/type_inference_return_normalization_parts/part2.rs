impl<'a> DeclarationEmitter<'a> {
    fn lexical_function_source_indexed_call_return_type_text(
        &self,
        call: &CallExprData,
    ) -> Option<String> {
        let callee_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(call.expression);
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let callee_name = self.get_identifier_text(callee_idx)?;
        let mut candidate = None;
        for node in &self.arena.nodes {
            if node.kind != syntax_kind_ext::FUNCTION_DECLARATION || node.pos > callee_node.pos {
                continue;
            }
            let Some(func) = self.arena.get_function(node) else {
                continue;
            };
            if self.get_identifier_text(func.name).as_deref() != Some(callee_name.as_str()) {
                continue;
            }
            let Some(type_text) =
                self.callable_source_indexed_access_return_type_text(self.arena, func)
            else {
                continue;
            };
            let Some(type_text) =
                self.substitute_indexed_call_type_parameters(self.arena, func, call, type_text)
            else {
                continue;
            };
            if candidate.replace(type_text).is_some() {
                return None;
            }
        }
        candidate
    }

    fn local_variable_initializer_for_name_in_statement(
        &self,
        node_idx: NodeIndex,
        local_name: &str,
    ) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::METHOD_DECLARATION
        {
            return None;
        }
        if let Some(var_decl) = self.arena.get_variable_declaration(node)
            && var_decl.initializer.is_some()
            && self.get_identifier_text(var_decl.name).as_deref() == Some(local_name)
        {
            return Some(var_decl.initializer);
        }
        let mut candidate = None;
        for child_idx in self.arena.get_children(node_idx) {
            if let Some(initializer) =
                self.local_variable_initializer_for_name_in_statement(child_idx, local_name)
            {
                if candidate.replace(initializer).is_some() {
                    return None;
                }
            }
        }
        candidate
    }

    fn statement_ancestor_in_block(&self, from_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = from_idx;
        for _ in 0..64 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::BLOCK {
                return Some(current);
            }
            current = parent_idx;
        }
        None
    }

    fn node_writes_identifier(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(node)
            && binary.operator_token >= SyntaxKind::EqualsToken as u16
            && binary.operator_token <= SyntaxKind::CaretEqualsToken as u16
        {
            return self.node_contains_identifier(binary.left, name);
        }
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node)
            && (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
        {
            return self.node_contains_identifier(unary.operand, name);
        }
        self.arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.node_writes_identifier(child_idx, name))
    }

    fn node_contains_identifier(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16
            && self.get_identifier_text(node_idx).as_deref() == Some(name)
        {
            return true;
        }
        self.arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.node_contains_identifier(child_idx, name))
    }

    fn callable_source_indexed_access_return_type_text(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        if func.type_annotation.is_some() {
            let type_idx =
                source_arena.skip_parenthesized_and_assertions_and_comma(func.type_annotation);
            let type_node = source_arena.get(type_idx)?;
            if type_node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
                return self
                    .source_slice_from_arena(source_arena, func.type_annotation)
                    .map(|text| text.trim().to_string());
            }
        }

        if func.body.is_some() {
            if std::ptr::eq(source_arena, self.arena) {
                return self.function_body_source_indexed_access_return_type_text(func.body);
            }
            let scratch = DeclarationEmitter::new(source_arena);
            return scratch.function_body_source_indexed_access_return_type_text(func.body);
        }

        None
    }

    fn this_method_source_indexed_call_return_type_text(
        &self,
        call: &CallExprData,
    ) -> Option<String> {
        let callee_node = self.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        if self
            .arena
            .get(receiver_idx)
            .is_none_or(|node| node.kind != SyntaxKind::ThisKeyword as u16)
        {
            return None;
        }
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let class_idx = self.enclosing_class_declaration_index(call.expression)?;
        let class = self.arena.get_class_at(class_idx)?;
        self.class_method_source_indexed_call_return_type_text(
            self.arena,
            class,
            &method_name,
            call,
            0,
        )
    }

    fn class_method_source_indexed_call_return_type_text(
        &self,
        source_arena: &NodeArena,
        class: &ClassData,
        method_name: &str,
        call: &CallExprData,
        depth: usize,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }

        let mut saw_named_member = false;
        let mut candidate = None;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = source_arena.get(member_idx) else {
                continue;
            };
            let member_name = source_arena
                .get_method_decl(member_node)
                .and_then(|method| self.property_name_text_from_arena(source_arena, method.name));
            if member_name.as_deref() != Some(method_name) {
                continue;
            }
            saw_named_member = true;
            let Some(method) = source_arena.get_method_decl(member_node) else {
                continue;
            };
            let Some(type_text) =
                self.method_source_indexed_access_return_type_text(source_arena, method)
            else {
                continue;
            };
            let type_text = self.substitute_indexed_callable_type_parameters(
                source_arena,
                method.type_parameters.as_ref(),
                &method.parameters,
                call,
                type_text,
            )?;
            if candidate.replace(type_text).is_some() {
                return None;
            }
        }
        if candidate.is_some() || saw_named_member {
            return candidate;
        }

        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for &heritage_idx in &heritage_clauses.nodes {
            let Some(heritage) = source_arena.get_heritage_clause_at(heritage_idx) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &base_idx in &heritage.types.nodes {
                let base_expr = source_arena
                    .get_expr_type_args_at(base_idx)
                    .map_or(base_idx, |expr| expr.expression);
                let Some(base_sym_id) =
                    self.declaration_type_symbol_from_type_node(source_arena, base_expr)
                else {
                    continue;
                };
                if let Some(type_text) =
                    self.with_symbol_declarations(base_sym_id, |base_arena, decl_idx| {
                        let base_node = base_arena.get(decl_idx)?;
                        let base_class = base_arena.get_class(base_node)?;
                        self.class_method_source_indexed_call_return_type_text(
                            base_arena,
                            base_class,
                            method_name,
                            call,
                            depth + 1,
                        )
                    })
                {
                    return Some(type_text);
                }
            }
        }
        None
    }

    fn method_source_indexed_access_return_type_text(
        &self,
        source_arena: &NodeArena,
        method: &MethodDeclData,
    ) -> Option<String> {
        if method.type_annotation.is_some() {
            let type_idx =
                source_arena.skip_parenthesized_and_assertions_and_comma(method.type_annotation);
            let type_node = source_arena.get(type_idx)?;
            if type_node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
                return self
                    .source_slice_from_arena(source_arena, method.type_annotation)
                    .map(|text| text.trim().to_string());
            }
        }

        if method.body.is_some() {
            if std::ptr::eq(source_arena, self.arena) {
                return self.function_body_source_indexed_access_return_type_text(method.body);
            }
            let scratch = DeclarationEmitter::new(source_arena);
            return scratch.function_body_source_indexed_access_return_type_text(method.body);
        }

        None
    }

    fn substitute_indexed_call_type_parameters(
        &self,
        source_arena: &NodeArena,
        func: &tsz_parser::parser::node::FunctionData,
        call: &tsz_parser::parser::node::CallExprData,
        type_text: String,
    ) -> Option<String> {
        self.substitute_indexed_callable_type_parameters(
            source_arena,
            func.type_parameters.as_ref(),
            &func.parameters,
            call,
            type_text,
        )
    }

    fn substitute_indexed_callable_type_parameters(
        &self,
        source_arena: &NodeArena,
        type_parameters: Option<&NodeList>,
        parameters: &NodeList,
        call: &CallExprData,
        type_text: String,
    ) -> Option<String> {
        let Some(type_params) = type_parameters else {
            return Some(type_text);
        };
        if type_params.nodes.is_empty() {
            return Some(type_text);
        }

        let type_param_names = self.collect_type_param_names_from_arena(source_arena, type_params);
        if type_param_names.is_empty() {
            return Some(type_text);
        }

        let mut substitutions = Vec::new();
        if call
            .type_arguments
            .as_ref()
            .is_some_and(|type_args| !type_args.nodes.is_empty())
        {
            let explicit_type_args =
                self.type_argument_list_source_text(call.type_arguments.as_ref());
            for (name, value) in type_param_names.iter().zip(explicit_type_args.iter()) {
                substitutions.push((name.clone(), value.clone()));
            }
        } else if let Some(args) = call.arguments.as_ref() {
            for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
                let Some(param_node) = source_arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = source_arena.get_parameter(param_node) else {
                    continue;
                };
                let Some(param_type_text) = self
                    .source_slice_from_arena(source_arena, param.type_annotation)
                    .or_else(|| {
                        self.emit_type_node_text_from_arena(source_arena, param.type_annotation)
                    })
                    .map(|text| text.trim().to_string())
                else {
                    continue;
                };
                if !type_param_names
                    .iter()
                    .any(|name| name.as_str() == param_type_text)
                    || substitutions
                        .iter()
                        .any(|(name, _)| name.as_str() == param_type_text)
                {
                    continue;
                }
                let Some(arg_text) = self.indexed_call_argument_type_text(arg_idx) else {
                    continue;
                };
                substitutions.push((param_type_text, arg_text));
            }
        }

        let type_text = Self::replace_whole_words_in_text(&type_text, &substitutions);
        if type_param_names.iter().any(|name| {
            Self::contains_whole_word_in_text(&type_text, name)
                && !substitutions
                    .iter()
                    .any(|(substituted_name, _)| substituted_name == name)
        }) {
            return None;
        }
        Some(type_text)
    }
}
