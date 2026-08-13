//! Regression tests for type-parameter *fixing* with context-sensitive
//! arguments (issue #17282).
//!
//! When a generic signature has two function-typed parameters that reference
//! the same two type parameters in *swapped* covariant/contravariant roles
//! (`(z: U) => T` alongside `(x: T) => U`), the type parameters are fixed from
//! the non-context-sensitive arguments first (`T = A`, `U = B`). tsc treats
//! those fixes as immutable: inference from the context-sensitive callback
//! arguments must not *widen* them. Before the fix, tsz merged a
//! callback-derived candidate into each fixed variable and widened both to the
//! same union (`T = U = A | B`), which vacuously accepted a call `tsc` rejects
//! with `TS2741`.
//!
//! The matrix below is oracle-verified against `typescript@7.0.2` in the
//! issue. Binder names are varied across cases so the anchored behaviour cannot
//! ride on a specific identifier spelling.

use crate::test_utils::check_source_codes;

const TS2741: u32 = 2741;

/// The core repro: two swapped context-sensitive arrows. `p1`'s arrow returns
/// a value of type `A` where `B` is expected, so `TS2741` fires.
#[test]
fn swapped_callback_roles_report_missing_property() {
    let source = r#"
function f<T, U>(y: T, y1: U, p: (z: U) => T, p1: (x: T) => U): [T, U] {
  return [y, p1(y)];
}
interface A { a: A; }
interface B extends A { b: number; }
declare var a: A, b: B;
var d = f(a, b, x => x, x => x);
"#;
    assert!(
        check_source_codes(source).contains(&TS2741),
        "swapped-role callbacks must not widen the fixed type parameters"
    );
}

/// Same shape, different binder names — the behaviour is structural, not tied
/// to `T`/`U`/`f`.
#[test]
fn swapped_callback_roles_report_with_renamed_binders() {
    let source = r#"
function combine<First, Second>(
  head: First,
  tail: Second,
  back: (s: Second) => First,
  fwd: (f: First) => Second,
): [First, Second] {
  return [head, fwd(head)];
}
interface Base { self: Base; }
interface Derived extends Base { extra: number; }
declare var base: Base, derived: Derived;
var out = combine(base, derived, v => v, v => v);
"#;
    assert!(
        check_source_codes(source).contains(&TS2741),
        "renamed swapped-role callbacks must still report TS2741"
    );
}

/// A concrete named function in the `back` slot (so that argument is no longer
/// context-sensitive) with the swapped *declared* signature still misses the
/// error before the fix; it must report `TS2741` now.
#[test]
fn swapped_declared_signature_with_concrete_arg_reports() {
    let source = r#"
function f<T, U>(y: T, y1: U, p: (z: U) => T, p1: (x: T) => U): [T, U] {
  return [y, p1(y)];
}
interface A { a: A; }
interface B extends A { b: number; }
declare var a: A, b: B;
function idBA(z: B): A { return z; }
var d = f(a, b, idBA, x => x);
"#;
    assert!(
        check_source_codes(source).contains(&TS2741),
        "a swapped declared signature must report even with a concrete argument"
    );
}

/// Positive guard: when the callbacks return the *correct* type parameters,
/// the call is accepted. The fix must not over-fire.
#[test]
fn correct_callback_returns_are_accepted() {
    let source = r#"
function f<T, U>(y: T, y1: U, p: (z: U) => T, p1: (x: T) => U): [T, U] {
  return [y, p1(y)];
}
interface A { a: A; }
interface B extends A { b: number; }
declare var a: A, b: B;
var d = f(a, b, z => a, x => b);
"#;
    assert!(
        !check_source_codes(source).contains(&TS2741),
        "callbacks returning the correct type parameters must be accepted"
    );
}

/// Positive guard: a type parameter that is *only* determined by a
/// context-sensitive callback (never fixed in Round 1) must still be inferred
/// from that callback in Round 2 — freezing only applies to Round 1 fixes.
#[test]
fn callback_only_type_parameter_is_still_inferred() {
    let source = r#"
function map<T, U>(items: T[], fn: (x: T) => U): U[] {
  return [] as U[];
}
var r = map([1, 2, 3], x => x + 1);
var widened: number[] = r;
var mismatch: string[] = r;
"#;
    // `U` is inferred as `number` from the callback body; assigning to
    // `string[]` must fail (TS2322), assigning to `number[]` must not.
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "callback-only type parameter must be inferred (number here), so the \
         string[] assignment fails"
    );
}
