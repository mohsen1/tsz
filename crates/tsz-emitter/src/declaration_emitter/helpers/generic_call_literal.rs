use super::super::DeclarationEmitter;

use tsz_binder::symbol_flags;

use tsz_parser::parser::node::{FunctionData, NodeArena};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

include!("generic_call_literal_parts/part1.rs");
include!("generic_call_literal_parts/part2.rs");

fn function_return_type_parameter_name(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<String> {
    type_reference_identifier_name(source_arena, func.type_annotation)
}

fn function_returned_function_type_parameter_name(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<String> {
    let return_node = source_arena.get(func.type_annotation)?;
    if return_node.kind != syntax_kind_ext::FUNCTION_TYPE {
        return None;
    }
    let function_type = source_arena.get_function_type(return_node)?;
    type_reference_identifier_name(source_arena, function_type.type_annotation)
}

fn function_declares_type_parameter(
    source_arena: &NodeArena,
    func: &FunctionData,
    type_param_name: &str,
) -> bool {
    func.type_parameters.as_ref().is_some_and(|type_params| {
        type_params.nodes.iter().copied().any(|param_idx| {
            source_arena
                .get(param_idx)
                .and_then(|node| source_arena.get_type_parameter(node))
                .and_then(|param| identifier_text(source_arena, param.name))
                .is_some_and(|name| name == type_param_name)
        })
    })
}

fn function_has_rest_array_parameter_for_type_param(
    source_arena: &NodeArena,
    func: &FunctionData,
    type_param_name: &str,
) -> bool {
    func.parameters.nodes.iter().copied().any(|param_idx| {
        let Some(param_node) = source_arena.get(param_idx) else {
            return false;
        };
        let Some(param) = source_arena.get_parameter(param_node) else {
            return false;
        };
        if !param.dot_dot_dot_token {
            return false;
        }
        rest_array_element_type_parameter_name(source_arena, param.type_annotation).as_deref()
            == Some(type_param_name)
    })
}

fn rest_array_element_type_parameter_name(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
) -> Option<String> {
    let type_node = source_arena.get(type_idx)?;
    if type_node.kind == syntax_kind_ext::ARRAY_TYPE {
        let array = source_arena.get_array_type(type_node)?;
        return type_reference_identifier_name(source_arena, array.element_type);
    }
    if type_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
        || type_node.kind == syntax_kind_ext::OPTIONAL_TYPE
        || type_node.kind == syntax_kind_ext::REST_TYPE
    {
        let wrapped = source_arena.get_wrapped_type(type_node)?;
        return rest_array_element_type_parameter_name(source_arena, wrapped.type_node);
    }
    None
}

fn function_return_tuple_type_parameter_names(
    source_arena: &NodeArena,
    func: &FunctionData,
) -> Option<Vec<String>> {
    let return_node = source_arena.get(func.type_annotation)?;
    if return_node.kind != syntax_kind_ext::TUPLE_TYPE {
        return None;
    }
    let tuple = source_arena.get_tuple_type(return_node)?;
    tuple
        .elements
        .nodes
        .iter()
        .copied()
        .map(|element_idx| type_reference_identifier_name(source_arena, element_idx))
        .collect()
}

fn type_literal_function_property_return_type_params(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
) -> Vec<(String, String)> {
    let Some(type_node) = source_arena.get(type_idx) else {
        return Vec::new();
    };
    if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
        return Vec::new();
    }
    let Some(type_literal) = source_arena.get_type_literal(type_node) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for &member_idx in &type_literal.members.nodes {
        let Some(member_node) = source_arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            continue;
        }
        let Some(signature) = source_arena.get_signature(member_node) else {
            continue;
        };
        let Some(property_name) = identifier_text(source_arena, signature.name) else {
            continue;
        };
        let Some(type_node) = source_arena.get(signature.type_annotation) else {
            continue;
        };
        if type_node.kind != syntax_kind_ext::FUNCTION_TYPE {
            continue;
        }
        let Some(function_type) = source_arena.get_function_type(type_node) else {
            continue;
        };
        let Some(return_type_param) =
            type_reference_identifier_name(source_arena, function_type.type_annotation)
        else {
            continue;
        };
        result.push((property_name, return_type_param));
    }
    result
}

