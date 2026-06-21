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

/// Is the immediate owner of this `unique symbol` type-operator node (skipping
/// enclosing parenthesized types) a `readonly` property signature — an interface
/// or object-type-literal member?
///
/// Per tsc's `isValidESSymbolDeclaration`, such a member is a valid
/// `unique symbol` owner and gets a `unique symbol` of its own — identically for
/// interface and object-type-literal members. tsz must construct that unique
/// symbol at this site rather than widening the member to plain `symbol`,
/// otherwise `typeof obj.prop` loses the unique-symbol identity for
/// object-type-literal members (the variable-declaration ancestor would
/// otherwise make `has_declared_unique_symbol_owner` treat it as a declared
/// owner and widen it).
pub(crate) fn is_readonly_unique_symbol_property_signature(
    arena: &NodeArena,
    idx: NodeIndex,
) -> bool {
    let Some(parent_idx) = parenthesized_type_parent(arena, idx) else {
        return false;
    };
    let Some(parent) = arena.get(parent_idx) else {
        return false;
    };
    parent.kind == syntax_kind_ext::PROPERTY_SIGNATURE
        && arena
            .get_signature(parent)
            .is_some_and(|sig| arena.has_modifier(&sig.modifiers, SyntaxKind::ReadonlyKeyword))
}

/// The non-parenthesized owner of `idx`, skipping any enclosing
/// `(... )` parenthesized type wrappers (matching tsc's
/// `walkUpParenthesizedTypes(node.parent)`).
fn parenthesized_type_parent(arena: &NodeArena, idx: NodeIndex) -> Option<NodeIndex> {
    let mut cursor = idx;
    loop {
        let parent_idx = arena.get_extended(cursor)?.parent;
        let parent = arena.get(parent_idx)?;
        if parent.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            cursor = parent_idx;
            continue;
        }
        return Some(parent_idx);
    }
}

pub(crate) fn has_declared_unique_symbol_owner(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(parent_ext) = arena.get_extended(idx) else {
        return false;
    };
    let parent_idx = parent_ext.parent;
    let Some(parent) = arena.get(parent_idx) else {
        return false;
    };

    if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        return true;
    }

    // `static readonly p: unique symbol` on a class owns a unique-symbol
    // identity, the same way a `const x: unique symbol` does.
    if parent.kind == syntax_kind_ext::PROPERTY_DECLARATION {
        let owner_idx = arena.get_extended(parent_idx).map(|ext| ext.parent);
        if is_static_readonly_class_property(arena, parent_idx, owner_idx) {
            return true;
        }
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

/// Returns true when `prop_idx` is a property declaration whose modifier list
/// contains both `static` and `readonly`, and whose immediate owner is a
/// class declaration/expression. Caller passes `owner_idx` (the property's
/// parent) to avoid re-resolving `arena.get_extended(prop_idx)`.
fn is_static_readonly_class_property(
    arena: &NodeArena,
    prop_idx: NodeIndex,
    owner_idx: Option<NodeIndex>,
) -> bool {
    let Some(node) = arena.get(prop_idx) else {
        return false;
    };
    let Some(prop) = arena.get_property_decl(node) else {
        return false;
    };
    if !arena.is_static(&prop.modifiers) {
        return false;
    }
    if !arena.has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword) {
        return false;
    }
    let Some(owner) = owner_idx.and_then(|idx| arena.get(idx)) else {
        return false;
    };
    owner.kind == syntax_kind_ext::CLASS_DECLARATION
        || owner.kind == syntax_kind_ext::CLASS_EXPRESSION
}
