//! Tests for contextual typing of callback parameters against tuple rest signatures.
//!
//! Covers two bug fixes in `contextual_rest_tuple_parameter_type` and
//! `contextual_parameter_type_with_env_from_expected`:
//!
//! 1. False-positive TS2345 when a callback has more regular params than the
//!    fixed-length tuple rest, so the callback's `...rest` maps to an empty slice `[]`.
//! 2. Regular params before a rest param in the contextual signature must not be
//!    overshadowed by the rest constraint (e.g. `(x: number, ...args: T)` must give
//!    `a: number` for the first callback param, not `any`).

use tsz_checker::test_utils::check_source_codes;

// ---------------------------------------------------------------------------
// Bug 1: no false-positive when callback rest maps to empty tuple slice
// ---------------------------------------------------------------------------

/// `(a, b, c, ...x)` against `(...args: [A, B, C])`: the rest `...x` maps to
/// the slice `[A,B,C][3..] = []`.  The callback is valid; tsc emits no TS2345.
#[test]
fn rest_callback_exhausts_fixed_tuple_no_error() {
    let codes = check_source_codes(
        r#"
declare const t1: [number, boolean, string];
declare function f1(cb: (...args: typeof t1) => void): void;
f1((a, b, c, ...x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

/// The same pattern with an explicit literal tuple type.
#[test]
fn rest_callback_exhausts_explicit_tuple_no_error() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (...args: [number, string, boolean]) => void): void;
f((a, b, c, ...x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

/// Fewer regular params than the tuple — the rest absorbs the remaining elements;
/// still no error.
#[test]
fn rest_callback_partial_tuple_no_error() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (...args: [number, string, boolean]) => void): void;
f((a, ...x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

/// Callback with only a rest param against a fixed tuple — no error.
#[test]
fn rest_only_callback_against_fixed_tuple_no_error() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (...args: [number, string]) => void): void;
f((...x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Bug 2: regular params before rest must use the non-rest contextual type
// ---------------------------------------------------------------------------

/// `(a, b, ...x)` against `(x: number, ...args: T)`: `a` must be contextually
/// typed as `number` (not `any`).  The callback has more params than the single
/// fixed param, so it is not assignable — tsc DOES emit TS2345 here — but the
/// *source* of the error must reference the correct `a: number` type, not `a: any`.
///
/// We verify the error is emitted (regression guard) rather than a false negative.
#[test]
fn first_regular_param_typed_from_non_rest_contextual_param() {
    let codes = check_source_codes(
        r#"
function f4<T extends any[]>(t: T) {
    function f(cb: (x: number, ...args: T) => void) {}
    f((a, b, ...x) => {});
}
"#,
    );
    // tsc emits TS2345 here; we must emit it too (not a false negative).
    assert!(codes.contains(&2345), "expected TS2345, got: {codes:?}");
}

/// Sanity: callbacks with fewer params than fixed contextual params are fine.
#[test]
fn callback_fewer_params_than_contextual_no_error() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (a: number, b: string) => void): void;
f((x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

/// `f1((a, b, ...x) => {})` with a three-element tuple — the rest maps to
/// `[string]` (the tail), not the full tuple.  Compatible; no error.
#[test]
fn rest_callback_partial_exhaustion_no_error() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (...args: [number, string, boolean]) => void): void;
f((a, b, ...x) => {});
"#,
    );
    assert!(!codes.contains(&2345), "expected no TS2345, got: {codes:?}");
}

/// Regression: a normal assignability error on incompatible types must still fire.
#[test]
fn incompatible_callback_still_emits_ts2345() {
    let codes = check_source_codes(
        r#"
declare function f(cb: (a: number, b: string) => void): void;
const bad: (a: string, b: number) => void = (a, b) => {};
f(bad);
"#,
    );
    assert!(codes.contains(&2345), "expected TS2345, got: {codes:?}");
}