fn parameter_type_has_property_type_parameter(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    property_name: &str,
    type_param_name: &str,
) -> bool {
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_LITERAL => source_arena
            .get_type_literal(type_node)
            .is_some_and(|literal| {
                literal.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                        return false;
                    }
                    let Some(signature) = source_arena.get_signature(member_node) else {
                        return false;
                    };
                    identifier_text(source_arena, signature.name).as_deref() == Some(property_name)
                        && type_reference_identifier_name(source_arena, signature.type_annotation)
                            .as_deref()
                            == Some(type_param_name)
                })
            }),
        k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        parameter_type_has_property_type_parameter(
                            source_arena,
                            part_idx,
                            property_name,
                            type_param_name,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE => source_arena
            .get_wrapped_type(type_node)
            .is_some_and(|wrapped| {
                parameter_type_has_property_type_parameter(
                    source_arena,
                    wrapped.type_node,
                    property_name,
                    type_param_name,
                )
            }),
        _ => false,
    }
}

fn return_type_parameter_appears_in_other_parameters(
    source_arena: &NodeArena,
    func: &FunctionData,
    selected_param_idx: NodeIndex,
    type_param_name: &str,
) -> bool {
    func.parameters
        .nodes
        .iter()
        .copied()
        .filter(|param_idx| *param_idx != selected_param_idx)
        .any(|param_idx| {
            source_arena
                .get(param_idx)
                .and_then(|node| source_arena.get_parameter(node))
                .is_some_and(|param| {
                    type_node_references_type_parameter(
                        source_arena,
                        param.type_annotation,
                        type_param_name,
                        0,
                    )
                })
        })
}

fn type_node_references_type_parameter(
    source_arena: &NodeArena,
    type_idx: NodeIndex,
    type_param_name: &str,
    depth: u8,
) -> bool {
    if depth > 32 {
        return false;
    }
    let Some(type_node) = source_arena.get(type_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == SyntaxKind::Identifier as u16 => {
            identifier_text(source_arena, type_idx).as_deref() == Some(type_param_name)
        }
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            let Some(type_ref) = source_arena.get_type_ref(type_node) else {
                return false;
            };
            identifier_text(source_arena, type_ref.type_name).as_deref() == Some(type_param_name)
                || type_ref.type_arguments.as_ref().is_some_and(|type_args| {
                    type_args.nodes.iter().copied().any(|arg_idx| {
                        type_node_references_type_parameter(
                            source_arena,
                            arg_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::TYPE_LITERAL => source_arena
            .get_type_literal(type_node)
            .is_some_and(|literal| {
                literal.members.nodes.iter().copied().any(|member_idx| {
                    let Some(member_node) = source_arena.get(member_idx) else {
                        return false;
                    };
                    source_arena
                        .get_signature(member_node)
                        .is_some_and(|signature| {
                            type_node_references_type_parameter(
                                source_arena,
                                signature.type_annotation,
                                type_param_name,
                                depth + 1,
                            )
                        })
                })
            }),
        k if k == syntax_kind_ext::INTERSECTION_TYPE || k == syntax_kind_ext::UNION_TYPE => {
            source_arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    composite.types.nodes.iter().copied().any(|part_idx| {
                        type_node_references_type_parameter(
                            source_arena,
                            part_idx,
                            type_param_name,
                            depth + 1,
                        )
                    })
                })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            source_arena
                .get_wrapped_type(type_node)
                .is_some_and(|wrapped| {
                    type_node_references_type_parameter(
                        source_arena,
                        wrapped.type_node,
                        type_param_name,
                        depth + 1,
                    )
                })
        }
        k if k == syntax_kind_ext::ARRAY_TYPE => {
            source_arena.get_array_type(type_node).is_some_and(|array| {
                type_node_references_type_parameter(
                    source_arena,
                    array.element_type,
                    type_param_name,
                    depth + 1,
                )
            })
        }
        k if k == syntax_kind_ext::TUPLE_TYPE => {
            source_arena.get_tuple_type(type_node).is_some_and(|tuple| {
                tuple.elements.nodes.iter().copied().any(|element_idx| {
                    type_node_references_type_parameter(
                        source_arena,
                        element_idx,
                        type_param_name,
                        depth + 1,
                    )
                })
            })
        }
        k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
            source_arena
                .get_function_type(type_node)
                .is_some_and(|func_type| {
                    type_node_references_type_parameter(
                        source_arena,
                        func_type.type_annotation,
                        type_param_name,
                        depth + 1,
                    ) || func_type.parameters.nodes.iter().copied().any(|param_idx| {
                        source_arena
                            .get(param_idx)
                            .and_then(|node| source_arena.get_parameter(node))
                            .is_some_and(|param| {
                                type_node_references_type_parameter(
                                    source_arena,
                                    param.type_annotation,
                                    type_param_name,
                                    depth + 1,
                                )
                            })
                    })
                })
        }
        _ => false,
    }
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
