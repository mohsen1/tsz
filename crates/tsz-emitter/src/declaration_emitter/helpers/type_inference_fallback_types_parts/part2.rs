impl<'a> DeclarationEmitter<'a> {
    fn strip_json_comments_and_trailing_commas(text: &str) -> String {
        let mut without_comments = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let mut in_string = false;
        let mut escaped = false;
        while let Some(ch) = chars.next() {
            if in_string {
                without_comments.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }

            if ch == '"' {
                in_string = true;
                without_comments.push(ch);
                continue;
            }

            if ch == '/' {
                match chars.peek().copied() {
                    Some('/') => {
                        chars.next();
                        for next in chars.by_ref() {
                            if next == '\n' {
                                without_comments.push('\n');
                                break;
                            }
                        }
                        continue;
                    }
                    Some('*') => {
                        chars.next();
                        let mut prev = '\0';
                        for next in chars.by_ref() {
                            if prev == '*' && next == '/' {
                                break;
                            }
                            prev = next;
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            without_comments.push(ch);
        }

        let chars: Vec<char> = without_comments.chars().collect();
        let mut result = String::with_capacity(chars.len());
        let mut index = 0usize;
        in_string = false;
        escaped = false;
        while index < chars.len() {
            let ch = chars[index];
            if in_string {
                result.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                index += 1;
                continue;
            }

            if ch == '"' {
                in_string = true;
                result.push(ch);
                index += 1;
                continue;
            }

            if ch == ',' {
                let mut lookahead = index + 1;
                while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                    lookahead += 1;
                }
                if lookahead < chars.len() && matches!(chars[lookahead], '}' | ']') {
                    index += 1;
                    continue;
                }
            }

            result.push(ch);
            index += 1;
        }
        result
    }

    fn conditional_unique_symbol_union_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let conditional = self.arena.get_conditional_expr(expr_node)?;
        let when_true = self.unique_symbol_reference_typeof_text(conditional.when_true)?;
        let when_false = self.unique_symbol_reference_typeof_text(conditional.when_false)?;
        if when_true == when_false {
            Some(when_true)
        } else {
            Some(format!("{when_true} | {when_false}"))
        }
    }

    fn unique_symbol_reference_typeof_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let name = self.get_identifier_text(expr_idx)?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        if !self.symbol_has_unique_symbol_type(sym_id) {
            return None;
        }
        Some(format!("typeof {name}"))
    }

    pub(in crate::declaration_emitter) fn symbol_has_unique_symbol_type(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));

        if let (Some(cache), Some(interner)) = (self.type_cache.as_ref(), self.type_interner)
            && let Some(type_id) = cache.symbol_types.get(&resolved_sym_id).copied()
            && tsz_solver::type_queries::is_unique_symbol_type(interner, type_id)
        {
            return true;
        }

        let Some(symbol) = binder.symbols.get(resolved_sym_id) else {
            return false;
        };
        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                return false;
            };
            if var_decl
                .type_annotation
                .into_option()
                .is_some_and(|type_idx| {
                    self.emit_type_node_text(type_idx).as_deref() == Some("unique symbol")
                })
            {
                return true;
            }
            self.arena.is_const_variable_declaration(decl_idx)
                && var_decl.initializer.is_some()
                && self.is_symbol_call(var_decl.initializer)
        })
    }

    pub(in crate::declaration_emitter) fn super_method_call_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let access_node = self.arena.get(call.expression)?;
        if access_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(access_node)?;
        if self
            .arena
            .get(access.expression)
            .is_none_or(|node| node.kind != SyntaxKind::SuperKeyword as u16)
        {
            return None;
        }
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let is_static_context = self
            .enclosing_method_for_node(expr_idx)
            .is_some_and(|method| self.arena.is_static(&method.modifiers));
        let method_idx =
            self.super_method_declaration(expr_idx, &method_name, is_static_context)?;
        let method_node = self.arena.get(method_idx)?;
        let method = self.arena.get_method_decl(method_node)?;
        self.method_source_return_type_text(method_idx, method)
    }

    fn super_method_declaration(
        &self,
        expr_idx: NodeIndex,
        method_name: &str,
        is_static_context: bool,
    ) -> Option<NodeIndex> {
        let class_idx = self.enclosing_class_for_node(expr_idx)?;
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;
        let base_expr = self.class_extends_expression(class)?;
        let base_sym = self.value_reference_symbol(base_expr)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(base_sym)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(base_class) = self.arena.get_class(decl_node) else {
                continue;
            };
            if let Some(method_idx) =
                self.class_method_named(base_class, method_name, is_static_context)
            {
                return Some(method_idx);
            }
        }

        None
    }

    fn method_source_return_type_text(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> Option<String> {
        if method.type_annotation.is_some() {
            return self.emit_type_node_text(method.type_annotation);
        }
        if method.body.is_some() {
            if self.body_returns_void(method.body) {
                return Some("void".to_string());
            }
            if let Some(type_text) = self.function_body_preferred_return_type_text(method.body) {
                return Some(type_text);
            }
        }

        let method_type_id = self
            .get_node_type_or_names(&[method_idx, method.name])
            .or_else(|| self.get_type_via_symbol_for_func(method_idx, method.name))?;
        let Some(interner) = self.type_interner else {
            return Some(self.print_type_id(method_type_id));
        };
        tsz_solver::type_queries::get_return_type(interner, method_type_id)
            .map(|return_type| self.print_type_id(return_type))
            .or_else(|| Some(self.print_type_id(method_type_id)))
    }

    pub(in crate::declaration_emitter) fn enclosing_method_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::MethodDeclData> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some()
                || self.arena.get_class(parent_node).is_some()
            {
                return None;
            }
            if let Some(method) = self.arena.get_method_decl(parent_node) {
                return Some(method);
            }
            current = parent_idx;
        }
        None
    }

    fn enclosing_class_for_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if self.arena.get_source_file(parent_node).is_some() {
                return None;
            }
            if self.arena.get_class(parent_node).is_some() {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn class_extends_expression(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<NodeIndex> {
        let heritage_clauses = class.heritage_clauses.as_ref()?;
        for clause_idx in heritage_clauses.nodes.iter().copied() {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let base_idx = heritage.types.nodes.first().copied()?;
            let base_node = self.arena.get(base_idx)?;
            return self
                .arena
                .get_expr_type_args(base_node)
                .map(|expr| expr.expression)
                .or(Some(base_idx));
        }
        None
    }

    fn class_method_named(
        &self,
        class: &tsz_parser::parser::node::ClassData,
        method_name: &str,
        is_static: bool,
    ) -> Option<NodeIndex> {
        class.members.nodes.iter().copied().find(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                return false;
            };
            self.arena.is_static(&method.modifiers) == is_static
                && self.get_identifier_text(method.name).as_deref() == Some(method_name)
        })
    }
}
