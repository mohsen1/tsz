//! Tests for preserving the literal property members of a non-fresh
//! *intersection* source in TS2322-family assignability diagnostics.
//!
//! Structural rule: when the assignability source is a declared (non-fresh)
//! intersection that mixes a primitive with an object member carrying literal
//! property types — `number & { tag: "x" }` — `tsc` renders the source
//! structurally with its literal members intact. `getWidenedType` only widens
//! *fresh* types (already applied at construction), so a declared intersection
//! keeps `"x"` verbatim.
//!
//! Before the fix, `rewrite_source_display_for_non_literal_target_assignability`
//! reached the non-literal-target widening path because its
//! `source_carries_canonical_literal_member` traversal recursed into object,
//! array, tuple, and union members but **not** intersection members — so a
//! `primitive & { tag: "x" }` source was not recognised as carrying a canonical
//! literal member and rendered as `number & { tag: string }`. The fix adds the
//! intersection arm to that traversal, mirroring the union arm.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn primitive_object_intersection_keeps_literal_member() {
    // `number & { tag: "x" }` assigned to `string`: the object member's literal
    // `"x"` must survive, matching `tsc` (`number & { tag: "x"; }`).
    let messages = ts2322_messages(
        r#"
declare const value: number & { tag: "x" };
const text: string = value;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"number & { tag: "x"; }"#)),
        "intersection object member literal must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("tag: string")),
        "the literal member must not be widened, got: {messages:?}"
    );
}

#[test]
fn renamed_binders_keep_literal_member() {
    // The rule is structural, not keyed on the identifier/property spelling:
    // a differently-named binder and member behave identically (anti-hardcoding
    // control).
    let messages = ts2322_messages(
        r#"
declare const widget: number & { kind: "circle" };
const label: string = widget;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"number & { kind: "circle"; }"#)),
        "renamed intersection member literal must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("kind: string")),
        "the literal member must not be widened, got: {messages:?}"
    );
}

#[test]
fn boolean_base_intersection_keeps_literal_member() {
    // The primitive base of the intersection is not number-specific.
    let messages = ts2322_messages(
        r#"
declare const flag: boolean & { z: 1 };
const text: string = flag;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("boolean & { z: 1; }")),
        "boolean-base intersection literal must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("z: number")),
        "the literal member must not be widened, got: {messages:?}"
    );
}

#[test]
fn nested_object_in_intersection_keeps_deep_literal() {
    // A literal nested inside an object member of the intersection is preserved
    // at every depth (the traversal recurses).
    let messages = ts2322_messages(
        r#"
declare const node: number & { inner: { depth: 5 } };
const text: string = node;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("number & { inner: { depth: 5; }; }")),
        "deeply nested intersection literal must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("depth: number")),
        "the nested literal must not be widened, got: {messages:?}"
    );
}

#[test]
fn readonly_literal_member_in_intersection_preserved() {
    let messages = ts2322_messages(
        r#"
declare const cell: number & { readonly slot: 5 };
const text: string = cell;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("number & { readonly slot: 5; }")),
        "readonly literal member must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("slot: number")),
        "the readonly literal member must not be widened, got: {messages:?}"
    );
}

#[test]
fn binary_assignment_intersection_keeps_literal_member() {
    // The binary-assignment (`x = y`) source-display path preserves the literal
    // member the same way the variable-initializer path does.
    let messages = ts2322_messages(
        r#"
declare const source: number & { tag: "x" };
let target: string;
target = source;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"number & { tag: "x"; }"#)),
        "binary-assignment intersection literal must be preserved, got: {messages:?}"
    );
}

#[test]
fn fresh_object_literal_source_still_widens() {
    // Control: a *fresh* object literal source still widens its members to
    // their primitive base, exactly as `tsc` does — the fix only changes the
    // non-fresh (declared) path.
    let messages = ts2322_messages(
        r#"
const text: string = { tag: "x" } as number & { tag: "x" };
"#,
    );
    // The intersection alias keeps its literal even via `as`, mirroring tsc;
    // the important control is that fresh *bare* object literals widen.
    let widened = ts2322_messages(
        r#"
const plain: string = { count: 1, name: "n" };
"#,
    );
    assert!(
        widened
            .iter()
            .any(|m| m.contains("{ count: number; name: string; }")),
        "fresh bare object literal must widen its members, got: {widened:?}"
    );
    // Sanity: the intersection assertion case still produces a TS2322 (shape
    // assertion to a primitive is an error in both compilers).
    assert!(
        !messages.is_empty(),
        "intersection assertion to a primitive should still error, got: {messages:?}"
    );
}

#[test]
fn object_object_intersection_unaffected() {
    // Control: an object-and-object intersection (no primitive member) was
    // already preserved and must remain so.
    let messages = ts2322_messages(
        r#"
declare const pair: { a: 1 } & { b: "y" };
const text: string = pair;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"{ a: 1; } & { b: "y"; }"#)),
        "object-object intersection literals must be preserved, got: {messages:?}"
    );
}
