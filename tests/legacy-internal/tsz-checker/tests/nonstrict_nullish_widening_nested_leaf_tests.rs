//! Regression tests for #16396: the non-strict nullish widening-provenance
//! gate's leaf rule must be closed under nesting.
//!
//! Structural rule: with `strictNullChecks` off, the `null`/`undefined`-to-`any`
//! widening is a property of the *expression*. A fresh array/object literal
//! inherits the flavour from its elements, so the gate walks the literal's
//! syntax and asks, of every leaf, "is this a widening source?". The leaf rule
//! previously answered that with `leaf_type != undefined && leaf_type != null`,
//! which decides only whether a leaf is *exactly* scalar nullish. But
//! `widen_nullish_to_any_deep` recurses, so a leaf typed `undefined[]` passed
//! the gate and then had its *interior* rewritten:
//! `declare function supply(): undefined[]; var v = [supply()];` inferred
//! `any[][]` where tsc keeps `undefined[][]` — a false negative that silently
//! accepts later assignments tsc rejects.
//!
//! The predicate is now asked of the widener itself —
//! `widen_nullish_to_any_deep(leaf) == leaf` — which is the same question in
//! nesting-closed form and the formulation #16383's return-contribution seam
//! already uses. The rows below therefore cover both seams that share the walk:
//! the mutable-binding seam (`widen_initializer_type_for_mutable_binding_gated`,
//! #16384 leg B) and the generic-call candidate seam
//! (`fresh_literal_argument_nullish_leaves_are_widening`, #16384 leg A).
//!
//! Every row is pinned against a real `typescript@7.0.2` oracle,
//! `--target es2015 --strict false --noImplicitAny false`. Binder names are
//! varied across rows so no row can be satisfied by a name-shaped predicate.

use crate::test_utils::{check_with_options_code_messages, non_strict_checker_options};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

fn assert_inferred(source: &str, expected_type: &str, context: &str) {
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            format!("Type '{expected_type}' is not assignable to type 'string'.")
        )],
        "{context}: {messages:?}"
    );
}

// ── The reported witness and its nesting family (mutable-binding seam) ──

/// The reported repro: a call whose *declared* return type is `undefined[]`
/// carries no widening flavour, so the array built around it keeps its nullish
/// interior — tsc: `undefined[][]`.
#[test]
fn call_returning_declared_undefined_array_stays_unwidened() {
    assert_inferred(
        "\
declare function supply(): undefined[];
var nested = [supply()];
var probe: string = nested;
",
        "undefined[][]",
        "a call returning a declared `undefined[]` is not a widening source",
    );
}

/// Same shape reached through an identifier rather than a call: the leaf rule
/// is about the leaf's *type*, not about which expression form produced it.
#[test]
fn identifier_of_declared_undefined_array_stays_unwidened() {
    assert_inferred(
        "\
declare var holder: undefined[];
var wrapper = [holder];
var probe: string = wrapper;
",
        "undefined[][]",
        "an identifier typed `undefined[]` is not a widening source",
    );
}

/// The object-literal arm of the same walk.
#[test]
fn object_member_of_declared_undefined_array_stays_unwidened() {
    assert_inferred(
        "\
declare function produce(): undefined[];
var boxed = { slot: produce() };
var probe: string = boxed;
",
        "{ slot: undefined[]; }",
        "an object member typed `undefined[]` is not a widening source",
    );
}

/// `null`'s twin of the witness — the leaf rule must not be `undefined`-only.
#[test]
fn declared_null_array_leaf_stays_unwidened() {
    assert_inferred(
        "\
declare var blanks: null[];
var outer = [blanks];
var probe: string = outer;
",
        "null[][]",
        "an identifier typed `null[]` is not a widening source",
    );
}

/// A tuple leaf carrying a nullish slot: the widener would rewrite the slot, so
/// the leaf is not safe, even though the tuple itself is not nullish.
#[test]
fn declared_undefined_tuple_leaf_stays_unwidened() {
    assert_inferred(
        "\
declare var pair: [undefined, string];
var outer = [pair];
var probe: string = outer;
",
        "[undefined, string][]",
        "a tuple leaf with a nullish slot is not a widening source",
    );
}

/// An object-typed leaf carrying a nullish property — the nesting is through a
/// property rather than an element.
#[test]
fn object_typed_undefined_property_leaf_stays_unwidened() {
    assert_inferred(
        "\
declare var record: { field: undefined };
var outer = [record];
var probe: string = outer;
",
        "{ field: undefined; }[]",
        "an object leaf with a nullish property is not a widening source",
    );
}

/// Two levels of fresh literal above the declared leaf: the walk recurses, so
/// the leaf rule must hold at every depth, not only immediately under the
/// initializer.
#[test]
fn deeply_nested_undefined_array_leaf_stays_unwidened() {
    assert_inferred(
        "\
declare function fetchAll(): undefined[];
var deep = [[fetchAll()]];
var probe: string = deep;
",
        "undefined[][][]",
        "the leaf rule must hold at every nesting depth",
    );
}

// ── Mixed literals: one non-widening leaf decides the whole literal ──

