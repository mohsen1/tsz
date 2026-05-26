//! Declaration emit helpers for generic calls through mapped utility surfaces.

use super::super::DeclarationEmitter;
use tsz_binder::SymbolId;
use tsz_parser::parser::node::{FunctionData, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn generic_call_pick_mapped_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        let arguments = call.arguments.as_ref()?;

        if self.pick_call_function_expression_has_type_parameters(call.expression) {
            let callee_idx = self.skip_parenthesized_expression(call.expression)?;
            let callee_node = self.arena.get(callee_idx)?;
            let func = self.arena.get_function(callee_node)?;
            return self
                .generic_call_pick_mapped_type_text_for_function(self.arena, func, arguments);
        }

        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            self.generic_call_pick_mapped_type_text_for_function(source_arena, func, arguments)
        })
    }

    pub(in crate::declaration_emitter) fn generic_call_constrained_mapped_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let call = self.arena.get_call_expr(expr_node)?;
        if call
            .arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let func = callable_function_from_symbol_decl(source_arena, decl_idx)?;
            self.generic_call_constrained_mapped_return_type_text_for_function(
                expr_idx,
                source_arena,
                func,
            )
        })
    }

    fn generic_call_pick_mapped_type_text_for_function(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        arguments: &NodeList,
    ) -> Option<String> {
        if let Some((return_object_idx, return_key_idx)) =
            self.pick_type_reference_args(source_arena, func.type_annotation)
        {
            let object_text = self.type_parameter_argument_type_text(
                source_arena,
                func,
                arguments,
                return_object_idx,
            )?;
            let key_text = self.type_parameter_argument_type_text(
                source_arena,
                func,
                arguments,
                return_key_idx,
            )?;
            if object_text == "any" {
                return Some(format!("Pick<any, {key_text}>"));
            }
        }

        let return_text = self
            .emit_type_node_text_from_arena(source_arena, func.type_annotation)
            .or_else(|| self.source_slice_from_arena(source_arena, func.type_annotation))?
            .trim()
            .to_string();

        func.parameters
            .nodes
            .iter()
            .copied()
            .zip(arguments.nodes.iter().copied())
            .find_map(|(param_idx, arg_idx)| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                let pick = self.pick_type_reference_in_type(source_arena, param.type_annotation)?;
                let object_text = self
                    .emit_type_node_text_from_arena(source_arena, pick.object_type)
                    .or_else(|| self.source_slice_from_arena(source_arena, pick.object_type))?
                    .trim()
                    .to_string();
                let key_text = self
                    .emit_type_node_text_from_arena(source_arena, pick.key_type)
                    .or_else(|| self.source_slice_from_arena(source_arena, pick.key_type))?
                    .trim()
                    .to_string();

                if return_text == key_text {
                    return self.object_literal_key_union_type_text(arg_idx);
                }

                if return_text != object_text {
                    return None;
                }

                let shape_text = self.object_literal_pick_argument_shape_type_text(
                    arg_idx,
                    pick.requires_single_property_unwrap,
                    0,
                )?;
                if source_arena
                    .get(pick.object_type)
                    .is_some_and(|node| node.kind == syntax_kind_ext::INTERSECTION_TYPE)
                {
                    let arm_count = source_arena
                        .get(pick.object_type)
                        .and_then(|node| source_arena.get_composite_type(node))
                        .map(|composite| composite.types.nodes.len())
                        .unwrap_or(1);
                    return Some(vec![shape_text; arm_count].join(" & "));
                }
                Some(shape_text)
            })
    }

    fn generic_call_constrained_mapped_return_type_text_for_function(
        &self,
        expr_idx: NodeIndex,
        source_arena: &NodeArena,
        func: &FunctionData,
    ) -> Option<String> {
        let mapped = mapped_type_from_annotation(source_arena, func.type_annotation)?;
        let type_params = func.type_parameters.as_ref()?;
        let mapped_param = source_arena
            .get(mapped.type_parameter)
            .and_then(|node| source_arena.get_type_parameter(node))?;
        let type_op = source_arena
            .get(mapped_param.constraint)
            .and_then(|node| source_arena.get_type_operator(node))?;
        if type_op.operator != SyntaxKind::KeyOfKeyword as u16 {
            return None;
        }
        let source_param_name = type_reference_identifier_name(source_arena, type_op.type_node)?;
        let source_type_idx = type_params.nodes.iter().copied().find_map(|param_idx| {
            let param = source_arena
                .get(param_idx)
                .and_then(|node| source_arena.get_type_parameter(node))?;
            if identifier_text(source_arena, param.name).as_deref()
                == Some(source_param_name.as_str())
            {
                param
                    .default
                    .into_option()
                    .or_else(|| param.constraint.into_option())
            } else {
                None
            }
        })?;
        if let Some(primitive_text) =
            self.primitive_keyword_type_text(source_arena, source_type_idx)
        {
            return Some(primitive_text);
        }

        let template_text = self
            .emit_type_node_text_from_arena(source_arena, mapped.type_node)
            .or_else(|| self.source_slice_from_arena(source_arena, mapped.type_node))?
            .trim()
            .to_string();
        if template_text.is_empty()
            || identifier_text(source_arena, mapped_param.name)
                .is_some_and(|name| Self::contains_whole_word_in_text(&template_text, &name))
            || Self::contains_whole_word_in_text(&template_text, &source_param_name)
        {
            return None;
        }
        self.mapped_constraint_member_object_type_text(
            source_arena,
            source_type_idx,
            &template_text,
        )
        .or_else(|| {
            let type_id = self.get_node_type_or_names(&[expr_idx])?;
            let expanded = self.print_type_id_expanded_for_inferred_declaration(type_id);
            (!expanded.is_empty() && !matches!(expanded.as_str(), "any" | "unknown"))
                .then_some(Self::strip_synthetic_anonymous_object_members(&expanded))
        })
    }

    fn primitive_keyword_type_text(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let node = source_arena.get(type_idx)?;
        let type_name_idx = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            source_arena.get_type_ref(node)?.type_name
        } else {
            type_idx
        };
        let primitive = match identifier_text(source_arena, type_name_idx)?.as_str() {
            "string" => "string",
            "number" => "number",
            "boolean" => "boolean",
            "bigint" => "bigint",
            "symbol" => "symbol",
            _ => return None,
        };
        Some(primitive.to_string())
    }

    fn mapped_constraint_member_object_type_text(
        &self,
        source_arena: &NodeArena,
        source_type_idx: NodeIndex,
        template_text: &str,
    ) -> Option<String> {
        let sym_id = self
            .type_reference_symbol_from_arena(source_arena, source_type_idx)
            .or_else(|| {
                let name = type_reference_identifier_name(source_arena, source_type_idx)?;
                self.binder?.get_global_type(&name)
            })?;
        let member_texts = self.mapped_constraint_member_type_texts(sym_id, template_text)?;
        let member_indent = "    ".to_string();
        let formatted_members = member_texts
            .iter()
            .map(|member| Self::format_object_member_entry(&member_indent, member))
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!("{{\n{formatted_members}\n}}"))
    }

    fn mapped_constraint_member_type_texts(
        &self,
        sym_id: SymbolId,
        template_text: &str,
    ) -> Option<Vec<String>> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let mut member_texts = Vec::new();
        for decl_idx in symbol.declarations.iter().copied() {
            if self.arena.get(decl_idx).is_some() {
                self.collect_mapped_constraint_members_from_decl(
                    self.arena,
                    decl_idx,
                    template_text,
                    &mut member_texts,
                );
            }
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    self.collect_mapped_constraint_members_from_decl(
                        arena.as_ref(),
                        decl_idx,
                        template_text,
                        &mut member_texts,
                    );
                }
            }
            if let Some(arena) = binder.symbol_arenas.get(&sym_id) {
                self.collect_mapped_constraint_members_from_decl(
                    arena.as_ref(),
                    decl_idx,
                    template_text,
                    &mut member_texts,
                );
            }
            if let Some(arena) = self.global_symbol_arenas.get(&sym_id) {
                self.collect_mapped_constraint_members_from_decl(
                    arena.as_ref(),
                    decl_idx,
                    template_text,
                    &mut member_texts,
                );
            }
        }
        (!member_texts.is_empty()).then_some(member_texts)
    }

    fn collect_mapped_constraint_members_from_decl(
        &self,
        decl_arena: &NodeArena,
        decl_idx: NodeIndex,
        template_text: &str,
        member_texts: &mut Vec<String>,
    ) {
        let Some(decl_node) = decl_arena.get(decl_idx) else {
            return;
        };
        let Some(members) = decl_arena
            .get_interface(decl_node)
            .map(|iface| iface.members.nodes.as_slice())
            .or_else(|| {
                decl_arena
                    .get_class(decl_node)
                    .map(|class| class.members.nodes.as_slice())
            })
        else {
            return;
        };
        for &member_idx in members {
            let Some(member_node) = decl_arena.get(member_idx) else {
                continue;
            };
            let name_idx = decl_arena
                .get_signature(member_node)
                .map(|signature| signature.name)
                .or_else(|| {
                    decl_arena
                        .get_method_decl(member_node)
                        .map(|method| method.name)
                })
                .or_else(|| {
                    decl_arena
                        .get_property_decl(member_node)
                        .map(|prop| prop.name)
                });
            let Some(name) =
                name_idx.and_then(|idx| self.property_name_text_from_arena(decl_arena, idx))
            else {
                continue;
            };
            let member_text = Self::format_object_member_type_text(&name, template_text, 1);
            if !member_texts.contains(&member_text) {
                member_texts.push(member_text);
            }
        }
    }

    fn type_parameter_argument_type_text(
        &self,
        source_arena: &NodeArena,
        func: &FunctionData,
        arguments: &NodeList,
        type_param_idx: NodeIndex,
    ) -> Option<String> {
        let type_param_name = type_reference_identifier_name(source_arena, type_param_idx)?;
        func.parameters
            .nodes
            .iter()
            .copied()
            .zip(arguments.nodes.iter().copied())
            .find_map(|(param_idx, arg_idx)| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                if type_reference_identifier_name(source_arena, param.type_annotation).as_deref()
                    == Some(type_param_name.as_str())
                {
                    return self
                        .preferred_expression_type_text(arg_idx)
                        .or_else(|| self.infer_fallback_type_text_at(arg_idx, 0));
                }
                if array_type_element_identifier_name(source_arena, param.type_annotation)
                    .as_deref()
                    == Some(type_param_name.as_str())
                {
                    return self.array_literal_const_union_type_text(arg_idx);
                }
                None
            })
    }

    fn object_literal_pick_argument_shape_type_text(
        &self,
        object_idx: NodeIndex,
        unwrap_single_property_object: bool,
        depth: u32,
    ) -> Option<String> {
        if !unwrap_single_property_object {
            return self.infer_object_literal_type_text_at(object_idx, depth);
        }

        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name = self.object_literal_member_name_text(name_idx)?;
            if name.is_empty() || name == ":" {
                return None;
            }
            let initializer = self.object_literal_member_initializer(member_node)?;
            let value_idx = self.single_property_object_literal_value(initializer)?;
            let type_text = self
                .infer_fallback_type_text_at(value_idx, depth + 1)
                .or_else(|| self.preferred_expression_type_text(value_idx))?;
            members.push(Self::format_object_member_type_text(
                &name,
                &type_text,
                depth + 1,
            ));
        }

        if members.is_empty() {
            return None;
        }
        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let formatted_members = members
            .iter()
            .map(|member| Self::format_object_member_entry(&member_indent, member))
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!("{{\n{formatted_members}\n{closing_indent}}}"))
    }

    fn single_property_object_literal_value(&self, object_idx: NodeIndex) -> Option<NodeIndex> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        let [member_idx] = object.elements.nodes.as_slice() else {
            return None;
        };
        let member_node = self.arena.get(*member_idx)?;
        if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
            return None;
        }
        self.object_literal_member_initializer(member_node)
    }

    fn object_literal_key_union_type_text(&self, object_idx: NodeIndex) -> Option<String> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        let mut keys = Vec::new();
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                return None;
            }
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name = self.object_literal_member_name_text(name_idx)?;
            if name.is_empty() || name == ":" {
                return None;
            }
            keys.push(format!("\"{name}\""));
        }
        (!keys.is_empty()).then(|| keys.join(" | "))
    }

    fn array_literal_const_union_type_text(&self, array_idx: NodeIndex) -> Option<String> {
        let array_idx = self.skip_parenthesized_expression(array_idx)?;
        let array_node = self.arena.get(array_idx)?;
        if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = self.arena.get_literal_expr(array_node)?;
        let mut elements = Vec::new();
        for &element_idx in &array.elements.nodes {
            elements.push(self.const_literal_initializer_text(element_idx)?);
        }
        (!elements.is_empty()).then(|| elements.join(" | "))
    }

    fn pick_call_function_expression_has_type_parameters(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        self.arena
            .get_function(expr_node)
            .and_then(|func| func.type_parameters.as_ref())
            .is_some_and(|params| !params.nodes.is_empty())
    }

    fn pick_type_reference_args(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let type_node = source_arena.get(type_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = source_arena.get_type_ref(type_node)?;
        let type_args = type_ref.type_arguments.as_ref()?;
        let [object_type, key_type] = type_args.nodes.as_slice() else {
            return None;
        };
        let sym_id = self.type_reference_symbol_from_arena(source_arena, type_ref.type_name)?;
        self.symbol_declares_pick_like_alias(sym_id)
            .then_some((*object_type, *key_type))
    }

    fn pick_type_reference_in_type(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<PickTypeReference> {
        self.pick_type_reference_in_type_inner(source_arena, type_idx, false)
    }

    fn pick_type_reference_in_type_inner(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
        requires_single_property_unwrap: bool,
    ) -> Option<PickTypeReference> {
        let type_node = source_arena.get(type_idx)?;
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some((object_type, key_type)) =
                    self.pick_type_reference_args(source_arena, type_idx)
                {
                    return Some(PickTypeReference {
                        object_type,
                        key_type,
                        requires_single_property_unwrap,
                    });
                }
                let type_ref = source_arena.get_type_ref(type_node)?;
                type_ref
                    .type_arguments
                    .as_ref()?
                    .nodes
                    .iter()
                    .copied()
                    .find_map(|arg_idx| {
                        self.pick_type_reference_in_type_inner(source_arena, arg_idx, true)
                    })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => source_arena
                .get_wrapped_type(type_node)
                .and_then(|wrapped| {
                    self.pick_type_reference_in_type_inner(
                        source_arena,
                        wrapped.type_node,
                        requires_single_property_unwrap,
                    )
                }),
            _ => None,
        }
    }

    fn type_reference_symbol_from_arena(
        &self,
        source_arena: &NodeArena,
        type_or_name_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let type_name_idx = source_arena.get(type_or_name_idx).and_then(|node| {
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                source_arena
                    .get_type_ref(node)
                    .map(|type_ref| type_ref.type_name)
            } else {
                Some(type_or_name_idx)
            }
        })?;
        if std::ptr::eq(source_arena, self.arena)
            && let Some(sym_id) = binder
                .get_node_symbol(type_name_idx)
                .or_else(|| binder.get_node_symbol(type_or_name_idx))
        {
            return Some(sym_id);
        }
        let name = identifier_text(source_arena, type_name_idx)?;
        if std::ptr::eq(source_arena, self.arena)
            && let Some(sym_id) = self.resolve_identifier_symbol(type_name_idx, &name)
        {
            return Some(sym_id);
        }
        binder.get_global_type(&name)
    }

    fn symbol_declares_pick_like_alias(&self, sym_id: SymbolId) -> bool {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let alias_node = source_arena.get(decl_idx)?;
            let alias = source_arena.get_type_alias(alias_node)?;
            type_alias_is_pick_like(source_arena, alias).then_some(())
        })
        .is_some()
    }
}

