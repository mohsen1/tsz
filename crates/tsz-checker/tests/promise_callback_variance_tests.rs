//! Variance regression tests for the Promise<T>-style callback pattern.
//!
//! These tests pin tsc-equivalent assignability behaviour for generic
//! interfaces whose only mention of `T` is inside a non-method callback that
//! is itself a parameter of a method. They specifically guard the variance
//! fix that distinguishes:
//!
//!   * `interface C<T> { m(x: T): void; }` — bivariant (method bivariance)
//!   * `interface C<T> { m(cb: (x: T) => void): void; }` — covariant
//!   * `interface C<T> { m(x: T, cb: (x: T) => void): void; }` — covariant
//!     (the strict callback occurrence pins the variance)
//!   * `interface Wrap<T> { container: C<T>; }` — inherits bivariance from
//!     the wrapped C<T>, so both directions remain assignable
//!
//! The first three cases are validated by the
//! `promisesWithConstraints` conformance test; the fourth is a regression
//! guard for the `inside_unreliable_application` shield introduced in the
//! variance computation.

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn collect_codes(source: &str) -> Vec<u32> {
    diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|code| *code != 2318) // ignore "Cannot find global type" noise
        .collect()
}

#[test]
fn promise_like_callback_pattern_rejects_mismatched_args() {
    // T appears only inside a non-method callback nested inside a method.
    // Variance must be COVARIANT, not bivariant.
    let codes = collect_codes(
        r#"
interface MyPromise<T> {
    then<U>(cb: (x: T) => MyPromise<U>): MyPromise<U>;
}
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: MyPromise<Foo>;
declare var b: MyPromise<Bar>;
a = b; // ok: Bar <: Foo
b = a; // ERROR: Foo not <: Bar (missing y)
"#,
    );
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for `b = a` (MyPromise<Foo> -> MyPromise<Bar>), got {codes:?}"
    );
}

#[test]
fn mixed_method_and_callback_variance_pins_covariant() {
    // The direct-method-param T occurrence is bivariant; the nested
    // callback occurrence is strictly covariant. The strict signal must win.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T, cb: (x: T) => void): void; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: C<Foo>;
declare var b: C<Bar>;
a = b; // ok
b = a; // ERROR: callback occurrence pins variance to covariant
"#,
    );
    assert!(
        codes.contains(&2322),
        "Expected TS2322 — callback occurrence should pin variance to covariant, got {codes:?}"
    );
}

#[test]
fn pure_method_param_variance_stays_bivariant() {
    // Regression guard: T solely as a direct method parameter must remain
    // bivariant (REJECTION_UNRELIABLE) so structural method bivariance can
    // still allow both assignment directions.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T): void; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: C<Foo>;
declare var b: C<Bar>;
a = b;
b = a;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Pure method-param C<T> should be bivariant — both assignments must succeed, got {codes:?}"
    );
}

#[test]
fn wrapper_of_bivariant_generic_stays_bivariant() {
    // `Wrap<T>` inherits unreliability from the bivariant `C<T>` it wraps.
    // The `inside_unreliable_application` shield must keep
    // `REJECTION_UNRELIABLE` set for `Wrap<T>` so structural bivariance can
    // accept both directions.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T): void; }
interface Wrap<T> { container: C<T>; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: Wrap<Foo>;
declare var b: Wrap<Bar>;
a = b;
b = a;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Wrap<T> wrapping a bivariant C<T> must stay bivariant, got {codes:?}"
    );
}
