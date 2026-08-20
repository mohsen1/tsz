//! Regression tests for #16384 leg A: the non-strict nullish widening flavour
//! must survive generic-call inference.
//!
//! Structural rule: with `strictNullChecks` off, tsc gives the `null`/`undefined`
//! keyword the widening flavour (`nullWideningType`/`undefinedWideningType`) and
//! `getInferredType` ends in `getWidenedType`, so a widening-flavoured argument
//! widens its nullish leaves to `any` *on the way into the substitution*:
//! `declare function id<T>(x: T): T; var v = id([undefined]);` infers `any[]`.
//! tsz inferred `undefined[]`, which then rejected every later assignment to the
//! binding with a false `TS2322` — the remaining half of the `wideningTuples`
//! conformance pair (`conformance/types/tuple/wideningTuples1.ts`).
//!
//! The variable-declaration seam cannot own this: it gates the deep widen on
//! `is_fresh_literal_expression(initializer)` and a call expression is never
//! fresh. The owner is the candidate seam —
//! `CheckerState::sanitize_generic_inference_arg_types`
//! (`types/computation/call_inference.rs`), which is where the checker already
//! rewrites argument types expression-awarely before handing them to the solver
//! — gated by `fresh_literal_argument_nullish_leaves_are_widening`
//! (`types/utilities/mutable_binding_nullish.rs`), reusing leg B's provenance
//! walk.
//!
//! The discriminator is "did the result come from *inferring* a type parameter
//! whose candidate was a widening-flavoured argument", not "is the result typed
//! `undefined`". The controls below are what make that discriminating rather
//! than a blanket widen: a declared `undefined` value, a declared
//! `undefined[]`-returning signature, and a strict-mode run must all keep
//! `undefined`.
//!
//! Every row below is pinned against a real `typescript@7.0.2` oracle,
//! `--target es2015 --strict false --noImplicitAny false` (and `--strict` for
//! the strict-mode control).
//!
//! The last group covers `widenedTypes/arrayLiteralWidened.ts` and belongs to
//! the *mutable-binding* seam (leg B), not the candidate seam. It lives here
//! because it was found and fixed from this branch, and because both seams now
//! share one provenance walk — a change to that walk has to be read against
//! both sets at once, which is exactly the coupling that let the elision hole
//! regress unnoticed.

use crate::test_utils::{
    check_with_options_code_messages, non_strict_checker_options, strict_checker_options,
};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, strict_checker_options())
}

/// Probe the inferred type by forcing it into a `TS2322` that prints it. A
/// `.d.ts` probe is NOT a valid proxy on this family — the DTS path already
/// emits `any[]` here while the checker disagreed (#16384).
fn assert_infers(source: &str, rendered: &str) {
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            format!("Type '{rendered}' is not assignable to type 'string'.")
        )],
        "expected the binding to infer `{rendered}`: {messages:?}"
    );
}

/// The conformance row itself (`wideningTuples1.ts`): tsc reports nothing,
/// because `T` infers as `[any]` and `[""]` is assignable to it.
#[test]
fn widening_tuples_row_reports_nothing() {
    let source = "\
declare function foo<T extends [any]>(x: T): T;
var y = foo([undefined]);
y = [\"\"];
";
    let messages = nonstrict_messages(source);
    assert!(
        messages.is_empty(),
        "wideningTuples1 must be clean; the constrained tuple candidate widens to `[any]`: {messages:?}"
    );
}

/// `declare function id<T>(x: T): T; var v = id([undefined]);` — tsc: `any[]`.
#[test]
fn bare_type_parameter_candidate_widens_to_any_array() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id([undefined]);
var e: string = v;
",
        "any[]",
    );
}

/// The array-shaped parameter reaches the same answer through element-wise
/// inference (`T` infers `any`, not `undefined`) — tsc: `any[]`.
#[test]
fn array_shaped_parameter_candidate_widens_to_any_array() {
    assert_infers(
        "\
declare function idn<T>(x: T[]): T[];
var v = idn([undefined]);
var e: string = v;
",
        "any[]",
    );
}

/// The constrained tuple form, read as a type rather than as the row's
/// assignment — tsc: `[any]`.
#[test]
fn constrained_tuple_candidate_widens_to_any_tuple() {
    assert_infers(
        "\
declare function foo<T extends [any]>(x: T): T;
var v = foo([undefined]);
var e: string = v;
",
        "[any]",
    );
}

/// `null` carries the same widening flavour as `undefined` — tsc: `any[]`.
#[test]
fn null_keyword_candidate_widens_to_any_array() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id([null]);
var e: string = v;
",
        "any[]",
    );
}

/// A fresh object literal propagates the flavour through its property values —
/// tsc: `{ p: any; }`.
#[test]
fn object_literal_candidate_widens_property_to_any() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id({ p: undefined });
var e: string = v;
",
        "{ p: any; }",
    );
}

/// Nested fresh literals widen at every depth the walk accounts for —
/// tsc: `any[][]`.
#[test]
fn nested_array_literal_candidate_widens_to_any_array_of_arrays() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id([[undefined]]);
var e: string = v;
",
        "any[][]",
    );
}

/// Anti-hardcoding: the fix is structural, so renaming the signature, its type
/// parameter, its parameter and the binding changes nothing — tsc: `any[]`.
#[test]
fn renamed_binders_widen_identically() {
    assert_infers(
        "\
declare function wrapValue<Element>(payload: Element): Element;
var collected = wrapValue([undefined]);
var e: string = collected;
",
        "any[]",
    );
}

