//! Regression tests for #12176 — variadic tuple tail inference must preserve
//! rest length and element positions, matching `tsc`'s `inferFromTupleTypes`.
//!
//! Structural rule: when inferring through `[H, ...Tail]`, `[...Init, L]`,
//! `[H, ...Mid, L]`, or `[...A, ...B]`, the arity of each variadic/rest segment
//! is preserved. Two adjacent variadic type parameters are split only when an
//! implied arity is available (a bare `...rest: T` rest parameter, or a fixed
//! constraint); otherwise neither is inferred and both fall back to their
//! constraints — exactly as `tsc` does.
//!
//! Each test pins behavior with an assignability witness: a correct target type
//! must NOT produce `TS2322`, and a deliberately wrong target type (one extra or
//! one missing element, or a dropped arity) MUST produce `TS2322`. The rule is
//! structural, so binder spellings are varied to guard against name-keyed logic.

use crate::test_utils::{check_source_diagnostics, diagnostics_with_code};

fn ts2322_count(source: &str) -> usize {
    diagnostics_with_code(&check_source_diagnostics(source), 2322).len()
}

/// Partial application (`bind`): the callback's parameter tuple `[...T, ...U]`
/// is split by the implied arity of the bound `...args: T`, so the returned
/// signature keeps the trailing parameters' arity (`U = [number, boolean]`).
#[test]
fn bind_preserves_tail_parameter_arity() {
    let ok = r#"
declare function bind<T extends unknown[], U extends unknown[], R>(
    fn: (...a: [...T, ...U]) => R,
    ...b: T
): (...r: U) => R;
declare const fn3: (a: string, b: number, c: boolean) => void;
const partial = bind(fn3, "x");
const ok: (b: number, c: boolean) => void = partial;
"#;
    assert_eq!(ts2322_count(ok), 0, "bind tail arity should match tsc");
}

/// A wrong-arity target for the same `bind` result must be rejected.
#[test]
fn bind_tail_arity_rejects_wrong_signature() {
    let bad = r#"
declare function bind<T extends unknown[], U extends unknown[], R>(
    fn: (...a: [...T, ...U]) => R,
    ...b: T
): (...r: U) => R;
declare const fn3: (a: string, b: number, c: boolean) => void;
const partial = bind(fn3, "x");
const bad: (b: boolean) => void = partial;
"#;
    assert_eq!(ts2322_count(bad), 1, "wrong tail arity must be a TS2322");
}

/// More bound arguments shift the split point: `bind(fn3, "x", 1)` leaves
/// `U = [boolean]`, so a single-parameter target signature is correct.
#[test]
fn bind_split_point_follows_bound_argument_count() {
    let ok = r#"
declare function bind<T extends unknown[], U extends unknown[], R>(
    fn: (...a: [...T, ...U]) => R,
    ...b: T
): (...r: U) => R;
declare const fn3: (a: string, b: number, c: boolean) => void;
const partial = bind(fn3, "x", 1);
const ok: (c: boolean) => void = partial;
"#;
    assert_eq!(
        ts2322_count(ok),
        0,
        "split point must follow bound arg count"
    );
}

/// Two adjacent variadic type parameters in a tuple-typed rest parameter have
/// no implied arity, so `tsc` infers nothing and both default to their
/// constraint (`unknown[]`). tsz must not over-infer a concrete arity.
#[test]
fn two_adjacent_variadics_without_implied_arity_fall_back_to_constraint() {
    let ok = r#"
declare function split<A extends unknown[], B extends unknown[]>(
    ...xs: [...A, ...B]
): [A, B];
const r = split(1, 2, 3);
const ok: [unknown[], unknown[]] = r;
"#;
    assert_eq!(
        ts2322_count(ok),
        0,
        "split must yield [unknown[], unknown[]]"
    );
}

/// The flip side: tsz must not silently infer `A = [number, number, number]`.
#[test]
fn two_adjacent_variadics_do_not_over_infer_arity() {
    let bad = r#"
declare function split<A extends unknown[], B extends unknown[]>(
    ...xs: [...A, ...B]
): [A, B];
const r = split(1, 2, 3);
const bad: [[number, number, number], []] = r;
"#;
    assert_eq!(
        ts2322_count(bad),
        1,
        "over-inferring a concrete arity for an unsplittable variadic is wrong"
    );
}

/// Fixed prefix + single variadic middle + fixed suffix: the middle keeps its
/// exact arity (`M = [boolean, {}]`).
#[test]
fn prefix_variadic_suffix_preserves_middle_arity() {
    let ok = r#"
declare function mid<M extends unknown[]>(...xs: [string, ...M, number]): M;
const m = mid("s", true, {}, 3);
const ok: [boolean, {}] = m;
"#;
    assert_eq!(ts2322_count(ok), 0, "middle arity must be a 2-tuple");
}

/// An off-by-one target for the middle slice must be rejected.
#[test]
fn prefix_variadic_suffix_rejects_wrong_middle_arity() {
    let bad = r#"
declare function mid<M extends unknown[]>(...xs: [string, ...M, number]): M;
const m = mid("s", true, {}, 3);
const bad: [boolean, {}, number] = m;
"#;
    assert_eq!(ts2322_count(bad), 1, "wrong middle arity must be a TS2322");
}

/// The fix is structural, not keyed to the spellings `T`/`U`/`R`. Renamed
/// binders must behave identically to `bind_preserves_tail_parameter_arity`.
#[test]
fn renamed_binders_preserve_tail_arity() {
    let ok = r#"
declare function applyHead<Head extends unknown[], Tail extends unknown[], Ret>(
    cb: (...p: [...Head, ...Tail]) => Ret,
    ...pre: Head
): (...post: Tail) => Ret;
declare const handler: (a: string, b: number, c: boolean) => void;
const partial = applyHead(handler, "x");
const ok: (b: number, c: boolean) => void = partial;
"#;
    assert_eq!(ts2322_count(ok), 0, "behavior must be name-independent");
}

/// Leading variadic + fixed suffix (`[...Init, L]`): the head keeps its arity.
#[test]
fn leading_variadic_with_fixed_suffix_preserves_arity() {
    let ok = r#"
declare function init<I extends unknown[]>(...xs: [...I, () => void]): I;
const r = init(1, "a", true, () => {});
const ok: [number, string, boolean] = r;
"#;
    assert_eq!(
        ts2322_count(ok),
        0,
        "leading variadic arity must be preserved"
    );
}
