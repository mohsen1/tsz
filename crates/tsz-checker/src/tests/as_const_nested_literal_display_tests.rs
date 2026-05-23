//! Tests for issue #9738: `as const` (and `satisfies` over an `as const`)
//! source types must display their literal members verbatim at every nesting
//! depth in TS2322 assignability diagnostics.
//!
//! Structural rule: when the assignability source is a non-fresh type (it
//! carries no fresh-object-literal display provenance — e.g. produced by
//! `as const`, a declared annotation, or a named type) its literal members are
//! canonical and tsc preserves them verbatim, including nested ones. Only
//! genuinely fresh object literals (whose interned canonical shape is widened)
//! are widened for non-literal targets.
//!
//! Before the fix, only *top-level* literal properties were preserved; nested
//! `as const` literals (`{ p: { q: 1 } }`) were text-widened to
//! `{ readonly p: { readonly q: number; }; }`.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn as_const_nested_object_preserves_nested_literal_against_primitive() {
    // Source is `{ readonly p: { readonly q: 1; }; }`; assigning to a primitive
    // must keep the nested literal `1`, not widen it to `number`.
    let messages = ts2322_messages(
        r#"
const j = { p: { q: 1 } } as const;
const bad: number = j;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ readonly p: { readonly q: 1; }; }")),
        "nested `as const` literal should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("readonly q: number")),
        "nested literal must not be widened, got: {messages:?}"
    );
}

#[test]
fn as_const_satisfies_record_preserves_nested_literal() {
    // The reported repro: `satisfies` only checks, never widens.
    let messages = ts2322_messages(
        r#"
const e = { p: { q: 1 } } as const satisfies Record<string, { q: number }>;
const bad: number = e;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ readonly p: { readonly q: 1; }; }")),
        "`as const satisfies` must preserve nested literal, got: {messages:?}"
    );
}

#[test]
fn as_const_deeply_nested_literal_preserved_with_renamed_keys() {
    // Vary the property names to prove the rule is structural, not keyed on a
    // particular identifier spelling.
    let messages = ts2322_messages(
        r#"
const cfg = { alpha: { beta: { gamma: 7 } } } as const;
const bad: number = cfg;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ readonly alpha: { readonly beta: { readonly gamma: 7; }; }; }")),
        "deeply nested `as const` literal should be preserved, got: {messages:?}"
    );
}

#[test]
fn as_const_object_with_tuple_preserves_element_literals() {
    // Literal members nested inside a tuple within an `as const` object.
    let messages = ts2322_messages(
        r#"
const f = { arr: [1, 2] } as const;
const bad: number = f;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ readonly arr: readonly [1, 2]; }")),
        "tuple element literals under `as const` should be preserved, got: {messages:?}"
    );
}

#[test]
fn declared_annotation_nested_literal_preserved() {
    // A declared annotation is also a non-fresh source: tsc keeps its literals.
    let messages = ts2322_messages(
        r#"
const g: { p: { q: 1 } } = { p: { q: 1 } };
const bad: string = g;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("{ p: { q: 1; }; }")),
        "declared-annotation nested literal should be preserved, got: {messages:?}"
    );
}

#[test]
fn fresh_object_literal_still_widens_nested_for_primitive_target() {
    // Negative/fallback case: a genuinely fresh object literal (no `as const`)
    // must STILL widen its nested members for a non-literal target, matching
    // tsc. This proves the fix did not over-preserve.
    let messages = ts2322_messages(
        r#"
const bad: number = { p: { q: 1 } };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ p: { q: number; }; }")),
        "fresh object literal should widen nested members, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("q: 1")),
        "fresh literal must be widened, not preserved, got: {messages:?}"
    );
}

#[test]
fn as_const_top_level_literal_against_literal_target_preserved() {
    // Sanity: top-level literal against a literal target was already correct;
    // ensure it stays correct.
    let messages = ts2322_messages(
        r#"
const k = { v: 1 } as const;
const bad: 2 = k.v;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Type '1'")),
        "top-level `as const` literal property access should display `1`, got: {messages:?}"
    );
}
