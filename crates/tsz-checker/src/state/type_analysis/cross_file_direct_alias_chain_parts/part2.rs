impl<'a> CheckerState<'a> {
    fn source_file_mapped_type_is_local_alias_chain_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
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

        if type_param.constraint.is_some()
            && !Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                type_param.constraint,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            )
        {
            return false;
        }
        if type_param.default.is_some()
            && !Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                type_param.default,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            )
        {
            return false;
        }

        (mapped.name_type.is_none()
            || Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                arena,
                binder,
                mapped.name_type,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            ))
            && (mapped.type_node.is_none()
                || Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                    arena,
                    binder,
                    mapped.type_node,
                    seen,
                    proof,
                    true,
                    inferred_guard_names,
                ))
    }

    fn source_file_indexed_access_object_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> bool {
        if let Some(node) = arena.get(node_idx)
            && node.kind == syntax_kind_ext::TYPE_LITERAL
        {
            return Self::source_file_type_literal_has_lowerable_properties(
                arena,
                binder,
                node,
                type_param_names,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            );
        }
        Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
            arena,
            binder,
            node_idx,
            type_param_names,
            seen,
            proof,
            recursion_guarded,
            inferred_guard_names,
        )
    }

    fn source_file_indexed_access_object_is_local_alias_chain_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node_idx: NodeIndex,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> bool {
        if let Some(node) = arena.get(node_idx)
            && node.kind == syntax_kind_ext::TYPE_LITERAL
        {
            return Self::source_file_type_literal_has_local_alias_chain_lowerable_properties(
                arena, binder, node, seen, proof,
            );
        }
        Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
            arena,
            binder,
            node_idx,
            seen,
            proof,
            recursion_guarded,
            inferred_guard_names,
        )
    }

    fn source_file_type_literal_has_generic_scope_independent_properties(
        arena: &NodeArena,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(
            arena,
            None,
            node,
            None,
            |type_node| {
                Self::source_file_type_node_is_generic_scope_independent(
                    arena,
                    type_node,
                    type_param_names,
                )
            },
        )
    }

    fn source_file_type_literal_has_lowerable_properties<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        _recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(
            arena,
            Some(binder),
            node,
            Some(proof),
            |type_node| {
                Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    type_node,
                    type_param_names,
                    seen,
                    proof,
                    true,
                    inferred_guard_names,
                )
            },
        )
    }

    fn source_file_type_literal_has_local_alias_chain_lowerable_properties<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
    ) -> bool {
        Self::source_file_type_literal_properties_are_lowerable(
            arena,
            Some(binder),
            node,
            Some(proof),
            |type_node| {
                Self::source_file_type_node_is_local_alias_chain_lowerable_with_guard(
                    arena,
                    binder,
                    type_node,
                    seen,
                    proof,
                    true,
                    &[],
                )
            },
        )
    }

    fn source_file_template_literal_type_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> bool {
        let Some(template) = arena.get_template_literal_type(node) else {
            return false;
        };
        if arena.get(template.head).is_none() {
            return false;
        }
        template.template_spans.nodes.iter().copied().all(|span_idx| {
            let Some(span_node) = arena.get(span_idx) else {
                return false;
            };
            let Some(span) = arena.get_template_span(span_node) else {
                return false;
            };
            arena.get(span.literal).is_some()
                && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    span.expression,
                    type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                    inferred_guard_names,
                )
        })
    }

    fn source_file_function_type_is_generic_local_alias_application_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        node: &tsz_parser::parser::node::Node,
        type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> bool {
        let Some(function_type) = arena.get_function_type(node) else {
            return false;
        };
        let Some(function_type_param_names) =
            Self::source_file_function_type_param_names_are_lowerable(
                arena,
                binder,
                function_type.type_parameters.as_ref(),
                type_param_names,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            )
        else {
            return false;
        };
        let active_type_param_names = if function_type_param_names.is_empty() {
            type_param_names.to_vec()
        } else {
            function_type_param_names
        };
        function_type.parameters.nodes.iter().copied().all(|param_idx| {
            let Some(param_node) = arena.get(param_idx) else {
                return false;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                return false;
            };
            param.type_annotation.is_some()
                && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.type_annotation,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                    inferred_guard_names,
                )
        }) && function_type.type_annotation.is_some()
            && Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                arena,
                binder,
                function_type.type_annotation,
                &active_type_param_names,
                seen,
                proof,
                recursion_guarded,
                inferred_guard_names,
            )
    }

    fn source_file_function_type_param_names_are_lowerable<'b>(
        arena: &'b NodeArena,
        binder: &'b BinderState,
        params: Option<&NodeList>,
        outer_type_param_names: &[String],
        seen: &mut Vec<SourceFileAliasProofKey>,
        proof: &SourceFileAliasProofContext<'b>,
        recursion_guarded: bool,
        inferred_guard_names: &[String],
    ) -> Option<Vec<String>> {
        let Some(params) = params else {
            return Some(Vec::new());
        };
        let mut active_type_param_names = outer_type_param_names.to_vec();
        let mut param_data = Vec::with_capacity(params.nodes.len());
        for param_idx in params.nodes.iter().copied() {
            let param_node = arena.get(param_idx)?;
            let param = arena.get_type_parameter(param_node)?;
            let name_node = arena.get(param.name)?;
            let name = arena.get_identifier(name_node)?;
            if !active_type_param_names
                .iter()
                .any(|param_name| param_name == &name.escaped_text)
            {
                active_type_param_names.push(name.escaped_text.to_string());
            }
            param_data.push(param);
        }

        for param in param_data {
            if param.constraint.is_some()
                && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.constraint,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                    inferred_guard_names,
                )
            {
                return None;
            }
            if param.default.is_some()
                && !Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_guard(
                    arena,
                    binder,
                    param.default,
                    &active_type_param_names,
                    seen,
                    proof,
                    recursion_guarded,
                    inferred_guard_names,
                )
            {
                return None;
            }
        }

        Some(active_type_param_names)
    }
}
