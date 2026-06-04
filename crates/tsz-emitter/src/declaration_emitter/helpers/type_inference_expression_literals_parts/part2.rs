impl<'a> DeclarationEmitter<'a> {
    fn object_literal_member_value_type_for_this_lookup(
        &self,
        member_idx: NodeIndex,
        object_context_idx: Option<NodeIndex>,
        current_member_idx: Option<NodeIndex>,
        depth: u32,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                self.preferred_object_member_initializer_type_text(data.initializer, depth)
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                let initializer = if data.object_assignment_initializer == NodeIndex::NONE {
                    data.name
                } else {
                    data.object_assignment_initializer
                };
                self.preferred_object_member_initializer_type_text(initializer, depth)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                self.infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| {
                        self.object_literal_this_property_return_type_text(
                            data.body,
                            object_context_idx,
                            current_member_idx,
                            depth,
                        )
                    })
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                self.infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| {
                        self.object_literal_this_property_return_type_text(
                            data.body,
                            object_context_idx,
                            current_member_idx,
                            depth,
                        )
                    })
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                data.parameters
                    .nodes
                    .first()
                    .and_then(|&p_idx| self.arena.get(p_idx))
                    .and_then(|p_node| self.arena.get_parameter(p_node))
                    .and_then(|param| {
                        self.infer_fallback_type_text_at(param.type_annotation, depth)
                    })
            }
            _ => None,
        }
    }

    fn object_literal_member_by_name_for_inference(
        &self,
        object_idx: NodeIndex,
        property_name: &str,
    ) -> Option<NodeIndex> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        object.elements.nodes.iter().copied().find(|member_idx| {
            let Some(member_node) = self.arena.get(*member_idx) else {
                return false;
            };
            self.object_literal_member_name_idx(member_node)
                .and_then(|name_idx| self.object_literal_member_name_text(name_idx))
                .as_deref()
                == Some(property_name)
        })
    }

    fn object_literal_method_uses_property_syntax(
        &self,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        let Some(name_node) = self.arena.get(method.name) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if self
            .resolved_computed_property_name_text(method.name)
            .is_some()
            || self.computed_property_name_is_symbol_access(method.name)
            || self.computed_property_name_is_literal_key(method.name)
        {
            return false;
        }

        let computed_key_requires_property_syntax = self
            .arena
            .get_computed_property(name_node)
            .and_then(|computed| self.get_node_type_or_names(&[computed.expression, method.name]))
            .is_none_or(|type_id| {
                type_id == tsz_solver::types::TypeId::ANY
                    || self.type_interner.is_some_and(|interner| {
                        !tsz_solver::type_queries::is_type_usable_as_property_name(
                            interner, type_id,
                        )
                    })
            });

        method.question_token || computed_key_requires_property_syntax
    }

    fn computed_property_name_is_literal_key(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NumericLiteral as u16
            || expr_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
    }

    pub(in crate::declaration_emitter) fn computed_property_name_is_symbol_access(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self.skip_parenthesized_non_null_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.get_identifier_text(access.expression).as_deref() == Some("Symbol")
    }
}
