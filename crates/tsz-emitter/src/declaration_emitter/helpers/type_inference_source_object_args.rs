//! Source object-argument inference helpers for declaration emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn infer_call_type_param_substitutions_from_type_text_argument(
        &self,
        source_arena: &NodeArena,
        param_type_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some((alias_name, type_args)) = Self::type_reference_application_parts(param_type_text)
        else {
            return;
        };
        let Some(alias_type_node) =
            self.find_type_alias_type_node_in_arena(source_arena, alias_name)
        else {
            return;
        };
        let Some(alias_decl_idx) = self.find_type_alias_decl_in_arena(source_arena, alias_name)
        else {
            return;
        };
        let Some(alias_decl_node) = source_arena.get(alias_decl_idx) else {
            return;
        };
        let Some(alias_decl) = source_arena.get_type_alias(alias_decl_node) else {
            return;
        };
        let Some(alias_type_params) = alias_decl.type_parameters.as_ref() else {
            return;
        };
        let mut aliases = Vec::new();
        for (&param_idx, type_arg) in alias_type_params.nodes.iter().zip(type_args.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(alias_param) = identifier_text(source_arena, param.name) else {
                continue;
            };
            if let Some(type_param_name) =
                Self::mapped_type_param_name(type_arg, type_param_names, &[])
            {
                aliases.push((alias_param, type_param_name));
            }
        }
        self.infer_object_argument_substitutions_from_type_node(
            source_arena,
            alias_type_node,
            arg_idx,
            type_param_names,
            &aliases,
            substitutions,
            0,
        );
    }

    pub(in crate::declaration_emitter) fn infer_object_argument_substitutions_from_type_node(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
        substitutions: &mut Vec<(String, String)>,
        depth: u8,
    ) {
        if depth > 16 {
            return;
        }
        let Some(type_node) = source_arena.get(type_idx) else {
            return;
        };
        match type_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name) = identifier_text(source_arena, type_idx)
                    && let Some(type_param_name) =
                        Self::mapped_type_param_name(&name, type_param_names, aliases)
                    && !substitutions
                        .iter()
                        .any(|(existing, _)| existing == &type_param_name)
                    && let Some(type_text) = self.call_argument_public_type_text(arg_idx)
                {
                    substitutions.push((type_param_name, type_text));
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = source_arena.get_wrapped_type(type_node) {
                    self.infer_object_argument_substitutions_from_type_node(
                        source_arena,
                        wrapped.type_node,
                        arg_idx,
                        type_param_names,
                        aliases,
                        substitutions,
                        depth + 1,
                    );
                }
            }
            k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
                if let Some(composite) = source_arena.get_composite_type(type_node) {
                    for part_idx in composite.types.nodes.iter().copied() {
                        self.infer_object_argument_substitutions_from_type_node(
                            source_arena,
                            part_idx,
                            arg_idx,
                            type_param_names,
                            aliases,
                            substitutions,
                            depth + 1,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                self.infer_object_argument_substitutions_from_type_literal(
                    source_arena,
                    type_idx,
                    arg_idx,
                    type_param_names,
                    aliases,
                    substitutions,
                    depth + 1,
                );
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                self.infer_object_argument_substitution_from_mapped_type(
                    source_arena,
                    type_idx,
                    arg_idx,
                    type_param_names,
                    aliases,
                    substitutions,
                );
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = source_arena.get_type_ref(type_node) else {
                    return;
                };
                if let Some(name) = identifier_text(source_arena, type_ref.type_name)
                    && let Some(type_param_name) =
                        Self::mapped_type_param_name(&name, type_param_names, aliases)
                    && !substitutions
                        .iter()
                        .any(|(existing, _)| existing == &type_param_name)
                    && let Some(type_text) = self.call_argument_public_type_text(arg_idx)
                {
                    substitutions.push((type_param_name, type_text));
                    return;
                }

                if let Some((type_param_name, value_text)) = self
                    .infer_mapped_alias_argument_substitution(
                        source_arena,
                        type_idx,
                        arg_idx,
                        type_param_names,
                        aliases,
                    )
                    && !substitutions
                        .iter()
                        .any(|(existing, _)| existing == &type_param_name)
                {
                    substitutions.push((type_param_name, value_text));
                    return;
                }

                if let Some((type_param_name, value_text)) = self
                    .infer_descriptor_argument_substitution(
                        source_arena,
                        type_idx,
                        arg_idx,
                        type_param_names,
                        aliases,
                    )
                    && !substitutions
                        .iter()
                        .any(|(existing, _)| existing == &type_param_name)
                {
                    substitutions.push((type_param_name, value_text));
                    return;
                }

                let Some(alias_sym_id) =
                    self.declaration_type_symbol_from_type_node(source_arena, type_idx)
                else {
                    return;
                };
                self.with_symbol_declarations(alias_sym_id, |alias_arena, decl_idx| {
                    let decl_node = alias_arena.get(decl_idx)?;
                    let alias = alias_arena.get_type_alias(decl_node)?;
                    let alias_type_params = alias.type_parameters.as_ref()?;
                    let alias_args = type_ref.type_arguments.as_ref()?;
                    let mut next_aliases = aliases.to_vec();
                    for (&param_idx, &arg_idx) in
                        alias_type_params.nodes.iter().zip(alias_args.nodes.iter())
                    {
                        let param_node = alias_arena.get(param_idx)?;
                        let param = alias_arena.get_type_parameter(param_node)?;
                        let alias_param = identifier_text(alias_arena, param.name)?;
                        let arg_text =
                            self.emit_type_node_text_from_arena(source_arena, arg_idx)
                                .or_else(|| self.source_slice_from_arena(source_arena, arg_idx))?;
                        if let Some(type_param_name) =
                            Self::mapped_type_param_name(&arg_text, type_param_names, aliases)
                        {
                            next_aliases.push((alias_param, type_param_name));
                        }
                    }
                    self.infer_object_argument_substitutions_from_type_node(
                        alias_arena,
                        alias.type_node,
                        arg_idx,
                        type_param_names,
                        &next_aliases,
                        substitutions,
                        depth + 1,
                    );
                    Some(())
                });
            }
            _ => {}
        }
    }

    fn infer_mapped_alias_argument_substitution(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
    ) -> Option<(String, String)> {
        let type_node = source_arena.get(type_idx)?;
        let type_ref = source_arena.get_type_ref(type_node)?;
        let type_args = type_ref.type_arguments.as_ref()?;
        let [type_arg_idx] = type_args.nodes.as_slice() else {
            return None;
        };
        let type_arg_text = self
            .source_slice_from_arena(source_arena, *type_arg_idx)
            .or_else(|| self.emit_type_node_text_from_arena(source_arena, *type_arg_idx))?;
        let type_param_name =
            Self::mapped_type_param_name(type_arg_text.trim(), type_param_names, aliases)?;
        let symbol_alias_is_mapped = self
            .declaration_type_symbol_from_type_node(source_arena, type_idx)
            .and_then(|alias_sym_id| {
                self.with_symbol_declarations(alias_sym_id, |alias_arena, decl_idx| {
                    let decl_node = alias_arena.get(decl_idx)?;
                    let alias = alias_arena.get_type_alias(decl_node)?;
                    let alias_type_node = alias_arena.get(alias.type_node)?;
                    (alias_type_node.kind == syntax_kind_ext::MAPPED_TYPE).then_some(())
                })
            })
            .is_some();
        let local_alias_is_mapped = identifier_text(source_arena, type_ref.type_name)
            .and_then(|name| self.find_type_alias_type_node_in_arena(source_arena, &name))
            .and_then(|alias_type_idx| source_arena.get(alias_type_idx))
            .is_some_and(|alias_type_node| alias_type_node.kind == syntax_kind_ext::MAPPED_TYPE);
        if !symbol_alias_is_mapped && !local_alias_is_mapped {
            return None;
        }
        let value_text = self.object_literal_property_value_map_type_text(arg_idx)?;
        Some((type_param_name, value_text))
    }

    fn infer_object_argument_substitutions_from_type_literal(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
        substitutions: &mut Vec<(String, String)>,
        depth: u8,
    ) {
        let Some(arg_idx) = self.skip_parenthesized_expression(arg_idx) else {
            return;
        };
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return;
        };
        if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(type_node) = source_arena.get(type_idx) else {
            return;
        };
        let Some(type_literal) = source_arena.get_type_literal(type_node) else {
            return;
        };

        for member_idx in type_literal.members.nodes.iter().copied() {
            let Some(member_node) = source_arena.get(member_idx) else {
                continue;
            };
            let Some(signature) = source_arena.get_signature(member_node) else {
                continue;
            };
            let Some(property_name) = identifier_text(source_arena, signature.name) else {
                continue;
            };
            let Some(arg_member_idx) = self.object_literal_member_by_name(arg_idx, &property_name)
            else {
                continue;
            };

            if let Some((type_param_name, value_text)) = self
                .infer_descriptor_argument_substitution(
                    source_arena,
                    signature.type_annotation,
                    arg_member_idx,
                    type_param_names,
                    aliases,
                )
                && !substitutions
                    .iter()
                    .any(|(existing, _)| existing == &type_param_name)
            {
                substitutions.push((type_param_name, value_text));
                continue;
            }

            let raw_member_type_text = self
                .source_slice_from_arena(source_arena, signature.type_annotation)
                .or_else(|| {
                    self.emit_type_node_text_from_arena(source_arena, signature.type_annotation)
                });
            let Some(member_type_text) = self
                .emit_type_node_text_from_arena(source_arena, signature.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, signature.type_annotation))
            else {
                continue;
            };
            let searchable = Self::type_text_without_this_type_markers(&member_type_text);
            if let Some(raw_member_type_text) = raw_member_type_text.as_deref()
                && let Some((type_param_name, value_text)) = self
                    .infer_object_map_substitution_from_annotation_text(
                        raw_member_type_text,
                        arg_member_idx,
                        type_param_names,
                        aliases,
                        &[],
                    )
            {
                Self::replace_or_push_substitution(substitutions, type_param_name, value_text);
                continue;
            }
            if let Some((type_param_name, value_text)) = self
                .infer_object_map_substitution_from_annotation_text(
                    &searchable,
                    arg_member_idx,
                    type_param_names,
                    aliases,
                    &[],
                )
            {
                Self::replace_or_push_substitution(substitutions, type_param_name, value_text);
                continue;
            }
            for type_param_name in type_param_names {
                let mentions_param =
                    Self::contains_whole_word_in_text(&searchable, type_param_name)
                        || aliases.iter().any(|(alias, mapped)| {
                            mapped == type_param_name
                                && Self::contains_whole_word_in_text(&searchable, alias)
                        });
                if !mentions_param
                    || substitutions
                        .iter()
                        .any(|(existing, _)| existing == type_param_name)
                {
                    continue;
                }
                if let Some(value_text) =
                    self.object_member_public_type_text_for_annotation(arg_member_idx, &searchable)
                {
                    substitutions.push((type_param_name.clone(), value_text));
                    break;
                }
            }

            self.infer_object_argument_substitutions_from_type_node(
                source_arena,
                signature.type_annotation,
                arg_member_idx,
                type_param_names,
                aliases,
                substitutions,
                depth + 1,
            );
        }
    }

    fn infer_object_argument_substitution_from_mapped_type(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(arg_idx) = self.skip_parenthesized_expression(arg_idx) else {
            return;
        };
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return;
        };
        if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(mapped_node) = source_arena.get(type_idx) else {
            return;
        };
        let Some(mapped) = source_arena.get_mapped_type(mapped_node) else {
            return;
        };
        let Some(type_param_node) = source_arena.get(mapped.type_parameter) else {
            return;
        };
        let Some(type_param) = source_arena.get_type_parameter(type_param_node) else {
            return;
        };
        let Some(constraint_text) = self
            .emit_type_node_text_from_arena(source_arena, type_param.constraint)
            .or_else(|| self.source_slice_from_arena(source_arena, type_param.constraint))
        else {
            return;
        };
        let Some(indexed_param) = constraint_text.trim().strip_prefix("keyof ").map(str::trim)
        else {
            return;
        };
        let Some(type_param_name) =
            Self::mapped_type_param_name(indexed_param, type_param_names, aliases)
        else {
            return;
        };
        if substitutions
            .iter()
            .any(|(existing, _)| existing == &type_param_name)
        {
            return;
        }
        let Some(object) = self.arena.get_literal_expr(arg_node) else {
            return;
        };

        let mut lines = Vec::new();
        for member_idx in object.elements.nodes.iter().copied() {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            if !Self::is_simple_identifier_text(&name_text) {
                continue;
            }
            let value_text = self
                .object_literal_member_initializer(member_node)
                .and_then(|initializer| {
                    self.infer_descriptor_value_type_from_annotation(
                        source_arena,
                        mapped.type_node,
                        initializer,
                    )
                })
                .or_else(|| self.object_member_public_type_text(member_idx));
            let Some(value_text) = value_text else {
                continue;
            };
            lines.push(format!("    {name_text}: {value_text};"));
        }
        if !lines.is_empty() {
            substitutions.push((type_param_name, format!("{{\n{}\n}}", lines.join("\n"))));
        }
    }

    fn infer_descriptor_argument_substitution(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
    ) -> Option<(String, String)> {
        let type_node = source_arena.get(type_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = source_arena.get_type_ref(type_node)?;
        let type_args = type_ref.type_arguments.as_ref()?;
        let [type_arg_idx] = type_args.nodes.as_slice() else {
            return None;
        };
        let type_arg_text = self
            .emit_type_node_text_from_arena(source_arena, *type_arg_idx)
            .or_else(|| self.source_slice_from_arena(source_arena, *type_arg_idx))?;
        let type_param_name =
            Self::mapped_type_param_name(type_arg_text.trim(), type_param_names, aliases)?;
        let value_text =
            self.infer_descriptor_value_type_from_annotation(source_arena, type_idx, arg_idx)?;
        Some((type_param_name, value_text))
    }

    fn infer_descriptor_value_type_from_annotation(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = source_arena.get(type_idx)?;
        if type_node.kind == syntax_kind_ext::UNION_TYPE {
            let union = source_arena.get_composite_type(type_node)?;
            return union.types.nodes.iter().copied().find_map(|part_idx| {
                self.infer_descriptor_value_type_from_annotation(source_arena, part_idx, arg_idx)
            });
        }
        if type_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let wrapped = source_arena.get_wrapped_type(type_node)?;
            return self.infer_descriptor_value_type_from_annotation(
                source_arena,
                wrapped.type_node,
                arg_idx,
            );
        }
        if type_node.kind == syntax_kind_ext::FUNCTION_TYPE {
            return self.object_member_public_type_text(arg_idx);
        }
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let alias_sym_id = self.declaration_type_symbol_from_type_node(source_arena, type_idx)?;
        self.with_symbol_declarations(alias_sym_id, |alias_arena, decl_idx| {
            let decl_node = alias_arena.get(decl_idx)?;
            let alias = alias_arena.get_type_alias(decl_node)?;
            let alias_type_node = alias_arena.get(alias.type_node)?;
            if alias_type_node.kind != syntax_kind_ext::TYPE_LITERAL {
                return None;
            }
            let type_literal = alias_arena.get_type_literal(alias_type_node)?;
            for member_idx in type_literal.members.nodes.iter().copied() {
                let member_node = alias_arena.get(member_idx)?;
                let signature = alias_arena.get_signature(member_node)?;
                let member_name = identifier_text(alias_arena, signature.name)?;
                if member_name == "value" {
                    if let Some(value_member_idx) =
                        self.object_literal_member_by_name(arg_idx, "value")
                        && let Some(type_text) =
                            self.object_member_public_type_text(value_member_idx)
                    {
                        return Some(type_text);
                    }
                } else if member_name == "get"
                    && let Some(get_member_idx) = self.object_literal_member_by_name(arg_idx, "get")
                    && let Some(type_text) = self.object_member_public_type_text(get_member_idx)
                {
                    return Some(type_text);
                } else if member_name == "set"
                    && let Some(set_member_idx) = self.object_literal_member_by_name(arg_idx, "set")
                    && let Some(type_text) = self.setter_first_parameter_type_text(set_member_idx)
                {
                    return Some(type_text);
                }
            }
            None
        })
    }

    fn object_literal_member_by_name(
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

    pub(in crate::declaration_emitter) fn infer_object_map_substitution_from_annotation_text(
        &self,
        annotation_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        aliases: &[(String, String)],
        existing_substitutions: &[(String, String)],
    ) -> Option<(String, String)> {
        if !annotation_text.contains('<') {
            return None;
        }
        let mentioned = type_param_names
            .iter()
            .filter(|name| {
                !existing_substitutions
                    .iter()
                    .any(|(existing, _)| existing == *name)
                    && (Self::contains_whole_word_in_text(annotation_text, name)
                        || aliases.iter().any(|(alias, mapped)| {
                            mapped == *name
                                && Self::contains_whole_word_in_text(annotation_text, alias)
                        }))
            })
            .cloned()
            .collect::<Vec<_>>();
        let [type_param_name] = mentioned.as_slice() else {
            return None;
        };
        let object_text = self.object_literal_property_value_map_type_text(arg_idx)?;
        Some((type_param_name.clone(), object_text))
    }

    fn object_literal_property_value_map_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        let arg_idx = if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            arg_idx
        } else {
            self.object_literal_member_initializer(arg_node)?
        };
        let arg_node = self.arena.get(arg_idx)?;
        let object = self.arena.get_literal_expr(arg_node)?;
        let mut lines = Vec::new();
        for member_idx in object.elements.nodes.iter().copied() {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name_text = self.object_literal_member_name_text(name_idx)?;
            if !Self::is_simple_identifier_text(&name_text) {
                return None;
            }
            let value_text = self
                .descriptor_like_object_member_value_type_text(member_idx)
                .or_else(|| self.object_member_public_type_text(member_idx))?;
            lines.push(format!("    {name_text}: {value_text};"));
        }
        (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
    }

    fn descriptor_like_object_member_value_type_text(
        &self,
        member_idx: NodeIndex,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let initializer = self.object_literal_member_initializer(member_node)?;
        self.object_literal_member_by_name(initializer, "value")
            .and_then(|value_member| self.object_member_public_type_text(value_member))
            .or_else(|| {
                self.object_literal_member_by_name(initializer, "get")
                    .and_then(|get_member| self.object_member_public_type_text(get_member))
            })
            .or_else(|| {
                self.object_literal_member_by_name(initializer, "set")
                    .and_then(|set_member| self.setter_first_parameter_type_text(set_member))
            })
    }

    fn setter_first_parameter_type_text(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let method = self.arena.get_method_decl(member_node)?;
        let param_idx = method.parameters.nodes.first().copied()?;
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        self.emit_type_node_text(param.type_annotation)
            .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))
    }

    fn object_member_public_type_text(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        if let Some(initializer) = self.object_literal_member_initializer(member_node) {
            return self.call_argument_public_type_text(initializer);
        }
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return self
                .emit_type_node_text(method.type_annotation)
                .or_else(|| self.function_body_preferred_return_type_text(method.body))
                .or_else(|| self.infer_fallback_type_text_at(method.body, 0))
                .map(|text| Self::widen_public_literal_type_text(&text));
        }
        if let Some(accessor) = self.arena.get_accessor(member_node)
            && accessor.body.is_some()
        {
            return self
                .emit_type_node_text(accessor.type_annotation)
                .or_else(|| self.function_body_preferred_return_type_text(accessor.body))
                .or_else(|| self.infer_fallback_type_text_at(accessor.body, 0))
                .map(|text| Self::widen_public_literal_type_text(&text));
        }
        None
    }

    fn object_member_public_type_text_for_annotation(
        &self,
        member_idx: NodeIndex,
        annotation_text: &str,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        if annotation_text.contains("=>")
            && let Some(initializer) = self.object_literal_member_initializer(member_node)
            && let Some(type_text) = self.function_expression_public_return_type_text(initializer)
        {
            return Some(type_text);
        }
        self.object_member_public_type_text(member_idx)
    }

    fn call_argument_public_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        self.primitive_literal_argument_widened_type_text(arg_idx)
            .or_else(|| self.object_literal_public_type_text(arg_idx))
            .or_else(|| self.function_expression_public_return_type_text(arg_idx))
            .or_else(|| self.call_argument_type_text_for_substitution(arg_idx, None))
    }

    fn function_expression_public_return_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(arg_node)?;
        let return_expr = if self
            .arena
            .get(func.body)
            .is_some_and(|node| node.kind == syntax_kind_ext::BLOCK)
        {
            self.function_body_single_return_expression(func.body)?
        } else {
            func.body
        };
        self.call_argument_public_type_text(return_expr)
            .map(|text| Self::widen_public_literal_type_text(&text))
    }

    fn object_literal_public_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(arg_node)?;
        let mut lines = Vec::new();
        for member_idx in object.elements.nodes.iter().copied() {
            let member_node = self.arena.get(member_idx)?;
            if let Some(method_text) = self.object_method_public_signature_text(member_node) {
                lines.push(method_text);
                continue;
            }
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name_text = self.object_literal_member_name_text(name_idx)?;
            if !Self::is_simple_identifier_text(&name_text) {
                return None;
            }
            let value_text = self.object_member_public_type_text(member_idx)?;
            lines.push(format!("    {name_text}: {value_text};"));
        }
        (!lines.is_empty()).then(|| format!("{{\n{}\n}}", lines.join("\n")))
    }

    fn object_method_public_signature_text(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        let method = self.arena.get_method_decl(member_node)?;
        let name = self.object_literal_member_name_text(method.name)?;
        if !Self::is_simple_identifier_text(&name) {
            return None;
        }
        let params = method
            .parameters
            .nodes
            .iter()
            .copied()
            .map(|param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name = self.get_identifier_text(param.name)?;
                let type_text = self
                    .emit_type_node_text(param.type_annotation)
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            })
            .collect::<Option<Vec<_>>>()?;
        let return_text = self
            .emit_type_node_text(method.type_annotation)
            .or_else(|| self.function_body_preferred_return_type_text(method.body))
            .or_else(|| self.infer_fallback_type_text_at(method.body, 0))
            .map(|text| Self::widen_public_literal_type_text(&text))
            .unwrap_or_else(|| "void".to_string());
        Some(format!("    {name}({}): {return_text};", params.join(", ")))
    }

    fn primitive_literal_argument_widened_type_text(&self, arg_idx: NodeIndex) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        match arg_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            _ => None,
        }
    }

    fn widen_public_literal_type_text(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return Self::widen_public_object_literal_type_text(trimmed);
        }
        if trimmed.parse::<f64>().is_ok() {
            return "number".to_string();
        }
        if trimmed == "true" || trimmed == "false" {
            return "boolean".to_string();
        }
        if trimmed.len() >= 2
            && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
        {
            return "string".to_string();
        }
        trimmed.to_string()
    }

    fn widen_public_object_literal_type_text(type_text: &str) -> String {
        let mut output = String::with_capacity(type_text.len());
        for line in type_text.lines() {
            let trimmed = line.trim();
            if let Some((name, rest)) = trimmed.split_once(':') {
                let value = rest.trim().trim_end_matches(';').trim();
                let widened = Self::widen_public_literal_type_text(value);
                if widened != value {
                    let indent = line
                        .get(..line.len().saturating_sub(line.trim_start().len()))
                        .unwrap_or("");
                    output.push_str(indent);
                    output.push_str(name.trim());
                    output.push_str(": ");
                    output.push_str(&widened);
                    output.push(';');
                    output.push('\n');
                    continue;
                }
            }
            output.push_str(line);
            output.push('\n');
        }
        output.trim_end().to_string()
    }

    fn mapped_type_param_name(
        name: &str,
        type_param_names: &[String],
        aliases: &[(String, String)],
    ) -> Option<String> {
        let trimmed = name.trim();
        if type_param_names.iter().any(|param| param == trimmed) {
            return Some(trimmed.to_string());
        }
        aliases
            .iter()
            .find_map(|(alias, mapped)| (alias == trimmed).then(|| mapped.clone()))
    }

    fn replace_or_push_substitution(
        substitutions: &mut Vec<(String, String)>,
        type_param_name: String,
        value_text: String,
    ) {
        if let Some((_, existing_value)) = substitutions
            .iter_mut()
            .find(|(existing, _)| existing == &type_param_name)
        {
            *existing_value = value_text;
        } else {
            substitutions.push((type_param_name, value_text));
        }
    }

    fn type_reference_application_parts(type_text: &str) -> Option<(&str, Vec<String>)> {
        let trimmed = type_text.trim();
        let open = trimmed.find('<')?;
        let name = trimmed.get(..open)?.trim();
        let inner = trimmed.get(open + 1..)?.trim().strip_suffix('>')?.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        Some((
            name,
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(str::trim)
                .map(str::to_string)
                .collect(),
        ))
    }

    fn find_type_alias_decl_in_arena(&self, arena: &NodeArena, name: &str) -> Option<NodeIndex> {
        let source_file = self.arena_source_file(arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = arena.get(stmt_idx)?;
            let Some(alias) = arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(arena, alias.name)
                .as_deref()
                == Some(name)
            {
                return Some(stmt_idx);
            }
        }
        None
    }

    fn type_text_without_this_type_markers(type_text: &str) -> String {
        let mut text = type_text.to_string();
        while let Some(start) = text.find("ThisType<") {
            let mut depth = 0usize;
            let mut end = None;
            for (offset, ch) in text[start + "ThisType".len()..].char_indices() {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            end = Some(start + "ThisType".len() + offset + ch.len_utf8());
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                break;
            };
            text.replace_range(start..end, "");
        }
        text
    }
}

