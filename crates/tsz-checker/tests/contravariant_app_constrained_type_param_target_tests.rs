//! `same_base_application_to_constrained_type_param_target` must skip
//! contravariant positions.
//!
//! Structural rule: when two Application types share a base and the target's
//! arg is a type parameter `U` whose constraint equals (or is assignable to)
//! the source's arg `X`, the helper rejects the assignment up-front — sound
//! for COVARIANT or INVARIANT positions because `App<X>` may carry data
//! shapes that narrower instantiations of `U` cannot accept. But the same
//! rejection is unsound for CONTRAVARIANT positions: in that orientation,
//! `App<X> <: App<U extends X>` is exactly what contravariance permits, and
//! the variance-aware fast path immediately downstream is responsible for
//! accepting it.
//!
//! Concrete consequence: `conditionalTypes2.ts` function `f2` —
//! `interface Contravariant<T> { foo: T extends string ? keyof T : number }`
//! — emitted a spurious second TS2322 on `b = a` because this helper
//! short-circuited the variance check before contravariance could fire.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn contravariant_application_to_constrained_param_target_passes() {
    // The conformance source pattern. With B extends A, contravariance lets
    // `Contravariant<A>` be assigned to `Contravariant<B>` (b = a) but not
    // the reverse (a = b — that's the lone TS2322 tsc 6.0.3 reports).
    let source = r#"
interface Contravariant<T> {
    foo: T extends string ? keyof T : number;
}
function f2<A, B extends A>(a: Contravariant<A>, b: Contravariant<B>) {
    a = b;  // Error
    b = a;  // OK
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Exactly one TS2322 expected (the `a = b` line). Codes: {codes:?}"
    );
}

#[test]
fn contravariant_via_explicit_in_annotation_passes() {
    // Explicit `in T` declares contravariance regardless of body shape.
    // `Contra<A> <: Contra<B>` when B extends A — the canonical
    // contravariant function-parameter case.
    let source = r#"
interface Contra<in T> {
    foo: (x: T) => void;
}
function f<A, B extends A>(ca: Contra<A>, cb: Contra<B>) {
    ca = cb;  // Error: Contra<B> not assignable to Contra<A>
    cb = ca;  // OK: contravariance permits Contra<A> -> Contra<B>
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Exactly one TS2322 expected (the wrong direction). Codes: {codes:?}"
    );
}

#[test]
fn covariant_application_to_constrained_param_target_rejects_wider_to_narrower() {
    // Anti-regression: COVARIANT containers still reject the wider-to-narrower
    // direction. `Covariant<A>` -> `Covariant<B>` fails when B extends A,
    // while `Covariant<B>` -> `Covariant<A>` is allowed.
    let source = r#"
interface Covariant<T> {
    foo: T extends string ? T : number;
}
function f<A, B extends A>(a: Covariant<A>, b: Covariant<B>) {
    a = b;  // OK (covariant: B<:A allowed)
    b = a;  // Error
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Exactly one TS2322 expected (the wrong covariant direction). Codes: {codes:?}"
    );
}

#[test]
fn invariant_application_to_constrained_param_target_rejects_both() {
    // Anti-regression: INVARIANT containers reject both directions.
    let source = r#"
interface Invariant<T> {
    foo: T extends string ? keyof T : T;
}
function f<A, B extends A>(a: Invariant<A>, b: Invariant<B>) {
    a = b;  // Error
    b = a;  // Error
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Both TS2322 expected for invariant case. Codes: {codes:?}"
    );
}
