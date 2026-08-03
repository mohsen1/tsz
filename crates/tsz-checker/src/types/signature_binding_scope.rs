//! Value bindings introduced by a signature parameter's binding pattern.
//!
//! A destructuring parameter declares real locals, and those locals stay in
//! scope for every *type* position of the same signature. Because a signature
//! has no body, the only place they can be referenced is a `typeof` type query:
//!
//! ```text
//! type F = ({ a: renamed }: O) => typeof renamed;
//! ```
//!
//! `tsc` resolves `typeof renamed` to the binding declared by `{ a: renamed }`
//! and, because the binding is referenced, reports nothing. Two tsz behaviours
//! depend on modelling that scope:
//!
//! * the `typeof` query must resolve, instead of falling through to global name
//!   resolution and producing `TS2304`/`TS2693`;
//! * `TS2842` ("is an unused renaming of") must fire only when the renamed
//!   binding has no such reference.
//!
//! Both are answered here so the two emission sites and the scope seeding share
//! one definition of "referenced inside the owning signature".

use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use crate::CheckerContext;

/// Signature-like nodes that own a parameter list and bound the scope of the
/// bindings that list declares.
const fn is_signature_like(kind: u16) -> bool {
    matches!(
        kind,
        syntax_kind_ext::FUNCTION_TYPE
            | syntax_kind_ext::CONSTRUCTOR_TYPE
            | syntax_kind_ext::CALL_SIGNATURE
            | syntax_kind_ext::CONSTRUCT_SIGNATURE
            | syntax_kind_ext::METHOD_SIGNATURE
            | syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::CONSTRUCTOR
            | syntax_kind_ext::GET_ACCESSOR
            | syntax_kind_ext::SET_ACCESSOR
    )
}

/// The innermost signature-like ancestor of `idx`, including `idx` itself.
fn enclosing_signature(ctx: &CheckerContext, idx: NodeIndex) -> Option<NodeIndex> {
    enclosing_signature_through_type_query(ctx, idx).map(|(signature, _)| signature)
}

/// As [`enclosing_signature`], also reporting whether a `typeof` type query was
/// crossed on the way up. Only a type query can reference a value binding from a
/// type position, so callers that model *resolution* require it; the `TS2842`
/// usage check starts at the binding itself and does not.
fn enclosing_signature_through_type_query(
    ctx: &CheckerContext,
    idx: NodeIndex,
) -> Option<(NodeIndex, bool)> {
    let mut current = idx;
    let mut guard = 0;
    let mut saw_type_query = false;
    while current.is_some() {
        guard += 1;
        if guard > 256 {
            return None;
        }
        let node = ctx.arena.get(current)?;
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            saw_type_query = true;
        }
        if is_signature_like(node.kind) {
            return Some((current, saw_type_query));
        }
        let ext = ctx.arena.get_extended(current)?;
        if ext.parent.is_none() {
            return None;
        }
        current = ext.parent;
    }
    None
}