fn identifier_text(source_arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    source_arena
        .get(idx)
        .and_then(|node| source_arena.get_identifier(node))
        .map(|ident| ident.escaped_text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    #[test]
    fn mapped_accessor_object_argument_infers_public_value_map() {
        let mut parser = ParserState::new(
            "accessor-map.ts".to_string(),
            r#"
type Accessor<V> = {
    get?(): V;
    set?(value: V): void;
};
type AccessorBag<S> = { [Key in keyof S]: (() => S[Key]) | Accessor<S[Key]> };
type Options<S> = {
    computed?: AccessorBag<S>;
};
let arg = {
    computed: {
        total(): number {
            return 1;
        },
        label: {
            get() {
                return "ready";
            },
            set(value: string) {
            }
        }
    }
};
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let options_type = emitter
            .find_type_alias_type_node_in_arena(arena, "Options")
            .expect("options alias type");
        let arg_idx = arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let node_idx = NodeIndex(idx as u32);
                (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && emitter
                        .object_literal_member_by_name(node_idx, "computed")
                        .is_some())
                .then_some(node_idx)
            })
            .expect("argument object literal");
        let computed_member_idx = emitter
            .object_literal_member_by_name(arg_idx, "computed")
            .expect("computed member");
        assert_eq!(
            emitter.object_literal_property_value_map_type_text(computed_member_idx),
            Some("{\n    total: number;\n    label: string;\n}".to_string())
        );
        let mut substitutions = Vec::new();
        emitter.infer_object_argument_substitutions_from_type_node(
            arena,
            options_type,
            arg_idx,
            &["S".to_string()],
            &[],
            &mut substitutions,
            0,
        );

        assert_eq!(
            substitutions,
            vec![(
                "S".to_string(),
                "{\n    total: number;\n    label: string;\n}".to_string()
            )]
        );
    }
}
