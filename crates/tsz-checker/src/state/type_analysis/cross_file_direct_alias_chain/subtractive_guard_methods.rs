impl<'a> CheckerState<'a> {
    fn source_file_type_arguments_contain_guard_name(
        arena: &NodeArena,
        args: &NodeList,
        guard_names: &[String],
    ) -> bool {
        !guard_names.is_empty()
            && args.nodes.iter().copied().any(|arg| {
                Self::source_file_type_node_contains_any_identifier_name(arena, arg, guard_names)
            })
    }

    fn source_file_type_arguments_contain_subtractive_guard<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        args: &NodeList,
        type_param_names: &[String],
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        args.nodes.iter().copied().any(|arg| {
            Self::source_file_type_node_is_subtractive_type_param_guard(
                arena,
                binder,
                arg,
                type_param_names,
                proof,
            )
        })
    }

    fn source_file_type_node_is_subtractive_type_param_guard<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = arena.get_wrapped_type(node)
        {
            return Self::source_file_type_node_is_subtractive_type_param_guard(
                arena,
                binder,
                wrapped.type_node,
                type_param_names,
                proof,
            );
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        let Some(args) = type_ref.type_arguments.as_ref() else {
            return false;
        };
        if args.nodes.len() != 2 {
            return false;
        }
        let Some(source_name) = Self::source_file_bare_type_param_name(arena, args.nodes[0]) else {
            return false;
        };
        let Some(removed_name) = Self::source_file_bare_type_param_name(arena, args.nodes[1])
        else {
            return false;
        };
        if source_name == removed_name
            || !type_param_names.iter().any(|name| name == source_name)
            || !type_param_names.iter().any(|name| name == removed_name)
        {
            return false;
        }
        let Some(name) = arena
            .get(type_ref.type_name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())
        else {
            return false;
        };
        (name == "Exclude"
            && Self::source_file_type_name_can_fall_back_to_global_type(
                arena, binder, name, proof,
            ))
            || Self::source_file_type_ref_is_transparent_subtractive_alias(
                arena, binder, name, proof,
            )
    }

    fn source_file_type_ref_is_transparent_subtractive_alias<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        name: &str,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let Some(raw_sym_id) = binder.file_locals.get(name) else {
            return false;
        };
        let Some(resolved) =
            Self::source_file_resolve_alias_symbol_for_lowering(arena, binder, raw_sym_id, proof)
        else {
            return false;
        };
        let Some(symbol) = resolved.binder.get_symbol(resolved.sym_id) else {
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
        let Some(decl_node) = resolved.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = resolved.arena.get_type_alias(decl_node) else {
            return false;
        };
        let Some(param_names) =
            Self::source_file_pair_type_alias_param_names(resolved.arena, type_alias)
        else {
            return false;
        };
        let resolved_proof = proof.for_file(resolved.file_idx);
        Self::source_file_type_node_is_global_exclude_of_pair_params(
            resolved.arena,
            resolved.binder,
            type_alias.type_node,
            &param_names,
            &resolved_proof,
        )
    }

    fn source_file_pair_type_alias_param_names(
        arena: &NodeArena,
        type_alias: &TypeAliasData,
    ) -> Option<[String; 2]> {
        let params = type_alias.type_parameters.as_ref()?;
        if params.nodes.len() != 2 {
            return None;
        }
        let first = Self::source_file_type_param_name(arena, params.nodes[0])?;
        let second = Self::source_file_type_param_name(arena, params.nodes[1])?;
        (first != second).then(|| [first.to_string(), second.to_string()])
    }

    fn source_file_type_param_name(arena: &NodeArena, node_idx: NodeIndex) -> Option<&str> {
        let param_node = arena.get(node_idx)?;
        let param = arena.get_type_parameter(param_node)?;
        let name_node = arena.get(param.name)?;
        arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.as_str())
    }

    fn source_file_type_node_is_global_exclude_of_pair_params<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        param_names: &[String; 2],
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = arena.get_wrapped_type(node)
        {
            return Self::source_file_type_node_is_global_exclude_of_pair_params(
                arena,
                binder,
                wrapped.type_node,
                param_names,
                proof,
            );
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        let Some(name) = arena
            .get(type_ref.type_name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())
        else {
            return false;
        };
        if name != "Exclude"
            || !Self::source_file_type_name_can_fall_back_to_global_type(arena, binder, name, proof)
        {
            return false;
        }
        let Some(args) = type_ref.type_arguments.as_ref() else {
            return false;
        };
        if args.nodes.len() != 2 {
            return false;
        }
        Self::source_file_bare_type_param_name(arena, args.nodes[0])
            .is_some_and(|name| name == param_names[0].as_str())
            && Self::source_file_bare_type_param_name(arena, args.nodes[1])
                .is_some_and(|name| name == param_names[1].as_str())
    }

    fn source_file_bare_type_param_name(arena: &NodeArena, node_idx: NodeIndex) -> Option<&str> {
        let node = arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let wrapped = arena.get_wrapped_type(node)?;
            return Self::source_file_bare_type_param_name(arena, wrapped.type_node);
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = arena.get_type_ref(node)?;
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

    fn source_file_type_name_can_fall_back_to_global_type<'b>(
        arena: &NodeArena,
        binder: &'b BinderState,
        name: &str,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        let Some(raw_sym_id) = binder.file_locals.get(name) else {
            return (proof.global_type_is_lowerable)(binder, name);
        };
        Self::source_file_local_symbol_can_fall_back_to_global_type(arena, binder, raw_sym_id)
            && (proof.global_type_is_lowerable)(binder, name)
    }

    fn source_file_type_node_contains_any_identifier_name(
        arena: &NodeArena,
        root: NodeIndex,
        names: &[String],
    ) -> bool {
        names
            .iter()
            .any(|name| Self::source_file_type_node_contains_identifier_name(arena, root, name))
    }
}