/// True when some `typeof <name>` type query inside `root` names `name`.
///
/// Nested signatures are deliberately included: an inner signature's type
/// positions are still inside the outer signature's scope, so a reference from
/// there keeps the outer binding used.
fn subtree_has_type_query_reference(ctx: &CheckerContext, root: NodeIndex, name: &str) -> bool {
    let mut stack = vec![root];
    let mut guard = 0usize;
    while let Some(node_idx) = stack.pop() {
        guard += 1;
        if guard > 100_000 {
            return false;
        }
        let Some(node) = ctx.arena.get(node_idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(type_query) = ctx.arena.get_type_query(node)
            && let Some(expr_node) = ctx.arena.get(type_query.expr_name)
            && expr_node.kind == SyntaxKind::Identifier as u16
            && ctx.arena.get_identifier_text(type_query.expr_name) == Some(name)
        {
            return true;
        }
        stack.extend(ctx.arena.get_children(node_idx));
    }
    false
}

/// True when some binding element under `name_idx` binds the identifier `name`.
fn binding_pattern_declares(ctx: &CheckerContext, name_idx: NodeIndex, name: &str) -> bool {
    let Some(node) = ctx.arena.get(name_idx) else {
        return false;
    };
    if node.kind == SyntaxKind::Identifier as u16 {
        return ctx.arena.get_identifier_text(name_idx) == Some(name);
    }
    if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
        && node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
    {
        return false;
    }
    let Some(pattern) = ctx.arena.get_binding_pattern(node) else {
        return false;
    };
    pattern.elements.nodes.iter().any(|&elem_idx| {
        ctx.arena
            .get(elem_idx)
            .filter(|elem_node| elem_node.kind != syntax_kind_ext::OMITTED_EXPRESSION)
            .and_then(|elem_node| ctx.arena.get_binding_element(elem_node))
            .is_some_and(|elem| binding_pattern_declares(ctx, elem.name, name))
    })
}

/// Signature-like kinds that are pure type syntax with no body of their own:
/// a `FunctionType`/`MethodSignature`/etc. nested inside an outer signature's
/// return type is still part of that outer signature's type positions, not a
/// separate scope. The remaining `is_signature_like` kinds (function/method/
/// accessor/constructor declarations and expressions) can own a body and
/// resolve their own parameters as ordinary local symbols, so they end the
/// climb to an outer signature.
const fn is_pure_type_signature(kind: u16) -> bool {
    matches!(
        kind,
        syntax_kind_ext::FUNCTION_TYPE
            | syntax_kind_ext::CONSTRUCTOR_TYPE
            | syntax_kind_ext::CALL_SIGNATURE
            | syntax_kind_ext::CONSTRUCT_SIGNATURE
            | syntax_kind_ext::METHOD_SIGNATURE
    )
}

/// True when a parameter declared directly on `signature` (not on a nested
/// signature inside it) binds `name`.
fn signature_own_parameters_declare(
    ctx: &CheckerContext,
    signature: NodeIndex,
    name: &str,
) -> bool {
    // Walk the signature's own subtree for its parameters without descending
    // into a nested signature: an inner signature's parameters are scoped to
    // that inner signature and must not answer for the outer one.
    let mut stack = vec![signature];
    let mut guard = 0usize;
    while let Some(node_idx) = stack.pop() {
        guard += 1;
        if guard > 100_000 {
            return false;
        }
        let Some(node) = ctx.arena.get(node_idx) else {
            continue;
        };
        if node_idx != signature && is_signature_like(node.kind) {
            continue;
        }
        if node.kind == syntax_kind_ext::PARAMETER
            && let Some(param) = ctx.arena.get_parameter(node)
            // A plain identifier parameter already resolves through the normal
            // symbol path; only pattern-introduced bindings need this.
            && ctx.arena.get_identifier_text(param.name).is_none()
            && binding_pattern_declares(ctx, param.name, name)
        {
            return true;
        }
        stack.extend(ctx.arena.get_children(node_idx));
    }
    false
}

/// True when a parameter of the signature owning `within` declares a value
/// binding called `name`.
///
/// This is the resolution half of the scope: a `typeof name` query inside the
/// signature names that binding, so it must not fall through to global name
/// resolution. It answers structurally from the AST rather than from seeded
/// scope state, so it holds on every traversal of the signature, not only the
/// one that lowers its return type.
///
/// A `typeof` query nested inside a pure-type signature that is itself part
/// of an outer signature's return type (`(a: T) => (b: U) => typeof a`) climbs
/// to that outer signature when the inner one doesn't declare the name — the
/// inner `FunctionType` has no body and so no scope of its own.
pub(crate) fn signature_parameter_declares_binding(
    ctx: &CheckerContext,
    within: NodeIndex,
    name: &str,
) -> bool {
    let Some((mut signature, saw_type_query)) = enclosing_signature_through_type_query(ctx, within)
    else {
        return false;
    };
    if !saw_type_query {
        return false;
    }
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 256 {
            return false;
        }
        if signature_own_parameters_declare(ctx, signature, name) {
            return true;
        }
        let Some(node) = ctx.arena.get(signature) else {
            return false;
        };
        if !is_pure_type_signature(node.kind) {
            return false;
        }
        let Some(ext) = ctx.arena.get_extended(signature) else {
            return false;
        };
        if ext.parent.is_none() {
            return false;
        }
        let Some(next) = enclosing_signature(ctx, ext.parent) else {
            return false;
        };
        signature = next;
    }
}

/// True when the binding named `name`, declared by a parameter of the signature
/// owning `within`, is referenced by a `typeof` query in that same signature.
///
/// `within` may be any node inside the signature — a parameter, a binding
/// element, or the signature itself.
pub(crate) fn binding_is_referenced_by_type_query(
    ctx: &CheckerContext,
    within: NodeIndex,
    name: &str,
) -> bool {
    let Some(signature) = enclosing_signature(ctx, within) else {
        return false;
    };
    subtree_has_type_query_reference(ctx, signature, name)
}