#[derive(Clone, Copy)]
struct PickTypeReference {
    object_type: NodeIndex,
    key_type: NodeIndex,
    requires_single_property_unwrap: bool,
}

fn array_type_element_identifier_name(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
) -> Option<String> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind != syntax_kind_ext::ARRAY_TYPE {
        return None;
    }
    let array = source_arena.get_array_type(type_node)?;
    type_reference_identifier_name(source_arena, array.element_type)
}

fn type_alias_is_pick_like(source_arena: &NodeArena, alias: &TypeAliasData) -> bool {
    let Some(type_params) = alias.type_parameters.as_ref() else {
        return false;
    };
    let [object_param_idx, key_param_idx] = type_params.nodes.as_slice() else {
        return false;
    };
    let Some(object_param_name) = type_parameter_name(source_arena, *object_param_idx) else {
        return false;
    };
    let Some(key_param_name) = type_parameter_name(source_arena, *key_param_idx) else {
        return false;
    };

    let Some(type_node) = source_arena.get(alias.type_node) else {
        return false;
    };
    if type_node.kind != syntax_kind_ext::MAPPED_TYPE {
        return false;
    }
    let Some(mapped) = source_arena.get_mapped_type(type_node) else {
        return false;
    };
    let Some(mapped_param) = source_arena
        .get(mapped.type_parameter)
        .and_then(|node| source_arena.get_type_parameter(node))
    else {
        return false;
    };
    let Some(mapped_param_name) = identifier_text(source_arena, mapped_param.name) else {
        return false;
    };
    if type_reference_identifier_name(source_arena, mapped_param.constraint).as_deref()
        != Some(key_param_name.as_str())
    {
        return false;
    }

    let Some(template_node) = source_arena.get(mapped.type_node) else {
        return false;
    };
    let Some(indexed) = source_arena.get_indexed_access_type(template_node) else {
        return false;
    };
    type_reference_identifier_name(source_arena, indexed.object_type).as_deref()
        == Some(object_param_name.as_str())
        && type_reference_identifier_name(source_arena, indexed.index_type).as_deref()
            == Some(mapped_param_name.as_str())
}

