//! Source-file shape lowering helpers for direct cross-file queries.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    pub(super) fn cross_file_interface_declarations<'b>(
        &self,
        sym_id: SymbolId,
        delegate_binder: &'b BinderState,
        fallback_arena: &'b NodeArena,
    ) -> Option<Vec<(NodeIndex, &'b NodeArena)>> {
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            return None;
        }

        let mut declarations = Vec::new();
        for decl_idx in symbol.declarations.iter().copied() {
            let mut found = false;
            if let Some(arenas) = delegate_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas.iter() {
                    if arena
                        .get(decl_idx)
                        .and_then(|node| arena.get_interface(node))
                        .is_some()
                    {
                        declarations.push((decl_idx, arena.as_ref()));
                        found = true;
                    }
                }
            }

            if !found
                && fallback_arena
                    .get(decl_idx)
                    .and_then(|node| fallback_arena.get_interface(node))
                    .is_some()
            {
                declarations.push((decl_idx, fallback_arena));
            }
        }

        (!declarations.is_empty()).then_some(declarations)
    }

    pub(super) fn interface_declarations_have_heritage(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            if interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|clauses| !clauses.nodes.is_empty())
            {
                return true;
            }

            false
        })
    }

    pub(super) fn interface_declarations_have_computed_names(
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> bool {
        declarations.iter().any(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                let name_idx = arena
                    .get_signature(member_node)
                    .map(|signature| signature.name)
                    .or_else(|| {
                        arena
                            .get_accessor(member_node)
                            .map(|accessor| accessor.name)
                    });
                name_idx
                    .and_then(|idx| arena.get(idx))
                    .is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    })
            })
        })
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_is_scope_independent(
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::AnyKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UnknownKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NeverKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::VoidKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::UndefinedKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NullKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BooleanKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::NumberKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::StringKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::BigIntKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::SymbolKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::ObjectKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => true,
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => true,
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };
                    match name {
                        "any" | "unknown" | "never" | "void" | "undefined" | "null" | "boolean"
                        | "number" | "string" | "bigint" | "symbol" | "object" => type_ref
                            .type_arguments
                            .as_ref()
                            .is_none_or(|args| args.nodes.is_empty()),
                        "Array" | "ReadonlyArray" => {
                            type_ref.type_arguments.as_ref().is_some_and(|args| {
                                args.nodes.len() == 1
                                    && Self::source_file_type_node_is_scope_independent(
                                        arena,
                                        args.nodes[0],
                                    )
                            })
                        }
                        _ => false,
                    }
                })
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => arena.get_literal_type(node).is_some(),
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_scope_independent(arena, member)
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_scope_independent(arena, array.element_type)
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_scope_independent(arena, element)
                    })
                })
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                arena.get_named_tuple_member(node).is_some_and(|member| {
                    Self::source_file_type_node_is_scope_independent(arena, member.type_node)
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_scope_independent(arena, wrapped.type_node)
                })
            }
            _ => false,
        }
    }

    pub(super) fn source_file_type_node_is_explicit_unknown(
        arena: &NodeArena,
        mut node_idx: NodeIndex,
    ) -> bool {
        for _ in 0..10 {
            let Some(node) = arena.get(node_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = arena.get_wrapped_type(node)
            {
                node_idx = wrapped.type_node;
                continue;
            }
            if node.kind == tsz_scanner::SyntaxKind::UnknownKeyword as u16 {
                return true;
            }
            return node.kind == syntax_kind_ext::TYPE_REFERENCE
                && arena.get_type_ref(node).is_some_and(|type_ref| {
                    type_ref
                        .type_arguments
                        .as_ref()
                        .is_none_or(|args| args.nodes.is_empty())
                        && arena
                            .get(type_ref.type_name)
                            .and_then(|name_node| arena.get_identifier(name_node))
                            .is_some_and(|ident| ident.escaped_text == "unknown")
                });
        }
        false
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_is_option_bag_lowerable<
        'b,
    >(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        node_idx: NodeIndex,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };

                    if matches!(name, "Array" | "ReadonlyArray") {
                        return type_ref.type_arguments.as_ref().is_some_and(|args| {
                            args.nodes.len() == 1
                                && Self::source_file_type_node_is_option_bag_lowerable(
                                    arena,
                                    delegate_binder,
                                    args.nodes[0],
                                    seen_type_names,
                                )
                        });
                    }

                    if type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty())
                    {
                        return false;
                    }

                    Self::source_file_type_reference_targets_option_bag_lowerable_declaration(
                        arena,
                        delegate_binder,
                        name,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_option_bag_lowerable(
                            arena,
                            delegate_binder,
                            member,
                            seen_type_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        array.element_type,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_option_bag_lowerable(
                            arena,
                            delegate_binder,
                            element,
                            seen_type_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                arena.get_named_tuple_member(node).is_some_and(|member| {
                    Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        member.type_node,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        wrapped.type_node,
                        seen_type_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    operator.operator == tsz_scanner::SyntaxKind::ReadonlyKeyword as u16
                        && Self::source_file_type_node_is_option_bag_lowerable(
                            arena,
                            delegate_binder,
                            operator.type_node,
                            seen_type_names,
                        )
                })
            }
            _ => false,
        }
    }

    pub(super) fn source_file_type_reference_targets_option_bag_lowerable_declaration<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        name: &'b str,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        if seen_type_names.contains(&name) {
            return false;
        }
        let Some(sym_id) = delegate_binder.file_locals.get(name) else {
            return false;
        };
        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            return false;
        };
        let disallowed_flags = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & disallowed_flags != 0 || symbol.declarations.len() != 1 {
            return false;
        }

        let decl_idx = symbol.declarations[0];
        if !Self::lib_declaration_name_matches(arena, decl_idx, name) {
            return false;
        }
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };

        if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            seen_type_names.push(name);
            let result = arena.get_type_alias(decl_node).is_some_and(|type_alias| {
                type_alias
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
                    && !Self::source_file_type_node_contains_disallowed_type_query(
                        arena,
                        delegate_binder,
                        type_alias.type_node,
                    )
                    && Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        type_alias.type_node,
                        seen_type_names,
                    )
            });
            seen_type_names.pop();
            result
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            arena.get_interface(decl_node).is_some()
                && Self::source_file_interface_declarations_are_direct_lowerable_with_seen(
                    &[(decl_idx, arena)],
                    delegate_binder,
                    seen_type_names,
                )
        } else {
            false
        }
    }

    pub(in crate::state_domain::type_analysis) fn source_file_local_name_def_id_for_lowering(
        &self,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        type_name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        let sym_id = delegate_binder.file_locals.get(type_name)?;
        let symbol = delegate_binder.get_symbol(sym_id)?;
        let allowed_flags = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        let disallowed_flags = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & allowed_flags == 0 || symbol.flags & disallowed_flags != 0 {
            return None;
        }
        if symbol
            .declarations
            .iter()
            .any(|&decl_idx| Self::lib_declaration_name_matches(symbol_arena, decl_idx, type_name))
        {
            Some(self.ctx.get_or_create_def_id(sym_id))
        } else {
            None
        }
    }

    pub(in crate::state_domain::type_analysis) fn source_file_global_name_def_id_for_lowering(
        &self,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        type_name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        if !self.source_file_global_type_is_direct_lowerable(delegate_binder, type_name) {
            return None;
        }
        if let Some(sym_id) = delegate_binder.file_locals.get(type_name)
            && !Self::source_file_local_symbol_can_fall_back_to_global_type(
                symbol_arena,
                delegate_binder,
                sym_id,
            )
        {
            return None;
        }
        self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
            .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_is_generic_scope_independent(
        arena: &NodeArena,
        node_idx: NodeIndex,
        type_param_names: &[String],
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                arena.get_type_ref(node).is_some_and(|type_ref| {
                    let Some(name) = arena
                        .get(type_ref.type_name)
                        .and_then(|name_node| arena.get_identifier(name_node))
                        .map(|ident| ident.escaped_text.as_str())
                    else {
                        return false;
                    };
                    if type_param_names.iter().any(|param| param == name) {
                        return type_ref
                            .type_arguments
                            .as_ref()
                            .is_none_or(|args| args.nodes.is_empty());
                    }
                    matches!(name, "Array" | "ReadonlyArray")
                        && type_ref.type_arguments.as_ref().is_some_and(|args| {
                            args.nodes.len() == 1
                                && Self::source_file_type_node_is_generic_scope_independent(
                                    arena,
                                    args.nodes[0],
                                    type_param_names,
                                )
                        })
                })
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                arena.get_conditional_type(node).is_some_and(|conditional| {
                    let mut true_branch_names = type_param_names.to_vec();
                    Self::collect_infer_type_param_names(
                        arena,
                        conditional.extends_type,
                        &mut true_branch_names,
                    );
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.check_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.extends_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.true_type,
                        &true_branch_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        conditional.false_type,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                arena.get_infer_type(node).is_some_and(|infer_type| {
                    let Some(type_param_node) = arena.get(infer_type.type_parameter) else {
                        return false;
                    };
                    let Some(type_param) = arena.get_type_parameter(type_param_node) else {
                        return false;
                    };
                    type_param.constraint.is_none() && type_param.default.is_none()
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        array.element_type,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_generic_scope_independent(
                            arena,
                            element,
                            type_param_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                arena.get_named_tuple_member(node).is_some_and(|member| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        member.type_node,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_generic_scope_independent(
                            arena,
                            member,
                            type_param_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        wrapped.type_node,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        operator.type_node,
                        type_param_names,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        indexed.object_type,
                        type_param_names,
                    ) && Self::source_file_type_node_is_generic_scope_independent(
                        arena,
                        indexed.index_type,
                        type_param_names,
                    )
                })
            }
            _ => false,
        }
    }

    pub(super) fn type_alias_type_param_names(
        arena: &NodeArena,
        type_alias: &TypeAliasData,
    ) -> Vec<String> {
        type_alias
            .type_parameters
            .as_ref()
            .into_iter()
            .flat_map(|params| params.nodes.iter().copied())
            .filter_map(|param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                let name_node = arena.get(param.name)?;
                let ident = arena.get_identifier(name_node)?;
                Some(ident.escaped_text.to_string())
            })
            .collect()
    }

    pub(in crate::state_domain::type_analysis) fn collect_infer_type_param_names(
        arena: &NodeArena,
        root: NodeIndex,
        names: &mut Vec<String>,
    ) {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE
                && let Some(infer_type) = arena.get_infer_type(node)
                && let Some(type_param_node) = arena.get(infer_type.type_parameter)
                && let Some(type_param) = arena.get_type_parameter(type_param_node)
                && let Some(name_node) = arena.get(type_param.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && !names.iter().any(|name| name == &ident.escaped_text)
            {
                names.push(ident.escaped_text.to_string());
            }
            stack.extend(arena.get_children(idx));
        }
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_contains_kind(
        arena: &NodeArena,
        root: NodeIndex,
        kind: u16,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if arena.get(idx).is_some_and(|node| node.kind == kind) {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_contains_disallowed_type_query(
        arena: &NodeArena,
        binder: &BinderState,
        root: NodeIndex,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_QUERY
                && !Self::source_file_type_query_is_well_known_global_symbol_property(
                    arena, binder, idx,
                )
            {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_query_is_well_known_global_symbol_property(
        arena: &NodeArena,
        binder: &BinderState,
        type_query_idx: NodeIndex,
    ) -> bool {
        let Some(type_query_node) = arena.get(type_query_idx) else {
            return false;
        };
        let Some(type_query) = arena.get_type_query(type_query_node) else {
            return false;
        };
        let Some(expr_node) = arena.get(type_query.expr_name) else {
            return false;
        };
        let (base_idx, member_idx) =
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(access) = arena.get_access_expr(expr_node) else {
                    return false;
                };
                (access.expression, access.name_or_argument)
            } else if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qualified) = arena.get_qualified_name(expr_node) else {
                    return false;
                };
                (qualified.left, qualified.right)
            } else {
                return false;
            };
        let Some(base_ident) = arena
            .get(base_idx)
            .and_then(|base| arena.get_identifier(base))
        else {
            return false;
        };
        if base_ident.escaped_text != "Symbol" {
            return false;
        }
        if let Some(sym_id) = binder.file_locals.get("Symbol")
            && !binder.lib_symbol_ids.contains(&sym_id)
            && binder.get_symbol(sym_id).is_some_and(|symbol| {
                !symbol.declarations.is_empty()
                    && symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS) != 0
            })
        {
            return false;
        }
        arena
            .get(member_idx)
            .and_then(|name| arena.get_identifier(name))
            .is_some()
    }

    pub(in crate::state_domain::type_analysis) fn source_file_type_node_contains_identifier_name(
        arena: &NodeArena,
        root: NodeIndex,
        name: &str,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if arena
                .get(idx)
                .and_then(|node| arena.get_identifier(node))
                .is_some_and(|ident| ident.escaped_text == name)
            {
                return true;
            }
            stack.extend(arena.get_children(idx));
        }
        false
    }

    pub(super) fn external_declaration_body_uses_local_array_shadow(
        arena: &NodeArena,
        delegate_binder: &BinderState,
        root: NodeIndex,
    ) -> bool {
        ["Array", "ReadonlyArray"].iter().any(|name| {
            delegate_binder.file_locals.get(name).is_some()
                && Self::source_file_type_node_contains_identifier_name(arena, root, name)
        })
    }

    pub(super) fn source_file_interface_declarations_are_direct_lowerable_with_seen<'b>(
        declarations: &[(NodeIndex, &'b NodeArena)],
        delegate_binder: &BinderState,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        declarations.iter().all(|(decl_idx, arena)| {
            let Some(node) = arena.get(*decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            let Some(interface_name) = arena
                .get(interface.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                return false;
            };
            if seen_type_names.contains(&interface_name) {
                return false;
            }
            if interface
                .type_parameters
                .as_ref()
                .is_some_and(|params| !params.nodes.is_empty())
            {
                return false;
            }

            seen_type_names.push(interface_name);
            let result = Self::source_file_interface_heritage_is_direct_lowerable(
                arena,
                delegate_binder,
                interface,
                seen_type_names,
            ) && interface.members.nodes.iter().copied().all(|member_idx| {
                Self::source_file_interface_member_is_direct_lowerable(
                    arena,
                    delegate_binder,
                    member_idx,
                    seen_type_names,
                )
            });
            seen_type_names.pop();
            result
        })
    }

    /// Decide whether a single source-file interface member can be lowered on
    /// the direct cross-file path (no child checker).
    ///
    /// The direct path reuses the same `TypeLowering` member collection as the
    /// mature path, so it produces an identical member type *as long as every
    /// type referenced by the member resolves through the option-bag guard*
    /// (primitives, `Array`/`ReadonlyArray`, and direct-lowerable sibling
    /// interfaces/aliases). Anything the guard cannot prove resolvable falls
    /// back to the child-checker path, which preserves correctness.
    ///
    /// Two member shapes qualify:
    /// - Plain property signatures whose annotation is option-bag lowerable.
    /// - Non-generic method signatures whose every parameter annotation and
    ///   return annotation are option-bag lowerable. Optional and rest
    ///   parameters are fine (the shared collector models them identically on
    ///   both paths); `this` parameters and own method type parameters need the
    ///   mature generic/self path and are rejected.
    ///
    /// Call/construct/index signatures, accessors, computed names, and members
    /// with unannotated parameters or return types stay on the child path.
    pub(super) fn source_file_interface_member_is_direct_lowerable<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        member_idx: NodeIndex,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        let Some(member_node) = arena.get(member_idx) else {
            return false;
        };
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                signature
                    .parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
                    && signature
                        .type_parameters
                        .as_ref()
                        .is_none_or(|params| params.nodes.is_empty())
                    && Self::source_file_type_node_is_option_bag_lowerable(
                        arena,
                        delegate_binder,
                        signature.type_annotation,
                        seen_type_names,
                    )
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                // Own method type parameters (`foo<T>(...)`) need the mature
                // generic instantiation path.
                if signature
                    .type_parameters
                    .as_ref()
                    .is_some_and(|params| !params.nodes.is_empty())
                {
                    return false;
                }
                signature.parameters.as_ref().is_none_or(|params| {
                    params.nodes.iter().copied().all(|param_idx| {
                        let Some(param_node) = arena.get(param_idx) else {
                            return false;
                        };
                        let Some(parameter) = arena.get_parameter(param_node) else {
                            return false;
                        };
                        !Self::source_file_parameter_is_this(arena, parameter)
                            && Self::source_file_type_node_is_option_bag_lowerable(
                                arena,
                                delegate_binder,
                                parameter.type_annotation,
                                seen_type_names,
                            )
                    })
                }) && Self::source_file_type_node_is_option_bag_lowerable(
                    arena,
                    delegate_binder,
                    signature.type_annotation,
                    seen_type_names,
                )
            }
            _ => false,
        }
    }

    pub(super) fn source_file_parameter_is_this(
        arena: &NodeArena,
        parameter: &tsz_parser::parser::node::ParameterData,
    ) -> bool {
        arena
            .get(parameter.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .is_some_and(|ident| ident.escaped_text == "this")
    }

    pub(super) fn source_file_interface_heritage_is_direct_lowerable<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        interface: &tsz_parser::parser::node::InterfaceData,
        seen_type_names: &mut Vec<&'b str>,
    ) -> bool {
        let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
            return true;
        };

        heritage_clauses.nodes.iter().copied().all(|clause_idx| {
            let Some(heritage) = arena.get_heritage_clause_at(clause_idx) else {
                return false;
            };
            heritage
                .types
                .nodes
                .iter()
                .copied()
                .all(|heritage_type_idx| {
                    let Some(base_name) =
                        Self::source_file_simple_heritage_identifier(arena, heritage_type_idx)
                    else {
                        return false;
                    };
                    let Some(base_decl_idx) = Self::source_file_direct_heritage_base_decl(
                        arena,
                        delegate_binder,
                        base_name,
                        seen_type_names,
                    ) else {
                        return false;
                    };
                    Self::source_file_interface_declarations_are_direct_lowerable_with_seen(
                        &[(base_decl_idx, arena)],
                        delegate_binder,
                        seen_type_names,
                    )
                })
        })
    }

    pub(super) fn source_file_simple_heritage_identifier(
        arena: &NodeArena,
        heritage_type_idx: NodeIndex,
    ) -> Option<&str> {
        let heritage_node = arena.get(heritage_type_idx)?;
        if let Some(identifier) = arena.get_identifier(heritage_node) {
            return Some(identifier.escaped_text.as_str());
        }
        if let Some(expr_type_args) = arena.get_expr_type_args(heritage_node) {
            if expr_type_args
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty())
            {
                return None;
            }
            return arena
                .get(expr_type_args.expression)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str());
        }

        let type_ref = arena.get_type_ref(heritage_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        arena
            .get(type_ref.type_name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())
    }

    pub(super) fn source_file_direct_heritage_base_decl<'b>(
        arena: &'b NodeArena,
        delegate_binder: &BinderState,
        base_name: &'b str,
        seen_type_names: &[&'b str],
    ) -> Option<NodeIndex> {
        if seen_type_names.contains(&base_name) {
            return None;
        }
        let base_sym_id = delegate_binder.file_locals.get(base_name)?;
        let base_symbol = delegate_binder.get_symbol(base_sym_id)?;
        if base_symbol.flags & symbol_flags::INTERFACE == 0
            || base_symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                != 0
            || base_symbol.declarations.len() != 1
        {
            return None;
        }
        let base_decl_idx = base_symbol.declarations[0];
        if !Self::lib_declaration_name_matches(arena, base_decl_idx, base_name) {
            return None;
        }
        let base_node = arena.get(base_decl_idx)?;
        arena.get_interface(base_node)?;
        Some(base_decl_idx)
    }

    pub(super) fn source_file_expand_direct_lowerable_interface_heritage<'b>(
        declarations: &[(NodeIndex, &'b NodeArena)],
        delegate_binder: &BinderState,
    ) -> Option<Vec<(NodeIndex, &'b NodeArena)>> {
        fn append_bases<'b>(
            arena: &'b NodeArena,
            delegate_binder: &BinderState,
            interface: &tsz_parser::parser::node::InterfaceData,
            seen_type_names: &mut Vec<&'b str>,
            expanded: &mut Vec<(NodeIndex, &'b NodeArena)>,
        ) -> Option<()> {
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                return Some(());
            };
            for clause_idx in heritage_clauses.nodes.iter().copied() {
                let heritage = arena.get_heritage_clause_at(clause_idx)?;
                for heritage_type_idx in heritage.types.nodes.iter().copied() {
                    let base_name = CheckerState::source_file_simple_heritage_identifier(
                        arena,
                        heritage_type_idx,
                    )?;
                    let base_decl_idx = CheckerState::source_file_direct_heritage_base_decl(
                        arena,
                        delegate_binder,
                        base_name,
                        seen_type_names,
                    )?;
                    let base_node = arena.get(base_decl_idx)?;
                    let base_interface = arena.get_interface(base_node)?;
                    seen_type_names.push(base_name);
                    append_bases(
                        arena,
                        delegate_binder,
                        base_interface,
                        seen_type_names,
                        expanded,
                    )?;
                    seen_type_names.pop();
                    expanded.push((base_decl_idx, arena));
                }
            }
            Some(())
        }

        let mut expanded = Vec::new();
        let mut seen_type_names = Vec::new();
        for (decl_idx, arena) in declarations.iter().copied() {
            let node = arena.get(decl_idx)?;
            let interface = arena.get_interface(node)?;
            let interface_name = arena
                .get(interface.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str())?;
            seen_type_names.push(interface_name);
            append_bases(
                arena,
                delegate_binder,
                interface,
                &mut seen_type_names,
                &mut expanded,
            )?;
            seen_type_names.pop();
            expanded.push((decl_idx, arena));
        }
        Some(expanded)
    }

    pub(super) fn source_file_interface_declarations_are_direct_lowerable(
        declarations: &[(NodeIndex, &NodeArena)],
        delegate_binder: &BinderState,
    ) -> bool {
        let mut seen_type_names = Vec::new();
        Self::source_file_interface_declarations_are_direct_lowerable_with_seen(
            declarations,
            delegate_binder,
            &mut seen_type_names,
        )
    }
}
