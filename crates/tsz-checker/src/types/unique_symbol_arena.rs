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

pub(super) fn is_unique_symbol_type_annotation(
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

pub(super) fn is_symbol_type_node(arena: &NodeArena, type_annotation: NodeIndex) -> bool {
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

pub(super) fn is_symbol_call_initializer(arena: &NodeArena, init_idx: NodeIndex) -> bool {
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
