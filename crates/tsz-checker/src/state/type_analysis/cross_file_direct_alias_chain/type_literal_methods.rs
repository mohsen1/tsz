impl<'a> CheckerState<'a> {
    fn source_file_type_literal_properties_are_lowerable(
        arena: &NodeArena,
        node: &tsz_parser::parser::node::Node,
        mut value_is_lowerable: impl FnMut(NodeIndex) -> bool,
    ) -> bool {
        let Some(type_literal) = arena.get_type_literal(node) else {
            return false;
        };
        type_literal
            .members
            .nodes
            .iter()
            .copied()
            .all(|member_idx| {
                let Some(member_node) = arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                    let Some(index_signature) = arena.get_index_signature(member_node) else {
                        return false;
                    };
                    let Some(param_idx) = index_signature.parameters.nodes.first().copied() else {
                        return false;
                    };
                    let Some(param_node) = arena.get(param_idx) else {
                        return false;
                    };
                    let Some(param) = arena.get_parameter(param_node) else {
                        return false;
                    };
                    return Self::source_file_type_node_is_scope_independent(
                        arena,
                        param.type_annotation,
                    ) && value_is_lowerable(index_signature.type_annotation);
                }
                if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                    return false;
                }
                let Some(signature) = arena.get_signature(member_node) else {
                    return false;
                };
                if signature.type_parameters.is_some()
                    || signature.parameters.is_some()
                    || signature.type_annotation.is_none()
                {
                    return false;
                }
                if arena
                    .get(signature.name)
                    .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    return false;
                }
                value_is_lowerable(signature.type_annotation)
            })
    }
}
