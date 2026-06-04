impl<'a> DeclarationEmitter<'a> {
    fn js_top_level_class_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && let Some(name) = self.get_identifier_text(class.name)
                {
                    names.insert(name);
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            if let Some(class) = self.arena.get_class(clause_node)
                && let Some(name) = self.get_identifier_text(class.name)
            {
                names.insert(name);
            }
        }
        names
    }

    fn js_class_define_property_accessor_for_statement(
        &self,
        stmt_idx: NodeIndex,
        class_names: &FxHashSet<String>,
    ) -> Option<(String, JsClassDefinePropertyAccessor)> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        if !self.is_object_define_property_call(call.expression) || args.nodes.len() < 3 {
            return None;
        }

        let target = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(args.nodes[0]);
        let target_node = self.arena.get(target)?;
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let target_access = self.arena.get_access_expr(target_node)?;
        if self
            .get_identifier_text(target_access.name_or_argument)
            .as_deref()
            != Some("prototype")
        {
            return None;
        }
        let class_name = self.get_identifier_text(target_access.expression)?;
        if !class_names.contains(&class_name) {
            return None;
        }

        let property_name = self.js_define_property_name(args.nodes[1])?;
        let descriptor_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(args.nodes[2]);
        let descriptor_node = self.arena.get(descriptor_idx)?;
        if descriptor_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let descriptor = self.arena.get_literal_expr(descriptor_node)?;
        let mut getter = None;
        let mut setter = None;

        for &member_idx in &descriptor.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method) = self.arena.get_method_decl(member_node) else {
                    continue;
                };
                match self.get_identifier_text(method.name).as_deref() {
                    Some("get") => getter = Some(member_idx),
                    Some("set") => {
                        setter = Some(JsClassDefinePropertySetter {
                            initializer: member_idx,
                            preserve_param_name: true,
                        });
                    }
                    _ => {}
                }
                continue;
            }
            if member_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.arena.get_property_assignment(member_node) else {
                continue;
            };
            match self.get_identifier_text(prop.name).as_deref() {
                Some("get") => getter = Some(prop.initializer),
                Some("set") => {
                    setter = Some(JsClassDefinePropertySetter {
                        initializer: prop.initializer,
                        preserve_param_name: false,
                    });
                }
                _ => {}
            }
        }

        if getter.is_none() && setter.is_none() {
            return None;
        }

        Some((
            class_name,
            JsClassDefinePropertyAccessor {
                property_name,
                getter,
                setter,
            },
        ))
    }
}
