//! Regression tests: a terminating recursive conditional alias driven by a
//! *named* tuple-type-alias argument must not trip a spurious TS2589.
//!
//! Root cause (use-site TS2589 convergence check): the residual-growth metric
//! compared the input application's argument weight against the residual
//! self-applications left in the evaluated body. A named tuple alias argument
//! (`type TN = [0, 0]`, a `Lazy(DefId)` reference) was scored as a single opaque
//! unit on the *input* side, while the evaluator left the *resolved* inline
//! tuple in the residual — so a recursion that is actually shrinking
//! (`Nest<T, TN>` -> `Nest<T, [...shorter]>`) was misread as growing and flagged
//! infinite. The fix resolves `Lazy(DefId)` alias arguments before weighing, so
//! a named alias and its inline expansion weigh identically.
//!
//! `tsc` accepts every "must be clean" case below and still reports TS2589 for
//! the genuinely diverging one.

use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn shrinking_recursion_with_named_tuple_alias_arg_is_not_ts2589() {
    let codes = diagnostic_codes(
        r#"
type Nest<T, N extends readonly any[]> =
  N extends readonly [any, ...infer R] ? { child: Nest<T, R> } : T;
type TN = [0, 0];
declare const d: Nest<{ leaf: string }, TN>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "terminating recursion through a named tuple alias arg must not be TS2589: {codes:?}"
    );
}

#[test]
fn shrinking_recursion_with_named_tuple_alias_arg_renamed_binders_is_not_ts2589() {
    // Same structure as above with every user-chosen identifier renamed, so the
    // fix is proven structural rather than keyed on any particular binder name.
    let codes = diagnostic_codes(
        r#"
type Wrap<Payload, Steps extends readonly any[]> =
  Steps extends readonly [any, ...infer Rest] ? { next: Wrap<Payload, Rest> } : Payload;
type Counter = [unknown, unknown, unknown];
declare const value: Wrap<{ tag: number }, Counter>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "renamed-binder variant must also avoid spurious TS2589: {codes:?}"
    );
}

#[test]
fn shrinking_recursion_named_alias_first_arg_position_is_not_ts2589() {
    // The named alias sits in the carried (non-driving) argument position; the
    // driving tuple is inline. The fix must not depend on argument order.
    let codes = diagnostic_codes(
        r#"
type Nest<T, N extends readonly any[]> =
  N extends readonly [any, ...infer R] ? { child: Nest<T, R> } : T;
type Carried = [0, 0];
declare const d: Nest<Carried, [0, 0]>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "named alias in the carried argument must not be TS2589: {codes:?}"
    );
}

#[test]
fn inline_tuple_arg_control_is_not_ts2589() {
    // Control: the inline-tuple form always worked; it must keep working.
    let codes = diagnostic_codes(
        r#"
type Nest<T, N extends readonly any[]> =
  N extends readonly [any, ...infer R] ? { child: Nest<T, R> } : T;
declare const d: Nest<{ leaf: string }, [0, 0]>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "inline-tuple control must not be TS2589: {codes:?}"
    );
}

#[test]
fn deep_terminating_recursion_with_named_tuple_alias_arg_is_not_ts2589() {
    // A longer (but still finite) seed exercises more recursion steps; tsc is
    // clean here too.
    let codes = diagnostic_codes(
        r#"
type Nest<T, N extends readonly any[]> =
  N extends readonly [any, ...infer R] ? { child: Nest<T, R> } : T;
type TN = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
declare const d: Nest<{ leaf: string }, TN>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "deep terminating recursion through a named tuple alias arg must not be TS2589: {codes:?}"
    );
}

#[test]
fn growing_recursion_with_named_tuple_alias_arg_still_emits_ts2589() {
    // True-positive guard: the same named-alias shape but with a *growing*
    // driving tuple (`[any, ...N]`) never reaches a base case. tsc reports
    // TS2589; the fix must keep detecting genuine divergence rather than
    // blanket-suppressing it once a named alias is involved.
    let codes = diagnostic_codes(
        r#"
type Grow<T, N extends readonly any[]> =
  N extends readonly [any, ...infer R] ? Grow<T, [any, ...N]> : T;
type TN = [0, 0];
declare const d: Grow<{ leaf: string }, TN>;
"#,
    );
    assert!(
        codes.contains(&2589),
        "genuinely diverging recursion must still emit TS2589: {codes:?}"
    );
}
