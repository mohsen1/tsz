impl<'a> CheckerState<'a> {
    fn cross_file_interface_declarations<'b>(
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

    fn interface_declarations_have_heritage(declarations: &[(NodeIndex, &NodeArena)]) -> bool {
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

    fn interface_declarations_have_computed_names(
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

    pub(super) fn source_file_type_node_is_scope_independent(
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

    fn source_file_type_node_is_explicit_unknown(
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

    pub(super) fn source_file_type_node_is_option_bag_lowerable<'b>(
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
            _ => false,
        }
    }

    fn source_file_type_reference_targets_option_bag_lowerable_declaration<'b>(
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

    pub(super) fn source_file_local_name_def_id_for_lowering(
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

    pub(super) fn source_file_type_node_is_generic_scope_independent(
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

    pub(crate) fn type_alias_type_param_names(
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

    pub(super) fn collect_infer_type_param_names(
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

    pub(super) fn source_file_type_node_contains_kind(
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

    pub(super) fn source_file_type_node_contains_disallowed_type_query(
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

    pub(super) fn source_file_type_query_is_well_known_global_symbol_property(
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
        let (base_idx, member_idx) = if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
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
        let Some(base_ident) = arena.get(base_idx).and_then(|base| arena.get_identifier(base))
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

    pub(super) fn source_file_type_node_contains_identifier_name(
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

    fn external_declaration_body_uses_local_array_shadow(
        arena: &NodeArena,
        delegate_binder: &BinderState,
        root: NodeIndex,
    ) -> bool {
        ["Array", "ReadonlyArray"].iter().any(|name| {
            delegate_binder.file_locals.get(name).is_some()
                && Self::source_file_type_node_contains_identifier_name(arena, root, name)
        })
    }

    fn source_file_interface_declarations_are_direct_lowerable_with_seen<'b>(
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
            let result = interface.members.nodes.iter().copied().all(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    return false;
                }
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
            });
            seen_type_names.pop();
            result
        })
    }

    fn source_file_interface_declarations_are_direct_lowerable(
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
