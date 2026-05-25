use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(super) fn source_file_type_node_is_generic_local_alias_application_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
    ) -> bool {
        let mut seen = AliasCycleTracker::new();
        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
            arena,
            binder,
            node_idx,
            type_param_names,
            &mut seen,
        )
    }

    pub(super) fn source_file_type_node_is_non_generic_local_alias_chain_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
    ) -> bool {
        let mut seen = AliasCycleTracker::new();
        Self::source_file_type_node_is_local_alias_chain_lowerable(
            arena, binder, node_idx, &mut seen,
        )
    }

    fn source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        seen: &mut AliasCycleTracker,
    ) -> bool {
        if Self::source_file_type_node_is_generic_scope_independent(
            arena,
            node_idx,
            type_param_names,
        ) {
            return true;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let Some(args) = type_ref.type_arguments.as_ref() else {
                    return Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena, binder, node_idx, seen,
                    );
                };
                if args.nodes.is_empty() {
                    return Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena, binder, node_idx, seen,
                    );
                }
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };
                let Some(raw_sym_id) = binder.file_locals.get(name) else {
                    return false;
                };
                let Some(sym_id) =
                    Self::source_file_resolve_alias_symbol_for_lowering(binder, raw_sym_id)
                else {
                    return false;
                };
                if seen.contains(&sym_id) {
                    return false;
                }
                let Some(symbol) = binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
                    return false;
                }
                let disallowed = symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if symbol.flags & disallowed != 0 || symbol.declarations.len() != 1 {
                    return false;
                }
                let decl_idx = symbol.declarations[0];
                let Some(decl_node) = arena.get(decl_idx) else {
                    return false;
                };
                let Some(type_alias) = arena.get_type_alias(decl_node) else {
                    return false;
                };
                let target_param_names = Self::type_alias_type_param_names(arena, type_alias);
                if args.nodes.len() != target_param_names.len() {
                    return false;
                }
                if !args.nodes.iter().copied().all(|arg| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        arg,
                        type_param_names,
                        seen,
                    )
                }) {
                    return false;
                }
                if Self::source_file_type_node_contains_kind(
                    arena,
                    type_alias.type_node,
                    syntax_kind_ext::TYPE_QUERY,
                ) {
                    return false;
                }
                if !seen.push(sym_id) {
                    return false;
                }
                let result =
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        type_alias.type_node,
                        &target_param_names,
                        seen,
                    );
                seen.pop(sym_id);
                result
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                arena.get_conditional_type(node).is_some_and(|conditional| {
                    let mut true_branch_names = type_param_names.to_vec();
                    Self::collect_infer_type_param_names(
                        arena,
                        conditional.extends_type,
                        &mut true_branch_names,
                    );
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        conditional.check_type,
                        type_param_names,
                        seen,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        conditional.extends_type,
                        type_param_names,
                        seen,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        conditional.true_type,
                        &true_branch_names,
                        seen,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        conditional.false_type,
                        type_param_names,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => arena.get_array_type(node).is_some_and(|array| {
                Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                    arena,
                    binder,
                    array.element_type,
                    type_param_names,
                    seen,
                )
            }),
            k if k == syntax_kind_ext::TUPLE_TYPE => arena.get_tuple_type(node).is_some_and(|tuple| {
                tuple.elements.nodes.iter().copied().all(|element| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        element,
                        type_param_names,
                        seen,
                    )
                })
            }),
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                            arena,
                            binder,
                            member,
                            type_param_names,
                            seen,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        wrapped.type_node,
                        type_param_names,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        operator.type_node,
                        type_param_names,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        indexed.object_type,
                        type_param_names,
                        seen,
                    ) && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                        arena,
                        binder,
                        indexed.index_type,
                        type_param_names,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                Self::source_file_mapped_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    type_param_names,
                    seen,
                )
            }
            _ => false,
        }
    }

    fn source_file_type_node_is_local_alias_chain_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        seen: &mut AliasCycleTracker,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let has_type_arguments = type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };
                let Some(raw_sym_id) = binder.file_locals.get(name) else {
                    return false;
                };
                let Some(sym_id) =
                    Self::source_file_resolve_alias_symbol_for_lowering(binder, raw_sym_id)
                else {
                    return false;
                };
                if seen.contains(&sym_id) {
                    return false;
                }
                let Some(symbol) = binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
                    return false;
                }
                let disallowed = symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if symbol.flags & disallowed != 0 || symbol.declarations.len() != 1 {
                    return false;
                }
                let decl_idx = symbol.declarations[0];
                let Some(decl_node) = arena.get(decl_idx) else {
                    return false;
                };
                let Some(type_alias) = arena.get_type_alias(decl_node) else {
                    return false;
                };
                let target_param_names = Self::type_alias_type_param_names(arena, type_alias);
                if has_type_arguments {
                    let Some(args) = type_ref.type_arguments.as_ref() else {
                        return false;
                    };
                    if args.nodes.len() != target_param_names.len() {
                        return false;
                    }
                    if !args.nodes.iter().copied().all(|arg| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable(
                            arena, binder, arg, seen,
                        )
                    }) {
                        return false;
                    }
                    if Self::source_file_type_node_contains_kind(
                        arena,
                        type_alias.type_node,
                        syntax_kind_ext::TYPE_QUERY,
                    ) || !seen.push(sym_id)
                    {
                        return false;
                    }
                    let result =
                        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                            arena,
                            binder,
                            type_alias.type_node,
                            &target_param_names,
                            seen,
                        );
                    seen.pop(sym_id);
                    if !result {
                        return false;
                    }
                    return true;
                }
                if !target_param_names.is_empty()
                    || type_alias
                        .type_parameters
                        .as_ref()
                        .is_some_and(|p| !p.nodes.is_empty())
                {
                    return false;
                }
                if !seen.push(sym_id) {
                    return false;
                }
                let result = Self::source_file_type_node_is_local_alias_chain_lowerable(
                    arena,
                    binder,
                    type_alias.type_node,
                    seen,
                );
                seen.pop(sym_id);
                result
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable(
                            arena, binder, member, seen,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        array.element_type,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable(
                            arena, binder, element, seen,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        wrapped.type_node,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(node).is_some_and(|operator| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        operator.type_node,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                arena.get_indexed_access_type(node).is_some_and(|indexed| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        indexed.object_type,
                        seen,
                    ) && Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        indexed.index_type,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                Self::source_file_mapped_type_is_generic_local_alias_application_lowerable(
                    arena,
                    binder,
                    node,
                    &[],
                    seen,
                )
            }
            _ => false,
        }
    }

    fn source_file_resolve_alias_symbol_for_lowering(
        binder: &BinderState,
        sym_id: SymbolId,
    ) -> Option<SymbolId> {
        let symbol = binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        if import_name == "*" {
            return None;
        }
        let (target_sym_id, _) =
            binder.resolve_import_with_reexports_type_only(module_specifier, import_name)?;
        (target_sym_id != sym_id).then_some(target_sym_id)
    }

    fn source_file_mapped_type_is_generic_local_alias_application_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut AliasCycleTracker,
    ) -> bool {
        let Some(mapped) = arena.get_mapped_type(node) else {
            return false;
        };
        if mapped
            .members
            .as_ref()
            .is_some_and(|members| !members.nodes.is_empty())
        {
            return false;
        }
        let Some(type_param_node) = arena.get(mapped.type_parameter) else {
            return false;
        };
        let Some(type_param) = arena.get_type_parameter(type_param_node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_param.name) else {
            return false;
        };
        let Some(name) = arena.get_identifier(name_node) else {
            return false;
        };

        if !type_param.constraint.is_none()
            && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                arena,
                binder,
                type_param.constraint,
                type_param_names,
                seen,
            )
        {
            return false;
        }
        if !type_param.default.is_none()
            && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                arena,
                binder,
                type_param.default,
                type_param_names,
                seen,
            )
        {
            return false;
        }

        let mut mapped_param_names = type_param_names.to_vec();
        if !mapped_param_names
            .iter()
            .any(|param| param == &name.escaped_text)
        {
            mapped_param_names.push(name.escaped_text.to_string());
        }

        (mapped.name_type.is_none()
            || Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                arena,
                binder,
                mapped.name_type,
                &mapped_param_names,
                seen,
            ))
            && (mapped.type_node.is_none()
                || Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                    arena,
                    binder,
                    mapped.type_node,
                    &mapped_param_names,
                    seen,
                ))
    }
}
