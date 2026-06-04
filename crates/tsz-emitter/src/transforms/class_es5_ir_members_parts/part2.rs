impl<'a> ES5ClassTransformer<'a> {
    pub(super) fn has_static_property_initializer(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                return false;
            };
            self.arena
                .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                && !is_private_identifier(self.arena, prop_data.name)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                && self.property_initializer_has_equals(member_node, prop_data)
        })
    }

    fn expression_contains_static_class_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class_data) = self.arena.get_class(node)
        {
            return self.has_static_property_initializer(&class_data.members);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.expression_contains_static_class_expression(paren.expression);
        }
        if (node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && let Some(assertion) = self.arena.get_type_assertion(node)
        {
            return self.expression_contains_static_class_expression(assertion.expression);
        }
        if node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(expr_type_args) = self.arena.get_expr_type_args(node)
        {
            return self.expression_contains_static_class_expression(expr_type_args.expression);
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
        {
            return self.expression_contains_static_class_expression(unary.expression);
        }

        false
    }

    /// Check if any static property initializer or static block uses `this`.
    /// Returns true if a class alias is needed (i.e. `var _a; _a = ClassName;`).
    ///
    /// Note: `this` in static methods/getters/setters does NOT need aliasing because
    /// regular functions have their own `this` binding. Only static property initializer
    /// expressions and static block statement bodies need `this` → `_a` substitution.
    pub(super) fn static_members_need_class_alias(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        if !self.class_decorators.is_empty() {
            return false;
        }

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                // Only static properties with initializers
                if !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                {
                    continue;
                }
                if self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                    || self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }
                if !self.property_initializer_has_equals(member_node, prop_data) {
                    continue;
                }
                // Async arrows in static initializers also need the class alias:
                // tsc passes it to the downlevel `__generator` call as lexical `this`.
                if contains_this_keyword_reference(self.arena, prop_data.initializer)
                    || contains_async_arrow_function(self.arena, prop_data.initializer)
                {
                    return true;
                }
            } else if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                // Check if the static block body contains `this`
                if let Some(block_data) = self.arena.get_block(member_node) {
                    for &stmt_idx in &block_data.statements.nodes {
                        if contains_this_keyword_reference(self.arena, stmt_idx)
                            || contains_async_arrow_function(self.arena, stmt_idx)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub(super) fn async_method_promise_constructor(
        &self,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }

        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return Some(self.qualified_type_name_to_expr(type_ref.type_name));
        }

        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name =
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, type_ref.type_name);
        if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
            && name != "Promise"
            && name != "PromiseLike"
            && !self.is_type_only_declaration_name(&name)
        {
            self.commonjs_import_substitutions
                .get(&name)
                .cloned()
                .or(Some(name))
        } else {
            None
        }
    }

    fn qualified_type_name_to_expr(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.get_qualified_name(node)
        {
            let left = self.qualified_type_name_to_expr(qn.left);
            let right =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, qn.right);
            return format!("{left}.{right}");
        }
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    fn is_type_only_declaration_name(&self, name: &str) -> bool {
        if self.has_value_declaration_name(name) {
            return false;
        }

        self.arena.nodes.iter().any(|node| {
            if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                self.arena.get_type_alias(node).is_some_and(|alias| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, alias.name)
                        == name
                })
            } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena.get_interface(node).is_some_and(|interface| {
                    crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        interface.name,
                    ) == name
                })
            } else {
                false
            }
        })
    }

    fn has_value_declaration_name(&self, name: &str) -> bool {
        self.arena.nodes.iter().any(|node| match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .arena
                .get_variable(node)
                .is_some_and(|var_stmt| self.variable_statement_declares_name(var_stmt, name)),
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(node).is_some_and(|func| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, func.name)
                        == name
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(node).is_some_and(|class| {
                    crate::transforms::emit_utils::identifier_text_or_empty(self.arena, class.name)
                        == name
                })
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|enum_decl| {
                    crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        enum_decl.name,
                    ) == name
                })
            }
            _ => false,
        })
    }

    fn variable_statement_declares_name(
        &self,
        var_stmt: &tsz_parser::parser::node::VariableData,
        name: &str,
    ) -> bool {
        var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };
            decl_list.declarations.nodes.iter().any(|&decl_idx| {
                self.arena
                    .get_variable_declaration_at(decl_idx)
                    .is_some_and(|decl| {
                        crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena, decl.name,
                        ) == name
                    })
            })
        })
    }
}
