//! Regression tests for #16384 leg B: the mutable-binding non-strict nullish
//! widen must gate on the initializer's own widening provenance, not merely
//! on freshness.
//!
//! Structural rule: with `strictNullChecks` off, tsc's `null`/`undefined`-to-
//! `any` widening (`nullWideningType`/`undefinedWideningType` reaching
//! `getWidenedType`) is a property of the *expression*, not the type. Only a
//! bare `null`/`undefined` keyword, or an identifier resolving to the global
//! `undefined`, carries that flavour. `widen_initializer_type_for_mutable_binding`
//! previously called `widen_nullish_to_any_deep` unconditionally whenever the
//! initializer was a fresh literal, so a *declared* `undefined` value flowing
//! through a fresh array/object literal (`declare var q: undefined; var av = [q];`)
//! was wrongly widened to `any[]` — a false negative that silently drops later
//! `TS2345`s on the binding. Fixed at the checker's fresh-literal widening
//! entry point (`CheckerState::widen_initializer_type_for_mutable_binding_gated`,
//! `types/utilities/widening.rs`), gated by the new provenance walk in
//! `types/utilities/mutable_binding_nullish.rs`.
//!
//! Every row below is pinned against a real `typescript@7.0.2` oracle,
//! `--target es2015 --strict false --noImplicitAny false`.

use crate::test_utils::{check_with_options_code_messages, non_strict_checker_options};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

/// The reported repro: a declared (non-widening-source) `undefined` value
/// inside a fresh array literal keeps `undefined[]` — tsc: `undefined[]`.
#[test]
fn declared_undefined_element_keeps_array_unwidened() {
    let source = "\
declare var q: undefined;
var av = [q];
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'undefined[]' is not assignable to type 'string'.".to_string()
        )],
        "declared `undefined` element must not widen the array to `any[]`: {messages:?}"
    );
}

/// Control: the real `undefined` keyword IS a widening source and must still
/// widen the whole array to `any[]` — tsc: `any[]`.
#[test]
fn undefined_keyword_element_widens_array_to_any() {
    let source = "\
var av = [undefined];
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'any[]' is not assignable to type 'string'.".to_string()
        )],
        "the `undefined` keyword element must still widen to `any[]`: {messages:?}"
    );
}

/// Control: repeated global-`undefined` identifier elements still widen —
/// the gate must not be an "exactly one leaf" special case.
#[test]
fn repeated_undefined_keyword_elements_widen_to_any() {
    let source = "\
var av = [undefined, undefined];
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'any[]' is not assignable to type 'string'.".to_string()
        )],
        "repeated `undefined` elements must still widen to `any[]`: {messages:?}"
    );
}

/// Control: a local PARAMETER named `undefined` shadows the global and is
/// not a widening source, so the array keeps `undefined[]` — tsc:
/// `undefined[]`. Exercises `is_global_undefined_identifier`'s shadowing
/// check through the new gate, not just the `UndefinedKeyword` token arm.
#[test]
fn shadowed_local_named_undefined_does_not_widen() {
    let source = "\
function f(undefined: undefined) {
  var av = [undefined];
  var e: string = av;
}
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'undefined[]' is not assignable to type 'string'.".to_string()
        )],
        "a local parameter named `undefined` must not be treated as the widening sentinel: {messages:?}"
    );
}

/// Same rule for a fresh object literal: a declared `undefined` property
/// value keeps its type — tsc: `{ p: undefined; }`.
#[test]
fn declared_undefined_property_value_not_widened() {
    let source = "\
declare var q: undefined;
var av = { p: q };
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type '{ p: undefined; }' is not assignable to type 'string'.".to_string()
        )],
        "declared `undefined` property value must not widen: {messages:?}"
    );
}

/// Control: a literal `undefined` object property value still widens —
/// tsc: `{ p: any; }`.
#[test]
fn undefined_keyword_property_value_widens_to_any() {
    let source = "\
var av = { p: undefined };
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type '{ p: any; }' is not assignable to type 'string'.".to_string()
        )],
        "the `undefined` keyword property value must still widen to `any`: {messages:?}"
    );
}

