//! Arena-only structural inspectors for `unique symbol` recognition.
//!
//! These helpers walk a `NodeArena` to decide whether a type annotation or
//! initializer expresses the `unique symbol` shape (`unique symbol` type
//! operator, the `symbol` type reference, or a `Symbol(...)` call).  They
//! depend solely on the arena and are intentionally free of `&self` so they
//! can be shared between `type_node.rs` and other resolvers without
//! additional boilerplate.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

pub(crate) fn is_unique_symbol_type_annotation(
    arena: &NodeArena,
    type_annotation: NodeIndex,
) -> bool {
    let Some(type_node) = arena.get(type_annotation) else {
        return false;
    };
    match type_node.kind {
        k if k == syntax_kind_ext::TYPE_OPERATOR => {
            arena.get_type_operator(type_node).is_some_and(|op| {
                op.operator == SyntaxKind::UniqueKeyword as u16
                    && is_symbol_type_node(arena, op.type_node)
            })
        }
        _ => false,
    }
}

pub(crate) fn is_unique_symbol_type_annotation_unwrapped(
    arena: &NodeArena,
    type_annotation: NodeIndex,
) -> bool {
    is_unique_symbol_type_annotation(arena, unwrap_parenthesized_type(arena, type_annotation))
}

pub(crate) fn unwrap_parenthesized_type(arena: &NodeArena, mut type_idx: NodeIndex) -> NodeIndex {
    while let Some(node) = arena.get(type_idx)
        && node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
        && let Some(wrapped) = arena.get_wrapped_type(node)
    {
        type_idx = wrapped.type_node;
    }
    type_idx
}

pub(crate) fn is_symbol_type_node(arena: &NodeArena, type_annotation: NodeIndex) -> bool {
    let Some(type_node) = arena.get(type_annotation) else {
        return false;
    };
    if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
        return false;
    }
    let Some(type_ref) = arena.get_type_ref(type_node) else {
        return false;
    };
    let Some(name_node) = arena.get(type_ref.type_name) else {
        return false;
    };
    arena
        .get_identifier(name_node)
        .is_some_and(|ident| ident.escaped_text == "symbol")
}

pub(crate) fn is_symbol_call_initializer(arena: &NodeArena, init_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(init_idx) else {
        return false;
    };
    if node.kind != syntax_kind_ext::CALL_EXPRESSION {
        return false;
    }
    let Some(call) = arena.get_call_expr(node) else {
        return false;
    };
    let Some(expr_node) = arena.get(call.expression) else {
        return false;
    };
    arena
        .get_identifier(expr_node)
        .is_some_and(|ident| ident.escaped_text == "Symbol")
}

pub(crate) fn has_declared_unique_symbol_owner(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(parent) = arena
        .get_extended(idx)
        .and_then(|ext| arena.get(ext.parent))
    else {
        return false;
    };

    if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        return true;
    }

    if parent.kind == syntax_kind_ext::PROPERTY_SIGNATURE
        || parent.kind == syntax_kind_ext::PROPERTY_DECLARATION
    {
        let mut cursor = idx;
        while let Some(ext) = arena.get_extended(cursor) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return true;
            }
            if parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            {
                return false;
            }
            cursor = parent_idx;
        }
    }

    false
}
