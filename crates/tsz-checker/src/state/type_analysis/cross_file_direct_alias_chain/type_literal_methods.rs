//! Type-literal lowerability helpers for source-file alias chains.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    pub(super) fn source_file_type_literal_properties_are_lowerable(
        arena: &NodeArena,
        binder: Option<&BinderState>,
        node: &tsz_parser::parser::node::Node,
        proof: Option<&SourceFileAliasProofContext<'_>>,
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
                    && !Self::source_file_computed_property_name_is_direct_lowerable(
                        arena,
                        binder,
                        signature.name,
                        proof,
                    )
                {
                    return false;
                }
                value_is_lowerable(signature.type_annotation)
            })
    }

    pub(super) fn source_file_computed_property_name_is_direct_lowerable(
        arena: &NodeArena,
        binder: Option<&BinderState>,
        name_idx: NodeIndex,
        proof: Option<&SourceFileAliasProofContext<'_>>,
    ) -> bool {
        let Some(name_node) = arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return true;
        }
        let Some(binder) = binder else {
            return false;
        };
        let Some(proof) = proof else {
            return false;
        };
        let Some(computed) = arena.get_computed_property(name_node) else {
            return false;
        };
        Self::source_file_computed_property_expression_is_direct_lowerable(
            arena,
            binder,
            computed.expression,
            proof,
        )
    }

    pub(super) fn source_file_computed_property_expression_is_direct_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        expr_idx: NodeIndex,
        proof: &SourceFileAliasProofContext<'_>,
    ) -> bool {
        let Some(expr_node) = arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = arena.get_access_expr(expr_node) else {
                return false;
            };
            return Self::source_file_computed_property_expression_is_direct_lowerable(
                arena,
                binder,
                access.expression,
                proof,
            );
        }
        let Some(ident) = arena.get_identifier(expr_node) else {
            return false;
        };
        let name = ident.escaped_text.as_str();
        let local_has_value = binder
            .file_locals
            .get(name)
            .and_then(|sym_id| binder.get_symbol(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::VALUE));
        !local_has_value && (proof.global_value_is_lowerable)(binder, name)
    }
}
