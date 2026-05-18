//! Structural guards for direct source-file type lowering.

use tsz_parser::NodeIndex;
use tsz_parser::parser::node::{NodeArena, TypeAliasData};
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
                matches!(name, "Array" | "ReadonlyArray")
                    && type_ref.type_arguments.as_ref().is_some_and(|args| {
                        args.nodes.len() == 1
                            && is_generic_direct_lowerable(arena, args.nodes[0], type_param_names)
                    })
            })
        }
        // Source-file conditional, composite, object, mapped, callable,
        // indexed-access, and type-operator bodies can carry file-local
        // binding, contextual, distributive, and recursive mapped-type
        // behavior that the child checker already handles correctly. Keep
        // those on the mature path until direct lowering has a semantic proof
        // for them.
        k if k == syntax_kind_ext::CONDITIONAL_TYPE
            || k == syntax_kind_ext::INFER_TYPE
            || k == syntax_kind_ext::ARRAY_TYPE
            || k == syntax_kind_ext::TUPLE_TYPE
            || k == syntax_kind_ext::UNION_TYPE
            || k == syntax_kind_ext::INTERSECTION_TYPE
            || k == syntax_kind_ext::TYPE_LITERAL
            || k == syntax_kind_ext::MAPPED_TYPE
            || k == syntax_kind_ext::FUNCTION_TYPE
            || k == syntax_kind_ext::CONSTRUCTOR_TYPE
            || k == syntax_kind_ext::TYPE_OPERATOR
            || k == syntax_kind_ext::INDEXED_ACCESS_TYPE =>
        {
            false
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            arena.get_wrapped_type(node).is_some_and(|wrapped| {
                is_generic_direct_lowerable(arena, wrapped.type_node, type_param_names)
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

fn type_parameter_name(arena: &NodeArena, param_idx: NodeIndex) -> Option<String> {
    let param_node = arena.get(param_idx)?;
    let param = arena.get_type_parameter(param_node)?;
    let name_node = arena.get(param.name)?;
    let ident = arena.get_identifier(name_node)?;
    Some(ident.escaped_text.to_string())
}