/// No nullish leaf at all: an ordinary numeric array is unaffected by the
/// gate either way.
#[test]
fn array_without_nullish_leaf_is_unaffected() {
    let source = "\
var av = [1, 2, 3];
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'number[]' is not assignable to type 'string'.".to_string()
        )],
    );
}

/// The gate is non-strict-only: with `strictNullChecks` on, tsc never widens
/// null/undefined regardless of provenance, and tsz's existing strict-mode
/// behavior (already correct) must be untouched by this change.
#[test]
fn strict_null_checks_on_keeps_undefined_array_unwidened_for_both_sources() {
    use crate::test_utils::{check_with_options_code_messages, strict_checker_options};

    let declared_source = "\
declare var q: undefined;
var av = [q];
var e: string = av;
";
    let keyword_source = "\
var av = [undefined];
var e: string = av;
";
    for source in [declared_source, keyword_source] {
        let messages = check_with_options_code_messages(source, strict_checker_options());
        assert_eq!(
            messages,
            vec![(
                2322,
                "Type 'undefined[]' is not assignable to type 'string'.".to_string()
            )],
            "strictNullChecks must keep `undefined[]` unwidened regardless of source: {messages:?}"
        );
    }
}

/// KNOWN GAP, not fixed by this PR: a declared `undefined` nested inside a
/// fresh array-of-arrays still widens to `any[][]`, because
/// `get_type_of_array_literal_with_request`'s best-common-type pre-widening
/// (`types/computation/array_literal.rs`, the `bct_element_types` map) calls
/// `widen_nullish_to_any_deep` on each compound element unconditionally,
/// before the initializer ever reaches the mutable-binding widening seam this
/// PR gates. tsc keeps `undefined[][]`. Fixing this needs the same provenance
/// gate threaded through that BCT computation, which does not carry a 1:1
/// element-index-to-AST-node mapping today (`element_nodes` is populated only
/// for the direct-expression push site, not the hole/spread/rest ones) — a
/// distinct owner-site fix, left for a follow-up.
#[test]
fn declared_undefined_in_nested_array_keeps_unwidened() {
    let source = "\
declare var q: undefined;
var av = [[q]];
var e: string = av;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'undefined[][]' is not assignable to type 'string'.".to_string()
        )],
    );
}

/// An assignment expression (`x = y`) evaluates to its RHS value, so its
/// freshness and widening provenance are the RHS's, not the assignment
/// node's own (`check_assignment_expression` already returns `right_type`
/// for the same reason). Reduced from `wideningTuples4.ts`: `b`'s initializer
/// is the assignment `a = [undefined, null]`, not a direct array literal —
/// `is_fresh_literal_expression_inner` and the nullish-widening provenance
/// walks must unwrap it the same way they unwrap a parenthesized expression,
/// or `b` keeps the unwidened `[undefined, null]` tuple instead of widening
/// to `[any, any]`. tsc: `b: [any, any]`, so `b = ["", ""]` is clean.
#[test]
fn assignment_expression_initializer_widens_through_rhs() {
    let source = "\
var a: [any];
var b = a = [undefined, null];
b = [\"\", \"\"];
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type '[undefined, null]' is not assignable to type '[any]'.".to_string()
        )],
        "b must widen to [any, any] through the assignment RHS, matching tsc: {messages:?}"
    );
}

/// Control: a *declared* (non-widening) value flowing through the same
/// assignment-expression shape must NOT widen — mirrors
/// `declared_undefined_element_keeps_array_unwidened` but through an
/// assignment initializer instead of a direct array literal.
#[test]
fn assignment_expression_initializer_with_declared_value_stays_unwidened() {
    let source = "\
declare var q: undefined;
var a: [any];
var b = a = [q];
var e: string = b;
";
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type '[undefined]' is not assignable to type 'string'.".to_string()
        )],
        "a declared `undefined` reaching through an assignment RHS must not widen: {messages:?}"
    );
}
