impl<'a> CheckerState<'a> {
    pub(super) fn source_file_local_type_alias_application_is_projection_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        symbol: &Symbol,
        arg_count: usize,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let disallowed = symbol_flags::VALUE
            | symbol_flags::CLASS
            | symbol_flags::INTERFACE
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0
            || symbol.flags & disallowed != 0
            || symbol.declarations.len() != 1
        {
            return false;
        }

        let decl_idx = symbol.declarations[0];
        let Some(decl_node) = arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = arena.get_type_alias(decl_node) else {
            return false;
        };
        let mut seen = Vec::new();
        let Some(target_param_names) =
            Self::source_file_type_alias_application_param_names_are_lowerable(
                arena, binder, type_alias, arg_count, &mut seen, proof,
            )
        else {
            return false;
        };
        if target_param_names.is_empty()
            || target_param_names.len() != arg_count
            || Self::source_file_type_node_contains_disallowed_type_query(
                arena,
                binder,
                type_alias.type_node,
            )
        {
            return false;
        }

        Self::source_file_type_alias_body_is_projection_transparent(
            arena,
            type_alias.type_node,
            &target_param_names,
        )
    }

    fn source_file_type_alias_body_is_projection_transparent(
        arena: &NodeArena,
        node_idx: NodeIndex,
        type_param_names: &[String],
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
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                Self::source_file_type_literal_has_generic_scope_independent_properties(
                    arena,
                    node,
                    type_param_names,
                )
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_alias_body_is_projection_transparent(
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
                    Self::source_file_type_alias_body_is_projection_transparent(
                        arena,
                        wrapped.type_node,
                        type_param_names,
                    )
                })
            }
            _ => false,
        }
    }
}