fn type_parameter_name(source_arena: &NodeArena, type_param_idx: NodeIndex) -> Option<String> {
    source_arena
        .get(type_param_idx)
        .and_then(|node| source_arena.get_type_parameter(node))
        .and_then(|param| identifier_text(source_arena, param.name))
}

fn mapped_type_from_annotation(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
) -> Option<&tsz_parser::parser::node::MappedTypeData> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind == syntax_kind_ext::MAPPED_TYPE {
        return source_arena.get_mapped_type(type_node);
    }
    if type_node.kind == syntax_kind_ext::TYPE_LITERAL {
        let literal = source_arena.get_type_literal(type_node)?;
        let [member_idx] = literal.members.nodes.as_slice() else {
            return None;
        };
        let member_node = source_arena.get(*member_idx)?;
        if member_node.kind == syntax_kind_ext::MAPPED_TYPE {
            return source_arena.get_mapped_type(member_node);
        }
    }
    None
}

fn type_reference_identifier_name(source_arena: &NodeArena, type_idx: NodeIndex) -> Option<String> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind == SyntaxKind::Identifier as u16 {
        return identifier_text(source_arena, type_idx);
    }
    let type_ref = source_arena.get_type_ref(type_node)?;
    identifier_text(source_arena, type_ref.type_name)
}

fn identifier_text(source_arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    source_arena
        .get(idx)
        .and_then(|node| source_arena.get_identifier(node))
        .map(|ident| ident.escaped_text.clone())
}

fn callable_function_from_symbol_decl(
    source_arena: &NodeArena,
    decl_idx: NodeIndex,
) -> Option<&FunctionData> {
    if let Some(func) = source_arena
        .get(decl_idx)
        .and_then(|node| source_arena.get_function(node))
    {
        return Some(func);
    }

    let mut current = decl_idx;
    for _ in 0..8 {
        let node = source_arena.get(current)?;
        if let Some(var_decl) = source_arena.get_variable_declaration(node) {
            let initializer_node = source_arena.get(var_decl.initializer)?;
            if initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return source_arena.get_function(initializer_node);
            }
        }
        current = source_arena.parent_of(current)?;
    }

    None
}
