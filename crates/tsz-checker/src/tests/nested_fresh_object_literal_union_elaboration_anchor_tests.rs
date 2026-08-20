//! A fresh object-literal source checked against a union target with 2+
//! object-like members never runs the per-property AST walk
//! (`try_elaborate_object_literal_properties_with_source`), even when the
//! failing property's own value is itself an object/array literal.
//!
//! Structural rule: when `isRelatedTo`'s excess-property union fold
//! (`hasExcessProperties`) owns the relation — i.e. the target, after
//! stripping nullish members, still has 2+ object-like members — tsc never
//! recurses via `elaborateElementwise` into a nested property's own value;
//! it keeps ONE diagnostic anchored at the outer expression, with a
//! dotted-path `The types of 'a.b' are incompatible between these types.`
//! fold down to the leaf relation. tsz did this through the solver's
//! `fresh_object_literal_union_property_fold` (#17721/#17729) plus the
//! checker's property-chain renderer, but the checker's own per-property AST
//! walk raced it: for a property whose value is an object/array literal, the
//! walk recursed into that value's own node and reported an independent
//! TS2322 anchored at the innermost failing leaf, dropping the outer
//! union-head/dotted-path frame and (in a call-argument position) even the
//! diagnostic code (TS2345 became TS2322). The walk now defers to the fold
//! whenever the union has 2+ object-like members (`union_target_object_like_member_count`),
//! matching the solver's own `check_members` gate.
//!
//! A union with at most one object-like member (e.g. `string | { a: T }`) is
//! never ambiguous for `hasExcessProperties`, so tsc keeps the ordinary
//! elementwise drill there — the walk must not bail in that case.

use crate::test_utils::check_source_diagnostics;

#[test]
fn nested_object_literal_property_keeps_outer_anchor_and_dotted_path() {
    let diags = check_source_diagnostics(
        r#"
type R = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 2 } };
const r: R = { kind: "a", v: { x: 2 } };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 anchored at the outer assignment, got: {diags:?}"
    );
    let diag = ts2322[0];
    // Anchored at `r` (the declaration), not at the nested `2` literal.
    assert!(
        diag.message_text.contains("is not assignable to type 'R'"),
        "diagnostic should carry the outer head, got: {}",
        diag.message_text
    );
    let chain: Vec<&str> = diag
        .related_information
        .iter()
        .map(|r| r.message_text.as_str())
        .collect();
    assert!(
        chain
            .iter()
            .any(|m| m.contains("The types of 'v.x' are incompatible")),
        "expected the dotted-path fold for 'v.x', got chain: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|m| m.contains("Type '2' is not assignable to type '1'")),
        "expected the leaf relation line, got chain: {chain:?}"
    );
}

#[test]
fn nested_object_literal_property_call_argument_keeps_ts2345_and_dotted_path() {
    let diags = check_source_diagnostics(
        r#"
type R6 = { kind: "a"; v: { x: 1 } } | { kind: "b"; v: { x: 2 } };
function f6(r: R6) {}
f6({ kind: "a", v: { x: 2 } });
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "call-argument elaboration must not fall back to TS2322, got: {diags:?}"
    );
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 anchored at the call argument, got: {diags:?}"
    );
    let chain: Vec<&str> = ts2345[0]
        .related_information
        .iter()
        .map(|r| r.message_text.as_str())
        .collect();
    assert!(
        chain
            .iter()
            .any(|m| m.contains("The types of 'v.x' are incompatible")),
        "expected the dotted-path fold for 'v.x', got chain: {chain:?}"
    );
}

#[test]
fn split_across_arms_plain_leaf_property_unchanged() {
    // No property value is itself an object/array literal here, so this is
    // the pre-existing #17721/#17729 witness — must stay unaffected by the
    // new union-member-count gate.
    let diags = check_source_diagnostics(
        r#"
type R2 = { p: 1; q: 2 } | { p: 3; q: 4 };
const r2: R2 = { p: 1, q: 4 };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "got: {diags:?}");
    let chain: Vec<&str> = ts2322[0]
        .related_information
        .iter()
        .map(|r| r.message_text.as_str())
        .collect();
    assert!(
        chain
            .iter()
            .any(|m| m.contains("Types of property 'q' are incompatible")),
        "expected the single-property frame for 'q', got chain: {chain:?}"
    );
}

#[test]
fn non_union_target_nested_property_keeps_deep_anchor() {
    // A plain (non-union) target keeps tsc's ordinary `elaborateElementwise`
    // drill: the diagnostic anchors at the innermost failing literal with no
    // outer frame. This must NOT change: the new gate only fires when the
    // union carries 2+ object-like members.
    let diags = check_source_diagnostics(
        r#"
type T4 = { a: { b: 1 } };
const t4: T4 = { a: { b: 2 } };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "got: {diags:?}");
    assert_eq!(
        ts2322[0].message_text, "Type '2' is not assignable to type '1'.",
        "non-union nested property mismatch should keep the bare leaf message"
    );
    assert!(
        !ts2322[0]
            .related_information
            .iter()
            .any(|r| r.message_text.contains("incompatible")),
        "non-union nested property mismatch should not gain an elaboration chain, got: {:?}",
        ts2322[0].related_information
    );
}

#[test]
fn nullable_union_nested_property_keeps_deep_anchor() {
    // `T | undefined` normalizes to a single non-nullish member before the
    // object-like-member count is taken, so it must keep the ordinary
    // elementwise drill exactly like the non-union case above.
    let diags = check_source_diagnostics(
        r#"
type T5 = { a: 1 } | undefined;
function f5(x: T5) {}
f5({ a: 2 });
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "got: {diags:?}");
    assert_eq!(
        ts2322[0].message_text, "Type '2' is not assignable to type '1'.",
        "nullable-union nested property mismatch should keep the bare leaf message"
    );
}

#[test]
fn union_with_single_object_like_member_keeps_deep_drill() {
    // Only one member of the union (`{ a: { b: number } }`) is object-like;
    // `string` is not, so there is no `hasExcessProperties` ambiguity and tsc
    // keeps the ordinary elementwise drill all the way to the innermost
    // literal — the gate must not fire here.
    let diags = check_source_diagnostics(
        r#"
type R7 = string | { a: { b: number } };
const r7: R7 = { a: { b: "x" } };
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "got: {diags:?}");
    assert_eq!(
        ts2322[0].message_text, "Type 'string' is not assignable to type 'number'.",
        "single-object-member union should keep the deep elementwise drill"
    );
}
