//! Regression tests for `yield*` delegation in *unannotated generator
//! declarations* (#15632, declaration-signature half).
//!
//! `tsc` folds the delegated operand's iteration yield type into the inferred
//! signature: `function* g() { yield* [1, 2, 3] }` is
//! `Generator<number, void, unknown>`. tsz collected the same contribution in
//! `dispatch/yield_.rs`, but `infer_generator_declaration_yield_type` — the
//! suppressed pre-pass that computes a *declaration*'s yield type before its
//! real body check runs — bailed on **any** `yield*` anywhere in the body, so
//! the signature fell back to `any` and every delegating generator declaration
//! silently accepted an incompatible `Generator<...>`.
//!
//! The bail exists for one real hazard, which these tests pin as negative
//! controls: when the delegate reads an *evolving* (implicit-any) binding whose
//! type depends circularly on the very aggregate being inferred
//! (`var o = []; while (true) { o = yield* o }`, TypeScript's own
//! `yieldExpressionInControlFlow.ts`), the speculative pass resolves that
//! circularity as a side effect and the later real declaration check stops
//! reporting the implicit-any diagnostics it owns. That shape is now detected
//! structurally instead of by "contains a `yield*` at all".
//!
//! Scope: every row here delegates to an array, tuple, or type-parameter
//! array, which is what `get_iterator_info`'s fast path answers. A delegate
//! that has to go through `[Symbol.iterator]` resolution — `Generator<...>`,
//! `Iterable<...>`, `Set<...>`, `string` — still contributes nothing to the
//! aggregated yield type, on the generator *expression* path as well as this
//! one, so it is a distinct root cause from the gate fixed here and is tracked
//! separately. Do not read this suite as covering that family.
//!
//! Every expectation below was pinned against `tsc` 7.0.2
//! (`--strict --target es2017`).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

/// Shared probe preamble. `wants*` are the assignability oracle: a delegating
/// generator's inferred yield type is only observable through whether its
/// `Generator<...>` is accepted by a pinned parameter type.
const PROBES: &str = r#"
export {};
declare function wantsNumber(g: Generator<number, void, unknown>): void;
declare function wantsString(g: Generator<string, void, unknown>): void;
declare function wantsBoolean(g: Generator<boolean, void, unknown>): void;
declare function wantsNumberOrString(g: Generator<number | string, void, unknown>): void;
"#;

fn probed(body: &str) -> Vec<u32> {
    strict_codes(&format!("{PROBES}{body}"))
}

#[test]
fn array_literal_delegate_infers_element_type() {
    assert!(
        probed("function* fromArray() { yield* [1, 2, 3]; }\nwantsNumber(fromArray());").is_empty(),
        "yield* over a number[] must infer Generator<number, ...>"
    );
}

#[test]
fn array_literal_delegate_rejects_wrong_yield_type() {
    // The negative half of the pair above: without it, an inferred `any` would
    // make the positive test pass for the wrong reason.
    assert_eq!(
        probed("function* fromArray() { yield* [1, 2, 3]; }\nwantsString(fromArray());"),
        vec![2345],
        "an inferred Generator<number, ...> must not satisfy Generator<string, ...>"
    );
}

#[test]
fn tuple_delegate_infers_the_element_union() {
    assert!(
        probed(
            r#"
declare const pair: [number, number];
function* fromTuple() { yield* pair; }
wantsNumber(fromTuple());
"#
        )
        .is_empty(),
        "yield* over a tuple must infer its element union"
    );
}

#[test]
fn tuple_delegate_rejects_wrong_yield_type() {
    assert_eq!(
        probed(
            r#"
declare const pair: [number, number];
function* fromTuple() { yield* pair; }
wantsString(fromTuple());
"#
        ),
        vec![2345]
    );
}

#[test]
fn renamed_binders_do_not_change_the_outcome() {
    // Anti-hardcoding control: identical shape, every binder renamed.
    assert_eq!(
        probed(
            r#"
function* qqq() {
    const zzz = [1, 2, 3];
    yield* zzz;
}
wantsString(qqq());
"#
        ),
        vec![2345]
    );
}

