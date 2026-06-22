//! A non-tuple array spread argument must select an overload with a rest
//! parameter even when a *fixed-arity* overload is declared first.
//!
//! Regression for #14319 (es-toolkit `flow`): with `flow(f: () => any)`
//! declared before `flow(...funcs: (() => any)[])`, calling `flow(...arr)` for a
//! non-tuple `arr: (() => any)[]` emitted a spurious TS2556. The non-tuple
//! spread collapsed to a single positional argument that satisfied the
//! fixed-arity overload, which was then accepted; the post-success spread
//! validation flagged the spread against that (wrong) overload instead of
//! letting overload resolution fall through to the rest overload.
//!
//! Structural rule (one sentence):
//!
//! > When an overloaded call spreads a non-tuple array/iterable, an overload
//! > that cannot absorb the spread at a rest (or optional-tail) position is a
//! > *soft* failure — overload resolution keeps scanning for a later overload
//! > with a rest parameter, and TS2556 is committed only if no overload
//! > matches; tsz now mirrors tsc's `chooseOverload` here.
//!
//! Array types use the `T[]` shorthand (built-in) rather than the `Array<T>`
//! global so the cases resolve without the full lib in the unit harness. The
//! lib-backed `Array<…>`/`Set<…>` (iterable) forms from the es-toolkit witness
//! are verified directly against the `tsz` binary.
//!
//! Every test varies user-chosen names so the fix is structural, not name-keyed.

use tsz_checker::test_utils::check_source_code_messages;

const TS2556: u32 = 2556;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|c| *c != 0)
        .collect()
}

// ───────────────────────── 1. reported repro ──────────────────────────────

#[test]
fn nontuple_spread_selects_rest_overload_after_fixed() {
    let src = r#"
declare function flow(f: () => any): any;
declare function flow(...funcs: (() => any)[]): any;
function f(arr: (() => any)[]): any { return flow(...arr); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "non-tuple spread must pick the rest overload declared after the fixed one (no TS2556)"
    );
}

// ───────────────────── 2. binder-name variation ───────────────────────────

#[test]
fn nontuple_spread_rest_overload_renamed_binders() {
    let src = r#"
declare function pipe(handler: (x: number) => number): number;
declare function pipe(...handlers: ((x: number) => number)[]): number;
function run(hs: ((x: number) => number)[]): number { return pipe(...hs); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "renamed fixed-then-rest overloads: non-tuple spread must select the rest overload"
    );
}

// ───────────────────── 3. adjacent positive cases ─────────────────────────

/// Reversed declaration order (rest overload first) already worked; keep it green.
#[test]
fn nontuple_spread_rest_overload_declared_first() {
    let src = r#"
declare function rf(...funcs: (() => any)[]): any;
declare function rf(f: () => any): any;
function f(arr: (() => any)[]): any { return rf(...arr); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "rest overload first must still accept the spread"
    );
}

/// The fixed overload's parameter *type* mismatches the spread element type, so
/// it fails as an `ArgumentTypeMismatch` rather than a spurious success — the
/// soft-failure deferral must still fall through to the rest overload.
#[test]
fn nontuple_spread_skips_typed_fixed_overload_for_rest() {
    let src = r#"
declare function k(a: string, b: string): any;
declare function k(...rest: number[]): any;
function f(arr: number[]): any { return k(...arr); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a type-mismatching fixed overload must defer to the number-rest overload"
    );
}

/// A single-parameter fixed overload whose parameter *type* mismatches the
/// spread element type fails as an `ArgumentTypeMismatch` at the same arity the
/// collapsed spread presents — the deferral must still fall through to the
/// rest overload rather than committing TS2556 against the fixed one.
#[test]
fn nontuple_spread_single_param_type_mismatch_defers_to_rest() {
    let src = r#"
declare function k1(a: string): any;
declare function k1(...rest: number[]): any;
function f(arr: number[]): any { return k1(...arr); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a single-param type-mismatching fixed overload must defer to the number-rest overload"
    );
}

/// The rest parameter sits *after* fixed parameters (splice-shaped): the spread
/// must still be admitted by that overload.
#[test]
fn nontuple_spread_into_trailing_rest_after_fixed_params() {
    let src = r#"
declare function ins(head: number, mid?: number): void;
declare function ins(head: number, ...tail: number[]): void;
function f(rest: number[]): void { ins(1, ...rest); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a spread landing on a trailing rest after fixed params must be admitted"
    );
}

// ───────────────────────── 4. negative controls ───────────────────────────

/// All overloads are fixed-arity: no rest overload can absorb the non-tuple
/// spread, so TS2556 must still be reported (the deferral commits the error
/// after the loop).
#[test]
fn nontuple_spread_all_fixed_overloads_still_ts2556() {
    let src = r#"
declare function g(f: () => any): any;
declare function g(a: () => any, b: () => any): any;
function h(arr: (() => any)[]): any { return g(...arr); }
export {};
"#;
    assert!(
        codes(src).contains(&TS2556),
        "with only fixed-arity overloads a non-tuple spread must still emit TS2556"
    );
}

/// A single (non-overloaded) fixed-arity signature still reports TS2556 — the
/// single-signature path keeps emitting immediately.
#[test]
fn nontuple_spread_single_fixed_signature_still_ts2556() {
    let src = r#"
declare function s(f: () => any): any;
function f(arr: (() => any)[]): any { return s(...arr); }
export {};
"#;
    assert!(
        codes(src).contains(&TS2556),
        "a single fixed-arity signature must still emit TS2556 for a non-tuple spread"
    );
}

/// A tuple spread into a fixed-arity overload is always allowed (its length is
/// known) — guards against the fix over-suppressing.
#[test]
fn tuple_spread_into_fixed_overload_is_allowed() {
    let src = r#"
declare function t(a: () => any, b: () => any): any;
function f(tup: [() => any, () => any]): any { return t(...tup); }
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a fixed-length tuple spread into a fixed-arity overload must not error"
    );
}
