//! Regression for issue #14417: a self-recursive conditional whose check type
//! is a **generic/concrete type alias that resolves to `Promise<…>`** must
//! reduce through the alias indirection and converge to the same type `tsc`
//! computes, instead of failing to canonicalize and leaving an opaque result
//! (or, schedule-dependently, non-terminating).
//!
//! Root cause: the reduced result of the recursive conditional records a
//! `display_alias` back-reference from its concrete value (`{ id: 0 }`) to the
//! recursive alias application (`Deep<AB<{ id: 0 }>>`).
//! `reduce_alias_body_to_application_form` treated that diagnostic-only
//! back-reference as a structural reduction handle and followed it back into
//! `Deep`, re-entering the recursion that produced the value — an unbounded
//! re-evaluation (the non-crash residual left after the #14123 crash fix). The
//! reducer now refuses to follow a `display_alias` whose application base is a
//! *recursive* alias (its body re-references its own `DefId`), mirroring the
//! `result_has_residual_recursive_alias` cache-poison guard; recoveries to a
//! non-recursive alias (e.g. the structural `Promise` body back to `Promise<…>`)
//! are still followed so the conditional `infer` slot binds.
//!
//! Each test asserts convergence (no TS2589) AND the exact `tsc` result via a
//! positive assignment plus a mismatched-target TS2322. Binder names are varied
//! per case so the guard is structural, not identifier-driven.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

fn codes(source: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// The minimal witness from the issue: recursion in the true branch + a
/// `Promise` check + the check type reaching the conditional through a *generic*
/// alias (`AB<S> = Promise<S>`). The conditional must bind `infer U = { id: 0 }`
/// and `Deep<{ id: 0 }>` must converge to `{ id: 0 }`.
#[test]
fn issue_14417_recursive_generic_alias_promise_converges() {
    let c = codes(
        r#"
type Deep<K> = K extends Promise<infer U> ? Deep<U> : K;
type AB<S> = Promise<S>;
type Out = Deep<AB<{ id: 0 }>>;
declare const o: Out;
const ok: { id: 0 } = o;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert!(
        !c.contains(&2322),
        "Out must equal {{ id: 0 }} (positive assignment holds). Got: {c:?}"
    );
}

/// Same shape, renamed binders, mismatched target: the reduced `Out` equals
/// `{ id: 0 }`, so assigning it to `{ id: 1 }` reports exactly one TS2322 —
/// proving the recursion converged to a concrete value, not an opaque/`any`
/// placeholder (which would suppress the error).
#[test]
fn issue_14417_recursive_generic_alias_promise_reports_mismatch() {
    let c = codes(
        r#"
type Unwrap<Q> = Q extends Promise<infer A> ? Unwrap<A> : Q;
type Wrapper<S> = Promise<S>;
type Result = Unwrap<Wrapper<{ id: 0 }>>;
declare const value: Result;
const bad: { id: 1 } = value;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "mismatched target must report exactly one TS2322. Got: {c:?}"
    );
}

/// Recursive + *concrete* (non-generic) alias: `Box = Promise<{ tag: 7 }>`.
/// The check type still reaches the conditional through an alias indirection.
#[test]
fn issue_14417_recursive_concrete_alias_promise_converges() {
    let c = codes(
        r#"
type Peel<T> = T extends Promise<infer R> ? Peel<R> : T;
type Box = Promise<{ tag: 7 }>;
type Out = Peel<Box>;
declare const o: Out;
const ok: { tag: 7 } = o;
const bad: { tag: 8 } = o;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "concrete-alias result must equal {{ tag: 7 }}; one mismatch TS2322. Got: {c:?}"
    );
}

/// Recursive + generic alias wrapping a structural payload:
/// `Carrier<S> = Promise<{ payload: S }>`. The conditional binds
/// `infer U = { payload: { id: 0 } }`, recurses once on a non-`Promise`, and
/// settles on `{ payload: { id: 0 } }`.
#[test]
fn issue_14417_recursive_generic_alias_payload_wrapper_converges() {
    let c = codes(
        r#"
type Dig<X> = X extends Promise<infer Y> ? Dig<Y> : X;
type Carrier<S> = Promise<{ payload: S }>;
type Out = Dig<Carrier<{ id: 0 }>>;
declare const o: Out;
const ok: { payload: { id: 0 } } = o;
const bad: { payload: { id: 1 } } = o;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "payload-wrapper result must equal {{ payload: {{ id: 0 }} }}; one mismatch TS2322. \
         Got: {c:?}"
    );
}

/// Non-regression for the passing rows of the issue's matrix: the fix must not
/// disturb the already-correct cases.
/// * base: `Deep<{ id: 0 }>`            → `{ id: 0 }`
/// * inline Promise: `Deep<Promise<…>>` → `{ id: 0 }`
/// * non-recursive + alias              → `{ id: 0 }`
#[test]
fn issue_14417_baseline_cases_unaffected() {
    let base = codes(
        r#"
type Sink<K> = K extends Promise<infer U> ? Sink<U> : K;
type Out = Sink<{ id: 0 }>;
declare const o: Out;
const ok: { id: 0 } = o;
const bad: { id: 1 } = o;
"#,
    );
    assert!(!base.contains(&2589), "base must converge. Got: {base:?}");
    assert_eq!(
        base.iter().filter(|&&x| x == 2322).count(),
        1,
        "base: one mismatch TS2322. Got: {base:?}"
    );

    let inline = codes(
        r#"
type Drain<K> = K extends Promise<infer U> ? Drain<U> : K;
type Out = Drain<Promise<{ id: 0 }>>;
declare const o: Out;
const ok: { id: 0 } = o;
const bad: { id: 1 } = o;
"#,
    );
    assert!(
        !inline.contains(&2589),
        "inline must converge. Got: {inline:?}"
    );
    assert_eq!(
        inline.iter().filter(|&&x| x == 2322).count(),
        1,
        "inline Promise: one mismatch TS2322. Got: {inline:?}"
    );

    let non_recursive = codes(
        r#"
type Once<K> = K extends Promise<infer U> ? U : K;
type AB<S> = Promise<S>;
type Out = Once<AB<{ id: 0 }>>;
declare const o: Out;
const ok: { id: 0 } = o;
const bad: { id: 1 } = o;
"#,
    );
    assert!(
        !non_recursive.contains(&2589),
        "non-recursive must converge. Got: {non_recursive:?}"
    );
    assert_eq!(
        non_recursive.iter().filter(|&&x| x == 2322).count(),
        1,
        "non-recursive + alias: one mismatch TS2322. Got: {non_recursive:?}"
    );
}