#[test]
fn delegate_through_a_const_binding_infers_element_type() {
    // Alias/wrapper row: the delegate is a reference, not a literal — and a
    // `const` with an initializer is *not* an evolving binding, so the pass
    // must still run.
    assert_eq!(
        probed(
            r#"
function* fromConst() {
    const items = ["a", "b"];
    yield* items;
}
wantsNumber(fromConst());
"#
        ),
        vec![2345]
    );
}

#[test]
fn mixed_plain_and_delegated_yields_union_both() {
    assert!(
        probed(
            r#"
function* mixed() { yield 1; yield* ["a"]; }
wantsNumberOrString(mixed());
"#
        )
        .is_empty(),
        "a body with both `yield 1` and `yield* string[]` must infer number | string"
    );
}

#[test]
fn mixed_plain_and_delegated_yields_reject_either_half_alone() {
    assert_eq!(
        probed(
            r#"
function* mixed() { yield 1; yield* ["a"]; }
wantsNumber(mixed());
"#
        ),
        vec![2345],
        "the delegated half must not be dropped from the union"
    );
}

#[test]
fn generic_delegate_infers_the_type_parameter() {
    assert!(
        probed(
            r#"
function* eachOf<T>(xs: T[]) { yield* xs; }
wantsBoolean(eachOf([true]));
"#
        )
        .is_empty(),
        "yield* over T[] must infer Generator<T, ...> and instantiate to boolean"
    );
}

#[test]
fn generic_delegate_rejects_wrong_instantiation() {
    assert_eq!(
        probed(
            r#"
function* eachOf<T>(xs: T[]) { yield* xs; }
wantsNumber(eachOf([true]));
"#
        ),
        vec![2345]
    );
}

#[test]
fn a_second_delegating_declaration_is_gated_independently() {
    // The gate is evaluated per generator declaration: `inner`'s own `yield*`
    // must not leak into `outer`'s decision or its inferred yield type.
    assert_eq!(
        probed(
            r#"
function* inner() { yield* [1, 2]; }
function* outer() { yield* [3, 4]; inner(); }
wantsString(outer());
"#
        ),
        vec![2345]
    );
}

#[test]
fn yield_star_inside_a_nested_function_does_not_gate_the_outer_generator() {
    // The walk must not see the inner generator's `yield*` as the outer's.
    assert_eq!(
        probed(
            r#"
function* outer() {
    yield 1;
    function* nested() { yield* ["a"]; }
    nested();
}
wantsString(outer());
"#
        ),
        vec![2345]
    );
}

#[test]
fn evolving_array_self_delegation_still_reports_implicit_any() {
    // NEGATIVE CONTROL — the shape the bail exists for, reduced from
    // TypeScript's own `yieldExpressionInControlFlow.ts`. `o` is an evolving
    // binding whose type depends on the yield* aggregate that reads it; the
    // pre-pass must keep bailing here so the real declaration check still owns
    // the implicit-any family.
    let codes = probed(
        r#"
function* circular() {
    var o = [];
    while (true) {
        o = yield* o;
    }
}
"#,
    );
    assert!(
        !codes.is_empty(),
        "the circular evolving-binding delegate must still report its implicit-any family, got {codes:?}"
    );
}

#[test]
fn evolving_array_self_delegation_is_unaffected_by_binder_names() {
    // Same negative control with every binder renamed: the guard is structural,
    // not name-shaped.
    let codes = probed(
        r#"
function* zzz() {
    var qqq = [];
    while (true) {
        qqq = yield* qqq;
    }
}
"#,
    );
    assert!(
        !codes.is_empty(),
        "renaming the evolving binding must not change the guard, got {codes:?}"
    );
}

#[test]
fn a_non_evolving_delegate_beside_an_evolving_one_still_bails() {
    // Fallback row: the guard is per-generator, not per-`yield*`. One evolving
    // delegate anywhere in the body keeps the whole pre-pass off, which is the
    // conservative direction.
    let codes = probed(
        r#"
function* both() {
    var o = [];
    yield* [1, 2];
    while (true) {
        o = yield* o;
    }
}
"#,
    );
    assert!(
        !codes.is_empty(),
        "an evolving delegate anywhere in the body keeps the bail, got {codes:?}"
    );
}