/// Control — a *declared* `undefined` value is not a widening source, so the
/// candidate keeps `undefined[]` even though the argument is a fresh literal.
/// tsc: `undefined[]`. This is the row that rules out a blanket widen.
#[test]
fn declared_undefined_element_keeps_undefined_array_through_inference() {
    assert_infers(
        "\
declare var q: undefined;
declare function id<T>(x: T): T;
var v = id([q]);
var e: string = v;
",
        "undefined[]",
    );
}

/// Control — a non-generic signature whose return type is *declared*
/// `undefined[]` never builds its result from a candidate, so it must keep
/// reporting `undefined[]`. tsc: `undefined[]`.
#[test]
fn declared_undefined_array_return_type_is_unaffected() {
    assert_infers(
        "\
declare function ida(x: undefined[]): undefined[];
var v = ida([undefined]);
var e: string = v;
",
        "undefined[]",
    );
}

/// Control — an identifier argument carries no recoverable flavour, so a
/// declared `undefined[]` passed through an identity call keeps its type.
/// tsc: `undefined[]`.
#[test]
fn declared_undefined_array_identifier_argument_is_unaffected() {
    assert_infers(
        "\
declare var qa: undefined[];
declare function id<T>(x: T): T;
var v = id(qa);
var e: string = v;
",
        "undefined[]",
    );
}

/// Control — a non-nullish candidate is untouched: the deep widen only maps
/// nullish leaves, so ordinary literal widening is unchanged. tsc: `number[]`.
#[test]
fn non_nullish_candidate_is_unchanged() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id([1]);
var e: string = v;
",
        "number[]",
    );
}

/// A mixed non-nullish/nullish array. tsc says `string[]` — it reaches that
/// through non-strict **union reduction** (`undefined` is absorbed out of
/// `string | undefined` when `strictNullChecks` is off), not through the
/// widening flavour this file otherwise covers. First fixed by #16574 on the
/// array-literal path; the reduction now lives at the solver union-construction
/// seam (`TypeInterner::reduce_nonstrict_nullish_members`, #16580), which this
/// row continues to witness.
#[test]
fn mixed_array_reduces_undefined_out_of_the_element_union() {
    assert_infers(
        "\
declare function id<T>(x: T): T;
var v = id([\"s\", undefined]);
var e: string = v;
",
        "string[]",
    );
}

/// Control — the whole rule is `strictNullChecks`-off only. Under `strict`,
/// `undefined` has no widening flavour and the inferred type must stay
/// `undefined[]`. tsc (`--strict`): `undefined[]`.
#[test]
fn strict_mode_keeps_undefined_through_inference() {
    let messages = strict_messages(
        "\
declare function id<T>(x: T): T;
var v = id([undefined]);
var e: string = v;
",
    );
    assert_eq!(
        messages,
        vec![(
            2322,
            "Type 'undefined[]' is not assignable to type 'string'.".to_string()
        )],
        "strict mode must not widen a nullish candidate: {messages:?}"
    );
}

/// The full `types/typeRelationships/widenedTypes/arrayLiteralWidened.ts`
/// conformance row, which #16387 regressed to `TS2403` and #16393 recorded as
/// un-bisected. The culprit was never `[null, null]` (already `any[]`) — it was
/// the ELIDED element `[,,]`, which parses as `NodeIndex::NONE` and so fell
/// into the provenance walk's node-lookup guard and failed closed, leaving
/// `undefined[]` where tsc says `any[]`.
///
/// The fixture's own last section is the control that rules out "treat every
/// hole as widening and stop there": `var x: undefined = undefined` is a
/// non-widening element, and tsc's comment states the rule outright — *no
/// widening when one or more elements are non-widening* — so `[, x]` must keep
/// `undefined[]` even though it contains a hole.
#[test]
fn array_literal_widened_conformance_row_is_clean() {
    let source = "\
var a = [];
var a = [,,];
var a = [null, null];
var a = [undefined, undefined];
";
    let messages = nonstrict_messages(source);
    assert!(
        messages.is_empty(),
        "every declaration must widen to `any[]`, so no TS2403: {messages:?}"
    );
}

/// The elided element alone — tsc: `any[]`.
#[test]
fn elided_element_is_a_widening_source() {
    assert_infers(
        "\
var a = [,,];
var e: string = a;
",
        "any[]",
    );
}

/// A hole beside a `null` keyword still widens — tsc: `any[]`.
#[test]
fn elided_element_beside_null_keyword_widens() {
    assert_infers(
        "\
var a = [, null];
var e: string = a;
",
        "any[]",
    );
}

/// Control from the fixture's own last section: one non-widening element makes
/// the whole literal non-widening, hole or not — tsc: `undefined[]`.
#[test]
fn elided_element_beside_declared_undefined_does_not_widen() {
    assert_infers(
        "\
var x: undefined = undefined;
var d = [, x];
var e: string = d;
",
        "undefined[]",
    );
}

/// Same control without the hole, pinning that the `all` semantics — not the
/// hole arm — is what declines here. tsc: `undefined[]`.
#[test]
fn declared_undefined_beside_undefined_keyword_does_not_widen() {
    assert_infers(
        "\
var x: undefined = undefined;
var d = [undefined, x];
var e: string = d;
",
        "undefined[]",
    );
}