/// A genuine widening source (`undefined` keyword) beside a declared
/// `undefined[]` leaf: the enclosing `all` means one non-widening element makes
/// the whole literal non-widening, so the declared leaf keeps `undefined[]`
/// rather than becoming `any[]` — that is the widening half this file's leaf
/// rule owns. The `| undefined` sibling is then absorbed by non-strict union
/// reduction (#16574), so tsc's full answer is `undefined[][]`, not
/// `(undefined[] | undefined)[]` or `any[][]` (the widening-only answer would
/// be wrong in the other direction).
#[test]
fn undefined_keyword_sibling_does_not_rescue_an_undefined_array_leaf() {
    assert_inferred(
        "\
declare function collect(): undefined[];
var mixed = [collect(), undefined];
var probe: string = mixed;
",
        "undefined[][]",
        "one non-widening element makes the whole literal non-widening, then non-strict reduction absorbs `undefined`",
    );
}

/// The elided-hole carve-out (#16393) is permissive on its own and decisive
/// nowhere: a hole beside a declared `undefined[]` leaf must not widen it.
/// Same non-strict union-reduction absorption as the row above (#16574).
#[test]
fn elided_hole_sibling_does_not_rescue_an_undefined_array_leaf() {
    assert_inferred(
        "\
declare function gather(): undefined[];
var sparse = [, gather()];
var probe: string = sparse;
",
        "undefined[][]",
        "an elided hole must not rescue a non-widening sibling leaf, then non-strict reduction absorbs `undefined`",
    );
}

// ── Generic-call candidate seam (shares the same walk) ──

/// The witness through the inference seam: the argument literal's flavour is
/// what propagates into the inferred type argument, so the same leaf rule
/// governs it — tsc: `undefined[][]`.
#[test]
fn generic_call_argument_with_undefined_array_leaf_stays_unwidened() {
    assert_inferred(
        "\
declare function echo<T>(value: T): T;
declare function supplyAll(): undefined[];
var viaCall = echo([supplyAll()]);
var probe: string = viaCall;
",
        "undefined[][]",
        "the generic-call seam shares the leaf rule",
    );
}

/// Control for the seam above: a scalar declared `undefined` argument leaf was
/// already correct (#16384 leg A) and must stay correct.
#[test]
fn generic_call_argument_with_declared_undefined_stays_unwidened() {
    assert_inferred(
        "\
declare function echo<T>(value: T): T;
declare var absent: undefined;
var viaCall = echo([absent]);
var probe: string = viaCall;
",
        "undefined[]",
        "a declared scalar `undefined` argument leaf must stay unwidened",
    );
}

/// Control for the seam above: the bare `undefined` keyword still carries the
/// flavour through inference — tsc: `any[]`.
#[test]
fn generic_call_argument_with_undefined_keyword_still_widens() {
    assert_inferred(
        "\
declare function echo<T>(value: T): T;
var viaCall = echo([undefined]);
var probe: string = viaCall;
",
        "any[]",
        "the `undefined` keyword must still widen through inference",
    );
}

// ── Controls: the widening the gate must keep ──

/// The bare `undefined` keyword is a widening source and must still widen.
#[test]
fn undefined_keyword_element_still_widens_to_any() {
    assert_inferred(
        "\
var direct = [undefined];
var probe: string = direct;
",
        "any[]",
        "the `undefined` keyword element must still widen to `any[]`",
    );
}

/// Nested fresh literals over a genuine widening source still widen at depth.
#[test]
fn nested_undefined_keyword_still_widens_to_any() {
    assert_inferred(
        "\
var layered = [[undefined]];
var probe: string = layered;
",
        "any[][]",
        "a nested `undefined` keyword must still widen",
    );
}

/// The object-literal arm's widening control.
#[test]
fn object_with_undefined_keyword_still_widens_to_any() {
    assert_inferred(
        "\
var carrier = { entries: [undefined] };
var probe: string = carrier;
",
        "{ entries: any[]; }",
        "an object member's `undefined` keyword must still widen",
    );
}

/// Elided holes alone are still a widening source (#16393's fixture row).
#[test]
fn elided_holes_still_widen_to_any() {
    assert_inferred(
        "\
var holes = [,,];
var probe: string = holes;
",
        "any[]",
        "elided holes must still widen to `any[]`",
    );
}

/// A hole beside a declared scalar `undefined` still does not widen — the
/// carve-out's original control, unchanged by this fix.
#[test]
fn elided_hole_with_declared_undefined_sibling_stays_unwidened() {
    assert_inferred(
        "\
var missing: undefined = undefined;
var sparse = [, missing];
var probe: string = sparse;
",
        "undefined[]",
        "a declared `undefined` sibling still makes the literal non-widening",
    );
}

/// A leaf the widener would never touch is irrelevant to the decision and must
/// not be turned into a rejection by the new fixed-point form.
#[test]
fn non_nullish_leaf_is_unaffected() {
    assert_inferred(
        "\
var words = [\"s\"];
var probe: string = words;
",
        "string[]",
        "a non-nullish leaf must be unaffected by the gate",
    );
}
