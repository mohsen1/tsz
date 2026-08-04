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

/// Control — a mixed array keeps its non-nullish member and widens only the
/// nullish one. tsc: `(string | any)[]`, which prints as `any[]` after union
/// absorption on both sides.
#[test]
fn mixed_array_widens_only_the_nullish_leaf() {
    let messages = nonstrict_messages(
        "\
declare function id<T>(x: T): T;
var v = id([\"s\", undefined]);
var e: string = v;
",
    );
    assert_eq!(messages.len(), 1, "expected exactly one diagnostic: {messages:?}");
    assert_eq!(messages[0].0, 2322);
    assert!(
        !messages[0].1.contains("undefined"),
        "the `undefined` leaf must not survive inference: {messages:?}"
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
