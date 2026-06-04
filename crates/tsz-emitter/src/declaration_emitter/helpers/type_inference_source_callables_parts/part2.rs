impl<'a> DeclarationEmitter<'a> {
    fn infer_constructor_option_object_type_arguments(
        &self,
        ctor: &tsz_parser::parser::node::ConstructorData,
        args: &NodeList,
        class_type_param_names: &[String],
        inferred: &mut FxHashMap<String, String>,
    ) {
        for (&param_idx, &arg_idx) in ctor.parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !param.type_annotation.is_some() {
                continue;
            }

            let Some(arg_idx) = self.skip_parenthesized_expression(arg_idx) else {
                continue;
            };
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }

            let Some(param_type_node) = self.arena.get(param.type_annotation) else {
                continue;
            };
            if param_type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                continue;
            }
            let Some(type_ref) = self.arena.get_type_ref(param_type_node) else {
                continue;
            };
            let Some(mut option_type_sym_id) =
                self.declaration_type_symbol_from_type_node(self.arena, param.type_annotation)
            else {
                continue;
            };
            if let Some(binder) = self.binder {
                option_type_sym_id = self
                    .resolve_portability_import_alias(option_type_sym_id, binder)
                    .unwrap_or(option_type_sym_id);
                option_type_sym_id =
                    self.resolve_portability_declaration_symbol(option_type_sym_id, binder);
            }

            let Some(object_literal) = self.arena.get_literal_expr(arg_node) else {
                continue;
            };
            for member_idx in object_literal.elements.nodes.iter().copied() {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(member_name_idx) = self.object_literal_member_name_idx(member_node) else {
                    continue;
                };
                let Some(member_name) = self.object_literal_member_name_text(member_name_idx)
                else {
                    continue;
                };
                let Some(type_param_name) = self.constructor_option_member_class_type_param_name(
                    option_type_sym_id,
                    &member_name,
                    type_ref.type_arguments.as_ref(),
                    class_type_param_names,
                ) else {
                    continue;
                };
                if inferred.contains_key(&type_param_name) {
                    continue;
                }
                let Some(initializer) = self.object_literal_member_initializer(member_node) else {
                    continue;
                };
                let Some(type_text) = self
                    .preferred_expression_type_text(initializer)
                    .or_else(|| self.infer_fallback_type_text_at(initializer, 0))
                    .filter(|text| !text.is_empty() && text != "any")
                else {
                    continue;
                };
                inferred.insert(type_param_name, type_text);
            }
        }
    }

    fn constructor_option_member_class_type_param_name(
        &self,
        option_type_sym_id: SymbolId,
        member_name: &str,
        option_type_arguments: Option<&NodeList>,
        class_type_param_names: &[String],
    ) -> Option<String> {
        let (member_type_text, option_type_param_names) = self
            .type_member_source_annotation_text_and_type_params(option_type_sym_id, member_name)?;
        let member_type_name = Self::simple_type_reference_name(&member_type_text)?;
        if class_type_param_names
            .iter()
            .any(|name| name == &member_type_name)
        {
            return Some(member_type_name);
        }

        let position = option_type_param_names
            .iter()
            .position(|name| name == &member_type_name)?;
        let option_type_arg = option_type_arguments
            .and_then(|type_args| type_args.nodes.get(position).copied())
            .and_then(|arg_idx| self.simple_type_argument_source_text(arg_idx))?;
        class_type_param_names
            .iter()
            .any(|name| name == &option_type_arg)
            .then_some(option_type_arg)
    }

    fn type_member_source_annotation_text_and_type_params(
        &self,
        type_sym_id: SymbolId,
        member_name: &str,
    ) -> Option<(String, Vec<String>)> {
        self.with_symbol_declarations(type_sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let mut members: Vec<NodeIndex> = Vec::new();
            let type_param_names = if let Some(interface) = source_arena.get_interface(decl_node) {
                members.extend(interface.members.nodes.iter().copied());
                self.collect_optional_type_param_names_from_arena(
                    source_arena,
                    interface.type_parameters.as_ref(),
                )
            } else if let Some(class_decl) = source_arena.get_class(decl_node) {
                members.extend(class_decl.members.nodes.iter().copied());
                self.collect_optional_type_param_names_from_arena(
                    source_arena,
                    class_decl.type_parameters.as_ref(),
                )
            } else if let Some(type_alias) = source_arena.get_type_alias(decl_node) {
                if let Some(type_node) = source_arena.get(type_alias.type_node)
                    && type_node.kind == syntax_kind_ext::TYPE_LITERAL
                    && let Some(type_literal) = source_arena.get_type_literal(type_node)
                {
                    members.extend(type_literal.members.nodes.iter().copied());
                }
                self.collect_optional_type_param_names_from_arena(
                    source_arena,
                    type_alias.type_parameters.as_ref(),
                )
            } else {
                Vec::new()
            };

            for member_idx in members {
                let Some(member_node) = source_arena.get(member_idx) else {
                    continue;
                };
                let annotation = if let Some(signature) = source_arena.get_signature(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, signature.name)
                        .as_deref()
                        == Some(member_name)
                    && signature.type_annotation.is_some()
                {
                    Some(signature.type_annotation)
                } else if let Some(prop_decl) = source_arena.get_property_decl(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, prop_decl.name)
                        .as_deref()
                        == Some(member_name)
                    && prop_decl.type_annotation.is_some()
                {
                    Some(prop_decl.type_annotation)
                } else if let Some(accessor) = source_arena.get_accessor(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, accessor.name)
                        .as_deref()
                        == Some(member_name)
                    && accessor.type_annotation.is_some()
                {
                    Some(accessor.type_annotation)
                } else {
                    None
                };
                let Some(annotation) = annotation else {
                    continue;
                };
                let raw = self
                    .source_slice_from_arena(source_arena, annotation)
                    .or_else(|| self.emit_type_node_text_from_arena(source_arena, annotation))?;
                return Some((
                    raw.trim().trim_end_matches(';').trim().to_string(),
                    type_param_names,
                ));
            }

            None
        })
    }

    fn collect_optional_type_param_names_from_arena(
        &self,
        source_arena: &NodeArena,
        type_params: Option<&NodeList>,
    ) -> Vec<String> {
        type_params
            .map(|params| {
                params
                    .nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        let param_node = source_arena.get(param_idx)?;
                        let param = source_arena.get_type_parameter(param_node)?;
                        self.identifier_text_from_arena(source_arena, param.name)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn relaxed_jsdoc_param_type_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let jsdoc = self.nearest_jsdoc_comment_for_pos_relaxed(param_node.pos)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        let found = if let Some(name) = self.get_identifier_text(param.name) {
            params.into_iter().find(|decl| decl.name == name)
        } else {
            params.into_iter().nth(position)
        }?;
        Some(self.jsdoc_type_text_for_declaration_emit(&found.type_text))
    }

    fn enclosing_method_parameter_jsdoc_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_name = self.get_identifier_text(arg_idx)?;
        let method = self.enclosing_method_for_node(arg_idx)?;
        for (position, &param_idx) in method.parameters.nodes.iter().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(arg_name.as_str()) {
                continue;
            }
            let jsdoc_param = self.jsdoc_param_decl_for_parameter(param_idx, position)?;
            return Some(self.jsdoc_type_text_for_declaration_emit(&jsdoc_param.type_text));
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
