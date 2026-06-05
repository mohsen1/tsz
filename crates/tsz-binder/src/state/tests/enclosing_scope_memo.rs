//! Correctness tests for the [`BinderState::find_enclosing_scope`] memo.
//!
//! Structural rule: a node's enclosing scope is a pure positional function of
//! its ancestor chain, so it can be memoized. The walk path-compresses (records
//! every node it passes), which turns the O(depth) per-identifier walk — and the
//! O(depth^2) cost of resolving every type reference in a deeply nested type
//! expression `A<A<A<...>>>` — into linear time. The memo only engages past a
//! depth threshold and only on the computed-property-name-free prefix, where the
//! result is the simple nearest-scope ancestor.
//!
//! The oracle below is shadow-free: identifiers shallower than the memo
//! threshold never touch the cache, so they yield the ground-truth scope; deeper
//! identifiers (which DO engage path compression) must resolve to that same
//! scope. A path-compression bug would make a deep identifier disagree.

use super::*;
use tsz_parser::NodeIndex;
use tsz_scanner::SyntaxKind;

/// Collect every `Identifier` node whose text equals `name`, in arena order
/// (so the first entries are the shallowest occurrences).
fn identifier_nodes_named(
    arena: &tsz_parser::parser::node::NodeArena,
    name: &str,
) -> Vec<NodeIndex> {
    let mut out = Vec::new();
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = arena.get_identifier(node)
            && ident.escaped_text == name
        {
            out.push(idx);
        }
    }
    out
}

/// Like [`identifier_nodes_named`] but only the occurrences whose parent is a
/// `TypeReference` — i.e. uses of the name in type position, excluding the
/// declaration's own name (which lives in a different scope).
fn type_reference_idents_named(
    arena: &tsz_parser::parser::node::NodeArena,
    name: &str,
) -> Vec<NodeIndex> {
    identifier_nodes_named(arena, name)
        .into_iter()
        .filter(|&idx| {
            arena
                .get_extended(idx)
                .and_then(|ext| arena.get(ext.parent))
                .is_some_and(|parent| parent.kind == syntax_kind_ext::TYPE_REFERENCE)
        })
        .collect()
}

#[test]
fn deeply_nested_type_identifiers_share_one_enclosing_scope() {
    // 60 nesting levels: the innermost `A` sits well past the memo threshold,
    // so path compression is exercised. Every `A` reference is lexically at file
    // scope, so they must all resolve to the same scope.
    let depth = 60usize;
    let mut ty = "number".to_string();
    for _ in 0..depth {
        ty = format!("A<{ty}>");
    }
    let source = format!("interface A<T> {{ v: T; }}\ntype R = {ty};\ndeclare const r: R;\n");

    let (binder, parser) = parse_and_bind(&source);
    let arena = parser.get_arena();
    // Only uses in type position: every `A<...>` shares the type-alias's scope.
    // The outermost (first) is shallow — below the memo threshold — so it yields
    // the ground-truth scope a fresh walk would; the deep ones engage the memo.
    let a_idents = type_reference_idents_named(arena, "A");
    assert!(
        a_idents.len() > 40,
        "expected the deep chain to produce many `A` type references, found {}",
        a_idents.len()
    );

    let expected = binder
        .find_enclosing_scope(arena, a_idents[0])
        .expect("shallowest `A` type reference must have an enclosing scope");

    for &idx in &a_idents {
        // Cold then warm: the warm call may hit a path-compressed entry; both
        // must equal the ground-truth scope.
        let cold = binder.find_enclosing_scope(arena, idx);
        let warm = binder.find_enclosing_scope(arena, idx);
        assert_eq!(
            cold,
            Some(expected),
            "deep `A` reference resolved to a different scope than the shallow one"
        );
        assert_eq!(
            warm, cold,
            "warm (memoized) call disagreed with the cold call"
        );
    }
}

#[test]
fn memo_is_name_agnostic() {
    // Same structure, different identifier spelling — proves the fix is about
    // ancestor-chain depth, not a particular name.
    let depth = 50usize;
    let mut ty = "string".to_string();
    for _ in 0..depth {
        ty = format!("Wrapper<{ty}>");
    }
    let source = format!("interface Wrapper<T> {{ inner: T; }}\ntype Out = {ty};\n");

    let (binder, parser) = parse_and_bind(&source);
    let arena = parser.get_arena();
    let idents = type_reference_idents_named(arena, "Wrapper");
    assert!(idents.len() > 40, "expected a deep `Wrapper` chain");

    let expected = binder.find_enclosing_scope(arena, idents[0]);
    assert!(expected.is_some());
    for &idx in &idents {
        assert_eq!(
            binder.find_enclosing_scope(arena, idx),
            expected,
            "all `Wrapper` references share the file scope regardless of depth"
        );
    }
}

#[test]
fn nested_namespace_members_resolve_to_their_namespace_scope() {
    // Deeply nested namespaces create deeply nested *scopes*; an identifier deep
    // inside must still find its nearest enclosing namespace scope. This checks
    // the memo does not skip past or mis-attribute scope-creating nodes on the
    // path.
    let depth = 45usize;
    let mut body = String::new();
    for i in 0..depth {
        body.push_str(&format!("namespace N{i} {{\n"));
    }
    body.push_str("export const leaf = 1;\n");
    for _ in 0..depth {
        body.push('}');
        body.push('\n');
    }

    let (binder, parser) = parse_and_bind(&body);
    let arena = parser.get_arena();
    let leaf = identifier_nodes_named(arena, "leaf");
    assert!(!leaf.is_empty(), "expected to find the `leaf` identifier");

    // The innermost namespace's scope: cold and warm must agree and be Some.
    let cold = binder.find_enclosing_scope(arena, leaf[0]);
    let warm = binder.find_enclosing_scope(arena, leaf[0]);
    assert!(
        cold.is_some(),
        "deeply nested member must have an enclosing scope"
    );
    assert_eq!(warm, cold, "memoized namespace-scope lookup must be stable");
}

#[test]
fn shallow_lookups_are_unaffected() {
    // A normal, shallow program: the memo never engages (threshold not reached),
    // so behavior is exactly the original algorithm.
    let source = "function outer() {\n  const x = 1;\n  return x;\n}\n";
    let (binder, parser) = parse_and_bind(source);
    let arena = parser.get_arena();
    let xs = identifier_nodes_named(arena, "x");
    assert_eq!(xs.len(), 2, "expected the declaration and the use of `x`");
    // Both `x` occurrences live in `outer`'s function scope.
    let s0 = binder.find_enclosing_scope(arena, xs[0]);
    let s1 = binder.find_enclosing_scope(arena, xs[1]);
    assert!(s0.is_some());
    assert_eq!(
        s0, s1,
        "both `x` references resolve to the same function scope"
    );
}
