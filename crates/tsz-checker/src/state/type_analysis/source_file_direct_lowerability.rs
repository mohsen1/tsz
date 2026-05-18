//! Structural guards for direct source-file type lowering.

use tsz_parser::NodeIndex;
use tsz_parser::parser::base::NodeList;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::syntax_kind_ext;

pub(super) fn is_scope_independent(arena: &NodeArena, node_idx: NodeIndex) -> bool {
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
                            args.nodes.len() == 1 && is_scope_independent(arena, args.nodes[0])
                        })
                    }
                    _ => false,
                }
            })
        }
        k if k == syntax_kind_ext::LITERAL_TYPE => arena.get_literal_type(node).is_some(),
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            arena.get_composite_type(node).is_some_and(|composite| {
                composite
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .all(|member| is_scope_independent(arena, member))
            })
        }
        k if k == syntax_kind_ext::ARRAY_TYPE => arena
            .get_array_type(node)
            .is_some_and(|array| is_scope_independent(arena, array.element_type)),
        k if k == syntax_kind_ext::TUPLE_TYPE => arena.get_tuple_type(node).is_some_and(|tuple| {
            tuple
                .elements
                .nodes
                .iter()
                .copied()
                .all(|element| is_scope_independent(arena, element))
        }),
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            arena
                .get_wrapped_type(node)
                .is_some_and(|wrapped| is_scope_independent(arena, wrapped.type_node))
        }
        _ => false,
    }
}

