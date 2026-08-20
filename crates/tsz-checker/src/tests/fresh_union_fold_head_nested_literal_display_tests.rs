//! Nested-literal display preservation in the fresh object-literal union-fold
//! HEAD (#17782).
//!
//! Structural rule: when a fresh object-literal source fails against a union
//! target and the diagnostic head re-renders the source from its syntax, a
//! property whose value is itself an object literal recurses with the
//! target's own per-property type (`tsc`'s
//! `getBestMatchIndexedAccessTypeOrUndefined` derivation) as the nested
//! contextual target — so a nested literal whose contextual property type
//! carries a literal of the same primitive base is preserved
//! (`v: { x: 9; }`), exactly like a top-level literal, instead of widening to
//! its base (`v: { x: number; }`). A nested literal with no same-base literal
//! in its contextual property type still widens.
//!
//! All expectations oracle-pinned against pinned typescript@7.0.2
//! (`scripts/conformance/oracle.sh --strict`, 2026-08-20). Binder and property
//! names vary across cases so the behavior is proven structural.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;

fn diags_with_code(source: &str, code: u32) -> Vec<Diagnostic> {
    check_source_strict(source)
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

fn single_diag(source: &str, code: u32) -> Diagnostic {
    let mut diags = diags_with_code(source, code);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one TS{code} for `{source}`, got: {diags:?}"
    );
    diags.remove(0)
}

#[test]
fn string_literal_nested_property_is_preserved_in_head() {
    // tsc: Type '{ tag: "m"; inner: { s: "q"; }; }' is not assignable to type 'W'.
    let diag = single_diag(
        r#"
type W = { tag: "m"; inner: { s: "p" } } | { tag: "n"; inner: { s: "q" } };
const w: W = { tag: "m", inner: { s: "q" } };
"#,
        2322,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Type '{ tag: "m"; inner: { s: "q"; }; }' is not assignable to type 'W'."#
        ),
        "string nested literal must survive the head render, got: {}",
        diag.message_text
    );
}

#[test]
fn boolean_literal_nested_property_is_preserved_in_head() {
    // tsc: Type '{ on: true; p: { q: true; }; }' is not assignable to type 'F'.
    let diag = single_diag(
        r#"
type F = { on: true; p: { q: false } } | { on: false; p: { q: true } };
const f: F = { on: true, p: { q: true } };
"#,
        2322,
    );
    assert!(
        diag.message_text
            .starts_with(r#"Type '{ on: true; p: { q: true; }; }' is not assignable to type 'F'."#),
        "boolean nested literal must survive the head render, got: {}",
        diag.message_text
    );
}

#[test]
fn deep_nested_literal_is_preserved_at_every_level() {
    // tsc: Type '{ k: "x"; a: { b: { c: 7; }; }; }' is not assignable to type 'D'.
    let diag = single_diag(
        r#"
type D = { k: "x"; a: { b: { c: 1 } } } | { k: "y"; a: { b: { c: 7 } } };
const d: D = { k: "x", a: { b: { c: 7 } } };
"#,
        2322,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Type '{ k: "x"; a: { b: { c: 7; }; }; }' is not assignable to type 'D'."#
        ),
        "nested literal must survive the head render at every depth, got: {}",
        diag.message_text
    );
}

#[test]
fn ts2345_argument_head_preserves_nested_literal() {
    // tsc: Argument of type '{ kind: "a"; v: { x: 9; }; }' is not assignable
    // to parameter of type 'A'.
    let diag = single_diag(
        r#"
type A = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } };
declare function take(arg: A): void;
take({ kind: "a", v: { x: 9 } });
"#,
        2345,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Argument of type '{ kind: "a"; v: { x: 9; }; }' is not assignable to parameter of type 'A'."#
        ),
        "TS2345 argument head must preserve the nested literal, got: {}",
        diag.message_text
    );
}

#[test]
fn nested_literal_without_same_base_contextual_literal_still_widens() {
    // Negative control. The arms' `v.x` is a bare primitive (`number` /
    // `string`), so tsc widens the boolean literal and reports the
    // inner-anchored cross-arm leaf, not an outer fold head.
    // tsc: (2,34): Type 'boolean' is not assignable to type 'string | number'.
    let diag = single_diag(
        r#"
type U2 = { kind: "a"; v: { x: number } } | { kind: "b"; v: { x: string } };
const u2: U2 = { kind: "a", v: { x: true } };
"#,
        2322,
    );
    assert_eq!(
        diag.message_text, "Type 'boolean' is not assignable to type 'string | number'.",
        "a nested literal with no same-base contextual literal must still widen"
    );
}

#[test]
fn as_const_nested_object_keeps_readonly_and_literal_in_head() {
    // tsc: Type '{ kind: "a"; v: { readonly x: 9; }; }' is not assignable to type 'B'.
    let diag = single_diag(
        r#"
type B = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 9 } };
const b: B = { kind: "a", v: { x: 9 } as const };
"#,
        2322,
    );
    assert!(
        diag.message_text.starts_with(
            r#"Type '{ kind: "a"; v: { readonly x: 9; }; }' is not assignable to type 'B'."#
        ),
        "as-const nested object must keep readonly + literal in the head, got: {}",
        diag.message_text
    );
}
