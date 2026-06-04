//! Regression coverage for variadic tuple inference through a tuple-typed rest
//! parameter that has fixed elements on *both* sides of the variadic part.
//!
//! Structural rule: a tuple-typed rest parameter is, to `tsc`, just a tuple
//! parameter. When a call's trailing arguments are matched against
//! `...args: [pre…, ...Mid, post…]`, `tsc` packs every trailing argument into a
//! single source tuple and runs `inferFromTupleTypes`, so the fixed prefix, the
//! single variadic middle, *and* the fixed suffix each receive candidates with
//! the correct arity and element positions.
//!
//! Before the fix, tsz only inferred the middle variadic slice: a fixed suffix
//! type parameter (`L` in `[H, ...M, L]`) was never inferred and fell back to
//! `unknown`, and a fixed prefix type parameter (`H`) was dropped whenever a
//! fixed suffix was present. Each case below pins behavior with an assignability
//! witness — the exact type `tsc` produces must NOT raise `TS2322`, and a
//! deliberately wrong target type MUST raise exactly one `TS2322`. Binder
//! spellings are varied so the behavior is structural, not name-keyed.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_only_one_2322(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "{label}: expected exactly one TS2322, got {codes:?}"
    );
}

// =============================================================================
// Fixed prefix + variadic middle + fixed suffix: [H, ...M, L]
// =============================================================================

#[test]
fn prefix_middle_suffix_type_params_all_inferred() {
    assert_no_errors(
        r#"
declare function f<H, M extends unknown[], L>(...args: [H, ...M, L]): [H, M, L];
const r = f("a", 1, true);
const ok: [string, [number], boolean] = r;
"#,
        "[H, ...M, L]: H=string, M=[number], L=boolean",
    );
}

#[test]
fn prefix_middle_suffix_rejects_wrong_suffix_type() {
    // The suffix `L` must actually be inferred (regression: it used to be
    // `unknown`, which silently accepted any target).
    assert_only_one_2322(
        r#"
declare function f<H, M extends unknown[], L>(...args: [H, ...M, L]): [H, M, L];
const r = f("a", 1, true);
const bad: [string, [number], string] = r;
"#,
        "wrong suffix element type must be a TS2322",
    );
}

#[test]
fn prefix_middle_suffix_rejects_wrong_prefix_type() {
    assert_only_one_2322(
        r#"
declare function f<H, M extends unknown[], L>(...args: [H, ...M, L]): [H, M, L];
const r = f("a", 1, true);
const bad: [number, [number], boolean] = r;
"#,
        "wrong prefix element type must be a TS2322",
    );
}

#[test]
fn prefix_middle_suffix_longer_middle_keeps_arity() {
    assert_no_errors(
        r#"
declare function f<H, M extends unknown[], L>(...args: [H, ...M, L]): [H, M, L];
const r = f("a", 1, 2, 3, true);
const ok: [string, [number, number, number], boolean] = r;
"#,
        "longer middle: M=[number, number, number]",
    );
}

#[test]
fn prefix_middle_suffix_empty_middle_is_empty_tuple() {
    assert_no_errors(
        r#"
declare function f<H, M extends unknown[], L>(...args: [H, ...M, L]): [H, M, L];
const r = f("a", true);
const ok: [string, [], boolean] = r;
"#,
        "no middle arguments: M=[]",
    );
}

#[test]
fn prefix_middle_suffix_renamed_binders() {
    // Proves the fix is structural, not keyed to the spellings H/M/L.
    assert_no_errors(
        r#"
declare function combine<Head, Mid extends unknown[], Tail>(
    ...xs: [Head, ...Mid, Tail]
): [Head, Mid, Tail];
const r = combine("h", 1, 2, "t");
const ok: [string, [number, number], string] = r;
"#,
        "renamed binders behave identically",
    );
}

// =============================================================================
// Leading variadic + fixed suffix type parameter(s): [...I, L]
// =============================================================================

#[test]
fn leading_variadic_with_suffix_type_param_inferred() {
    assert_no_errors(
        r#"
declare function f<I extends unknown[], L>(...args: [...I, L]): [I, L];
const r = f("a", 1, true);
const ok: [[string, number], boolean] = r;
"#,
        "[...I, L]: I=[string, number], L=boolean",
    );
}

#[test]
fn leading_variadic_with_suffix_rejects_wrong_suffix() {
    assert_only_one_2322(
        r#"
declare function f<I extends unknown[], L>(...args: [...I, L]): [I, L];
const r = f("a", 1, true);
const bad: [[string, number], string] = r;
"#,
        "wrong suffix element type must be a TS2322",
    );
}

#[test]
fn leading_variadic_with_two_fixed_suffix_params() {
    assert_no_errors(
        r#"
declare function f<I extends unknown[], P, Q>(...args: [...I, P, Q]): [I, P, Q];
const r = f(1, 2, "x", true);
const ok: [[number, number], string, boolean] = r;
"#,
        "[...I, P, Q]: I=[number, number], P=string, Q=boolean",
    );
}

// =============================================================================
// Multiple fixed prefix elements before the variadic: [A, B, ...M, L]
// =============================================================================

#[test]
fn two_prefix_elements_with_middle_and_suffix() {
    assert_no_errors(
        r#"
declare function f<A, B, M extends unknown[], L>(...args: [A, B, ...M, L]): [A, B, M, L];
const r = f("a", 1, true, false, {}, 9);
const ok: [string, number, [boolean, boolean, {}], number] = r;
"#,
        "[A, B, ...M, L]: prefix, middle, and suffix all inferred",
    );
}

// =============================================================================
// Guard: two adjacent variadic type parameters still fall back to constraints
// =============================================================================
//
// `[...A, ...B]` has no implied arity to split on, so `tsc` infers nothing and
// both `A` and `B` default to their constraint (`unknown[]`). The broad fix must
// preserve this: it must NOT over-infer a concrete arity by greedily packing the
// arguments into the first variadic.

#[test]
fn two_adjacent_variadics_fall_back_to_constraint() {
    assert_no_errors(
        r#"
declare function split<A extends unknown[], B extends unknown[]>(
    ...xs: [...A, ...B]
): [A, B];
const r = split("a", 1, true);
const ok: [unknown[], unknown[]] = r;
"#,
        "two adjacent variadics: A=B=unknown[]",
    );
}

#[test]
fn two_adjacent_variadics_do_not_over_infer() {
    assert_only_one_2322(
        r#"
declare function split<A extends unknown[], B extends unknown[]>(
    ...xs: [...A, ...B]
): [A, B];
const r = split("a", 1, true);
const bad: [[string, number, boolean], []] = r;
"#,
        "over-inferring a concrete arity for an unsplittable variadic is wrong",
    );
}

// =============================================================================
// Fixed prefix params before the rest parameter stay positional
// =============================================================================

#[test]
fn fixed_param_before_variadic_rest_tuple() {
    // A normal fixed parameter ahead of the rest parameter must still be
    // inferred positionally while the rest tuple distributes the trailing args.
    assert_no_errors(
        r#"
declare function f<F, M extends unknown[], L>(first: F, ...rest: [...M, L]): [F, M, L];
const r = f("first", 1, 2, true);
const ok: [string, [number, number], boolean] = r;
"#,
        "fixed leading param + [...M, L] rest tuple",
    );
}