pub(super) fn is_generic_direct_lowerable(
    arena: &NodeArena,
    node_idx: NodeIndex,
    type_param_names: &[String],
) -> bool {
    if is_scope_independent(arena, node_idx) {
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
                type_ref.type_arguments.as_ref().is_some_and(|args| {
                    !args.nodes.is_empty()
                        && args
                            .nodes
                            .iter()
                            .copied()
                            .all(|arg| is_generic_direct_lowerable(arena, arg, type_param_names))
                })
            })
        }
        k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
            arena.get_conditional_type(node).is_some_and(|conditional| {
                let mut true_branch_names = type_param_names.to_vec();
                collect_infer_type_param_names(
                    arena,
                    conditional.extends_type,
                    &mut true_branch_names,
                );
                is_generic_direct_lowerable(arena, conditional.check_type, type_param_names)
                    && is_generic_direct_lowerable(
                        arena,
                        conditional.extends_type,
                        type_param_names,
                    )
                    && is_generic_direct_lowerable(arena, conditional.true_type, &true_branch_names)
                    && is_generic_direct_lowerable(arena, conditional.false_type, type_param_names)
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
        k if k == syntax_kind_ext::ARRAY_TYPE => arena.get_array_type(node).is_some_and(|array| {
            is_generic_direct_lowerable(arena, array.element_type, type_param_names)
        }),
        k if k == syntax_kind_ext::TUPLE_TYPE => arena.get_tuple_type(node).is_some_and(|tuple| {
            tuple
                .elements
                .nodes
                .iter()
                .copied()
                .all(|element| is_generic_direct_lowerable(arena, element, type_param_names))
        }),
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            arena.get_composite_type(node).is_some_and(|composite| {
                composite
                    .types
                    .nodes
                    .iter()
                    .copied()
                    .all(|member| is_generic_direct_lowerable(arena, member, type_param_names))
            })
        }
        k if k == syntax_kind_ext::TYPE_LITERAL => {
            arena.get_type_literal(node).is_some_and(|type_literal| {
                type_literal.members.nodes.iter().copied().all(|member| {
                    type_literal_member_is_generic_direct_lowerable(arena, member, type_param_names)
                })
            })
        }
        k if k == syntax_kind_ext::MAPPED_TYPE => {
            arena.get_mapped_type(node).is_some_and(|mapped| {
                let Some(mapped_param_name) = type_parameter_name(arena, mapped.type_parameter)
                else {
                    return false;
                };
                let Some(mapped_param_node) = arena.get(mapped.type_parameter) else {
                    return false;
                };
                let Some(mapped_param) = arena.get_type_parameter(mapped_param_node) else {
                    return false;
                };
                let mut mapped_scope_names = type_param_names.to_vec();
                mapped_scope_names.push(mapped_param_name);

                let constraint_lowerable = mapped_param.constraint.is_none()
                    || is_generic_direct_lowerable(
                        arena,
                        mapped_param.constraint,
                        type_param_names,
                    );
                let default_lowerable = mapped_param.default.is_none()
                    || is_generic_direct_lowerable(arena, mapped_param.default, type_param_names);
                let name_type_lowerable = mapped.name_type.is_none()
                    || is_generic_direct_lowerable(arena, mapped.name_type, &mapped_scope_names);
                let template_lowerable = mapped.type_node.is_none()
                    || is_generic_direct_lowerable(arena, mapped.type_node, &mapped_scope_names);
                let members_lowerable = mapped
                    .members
                    .as_ref()
                    .is_none_or(|members| members.nodes.is_empty());

                constraint_lowerable
                    && default_lowerable
                    && name_type_lowerable
                    && template_lowerable
                    && members_lowerable
            })
        }
        k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
            arena.get_function_type(node).is_some_and(|function_type| {
                function_type.type_parameters.is_none()
                    && parameters_are_generic_direct_lowerable(
                        arena,
                        &function_type.parameters,
                        type_param_names,
                    )
                    && (function_type.type_annotation.is_none()
                        || is_generic_direct_lowerable(
                            arena,
                            function_type.type_annotation,
                            type_param_names,
                        ))
            })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            arena.get_wrapped_type(node).is_some_and(|wrapped| {
                is_generic_direct_lowerable(arena, wrapped.type_node, type_param_names)
            })
        }
        k if k == syntax_kind_ext::TYPE_OPERATOR => {
            arena.get_type_operator(node).is_some_and(|operator| {
                is_generic_direct_lowerable(arena, operator.type_node, type_param_names)
            })
        }
        k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
            arena.get_indexed_access_type(node).is_some_and(|indexed| {
                is_generic_direct_lowerable(arena, indexed.object_type, type_param_names)
                    && is_generic_direct_lowerable(arena, indexed.index_type, type_param_names)
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
        .filter_map(|param_idx| type_parameter_name(arena, param_idx))
        .collect()
}

fn parameters_are_generic_direct_lowerable(
    arena: &NodeArena,
    parameters: &NodeList,
    type_param_names: &[String],
) -> bool {
    parameters.nodes.iter().copied().all(|param_idx| {
        let Some(param_node) = arena.get(param_idx) else {
            return false;
        };
        let Some(param) = arena.get_parameter(param_node) else {
            return false;
        };
        param.type_annotation.is_none()
            || is_generic_direct_lowerable(arena, param.type_annotation, type_param_names)
    })
}

fn type_literal_member_is_generic_direct_lowerable(
    arena: &NodeArena,
    member_idx: NodeIndex,
    type_param_names: &[String],
) -> bool {
    let Some(member_node) = arena.get(member_idx) else {
        return false;
    };

    if let Some(signature) = arena.get_signature(member_node) {
        if signature.type_parameters.is_some() {
            return false;
        }
        if let Some(name_node) = arena.get(signature.name)
            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
        {
            return false;
        }
        let params_lowerable = signature.parameters.as_ref().is_none_or(|params| {
            parameters_are_generic_direct_lowerable(arena, params, type_param_names)
        });
        return params_lowerable
            && (signature.type_annotation.is_none()
                || is_generic_direct_lowerable(
                    arena,
                    signature.type_annotation,
                    type_param_names,
                ));
    }

    if let Some(index_sig) = arena.get_index_signature(member_node) {
        return parameters_are_generic_direct_lowerable(
            arena,
            &index_sig.parameters,
            type_param_names,
        ) && (index_sig.type_annotation.is_none()
            || is_generic_direct_lowerable(arena, index_sig.type_annotation, type_param_names));
    }

    false
}

fn type_parameter_name(arena: &NodeArena, param_idx: NodeIndex) -> Option<String> {
    let param_node = arena.get(param_idx)?;
    let param = arena.get_type_parameter(param_node)?;
    let name_node = arena.get(param.name)?;
    let ident = arena.get_identifier(name_node)?;
    Some(ident.escaped_text.to_string())
}

fn collect_infer_type_param_names(arena: &NodeArena, root: NodeIndex, names: &mut Vec<String>) {
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
