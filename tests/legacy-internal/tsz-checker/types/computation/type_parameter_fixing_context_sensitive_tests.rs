//! Type-parameter *fixing* with context-sensitive callback arguments
//! (issue #17282).
//!
//! When a generic signature has two function-typed parameters that reference
//! the same two type parameters in *swapped* covariant/contravariant roles
//! (`(z: U) => T` alongside `(x: T) => U`), the parameters are fixed from the
//! non-context-sensitive arguments first (`T = A`, `U = B`). `tsc` treats those
//! fixes as immutable (`InferenceInfo.isFixed`): a callback body may not *widen*
//! a fix to a supertype. tsz previously merged the callback-return candidate
//! into each fixed variable, widening both to the same union (`T = U = A | B`)
//! and vacuously accepting a call `tsc` rejects with `TS2741`.
//!
//! The fix restores the Round-1 fix during finalization, but only when the
//! widening is a body-only covariant one — it stands down for an `any`-tainted
//! call (`tsc` lets an `any` callback body collapse the parameters) and when an
//! explicitly annotated callback parameter supplies a divergent type. Every
//! matrix row below is oracle-verified against `typescript@7.0.2` (the pinned
//! conformance oracle) under `--strict false`, so the tests run non-strict.
//! Binder names vary across rows so the behaviour cannot ride on a spelling.

use crate::test_utils::check_source_non_strict_codes;

const TS2741: u32 = 2741;
const TS2322: u32 = 2322;

/// The core repro (`typeParameterFixingWithContextSensitiveArguments2`): two
/// swapped context-sensitive arrows. `p1`'s arrow returns a value of type `A`
/// where `B` is expected, so `TS2741` fires.
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
        check_source_non_strict_codes(source).contains(&TS2741),
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
        check_source_non_strict_codes(source).contains(&TS2741),
        "renamed swapped-role callbacks must still report TS2741"
    );
}

/// Positive guard: when the callbacks return the *correct* type parameters, the
/// call is accepted — the restore must not over-fire.
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
        !check_source_non_strict_codes(source).contains(&TS2741),
        "callbacks returning the correct type parameters must be accepted"
    );
}

/// `any`-taint exception (`typeParameterFixingWithContextSensitiveArguments5`):
/// a callback body of type `any` (`u2.b` where `b: any`) collapses the type
/// parameters to `any` in `tsc`, suppressing the mismatch. The restore must
/// stand down so tsz does not resurrect a `TS2741` that `tsc` does not report.
#[test]
fn any_typed_callback_body_suppresses_the_error() {
    let source = r#"
function f<T, U>(t1: T, u1: U, pf1: (u2: U) => T, pf2: (t2: T) => U): [T, U] {
  return [t1, pf2(t1)];
}
interface A { a: A; }
interface B extends A { b: any; }
declare var a: A, b: B;
var d = f(a, b, u2 => u2.b, t2 => t2);
"#;
    assert!(
        !check_source_non_strict_codes(source).contains(&TS2741),
        "an any-typed callback body must keep suppressing the mismatch"
    );
}

/// Annotated-parameter exception (the shape of `destructuringTuple`'s second
/// `reduce`): an explicitly annotated callback parameter (`acc: string[]`)
/// supplies a type that diverges from the Round-1 fix seeded by `init` (`[]`).
/// That annotation is real inference `tsc` keeps, so the restore must not force
/// the variable back to the empty-array fix.
#[test]
fn annotated_callback_parameter_is_not_frozen() {
    let source = r#"
declare function fold<U>(cb: (acc: U, e: number) => U, init: U): U;
const r = fold((acc: string[], e) => acc, []);
"#;
    assert!(
        check_source_non_strict_codes(source).is_empty(),
        "an annotated callback parameter must determine the type parameter, not \
         the empty-array Round-1 fix"
    );
}

/// A type parameter determined *only* by a context-sensitive callback (never
/// fixed in Round 1) must still be inferred from that callback in Round 2 —
/// freezing applies only to Round-1 fixes.
#[test]
fn callback_only_type_parameter_is_still_inferred() {
    let source = r#"
declare function mapper<T, U>(items: T[], fn: (x: T) => U): U[];
const r = mapper([1, 2, 3], x => x + 1);
const bad: string[] = r;
"#;
    // `U` is inferred as `number` from the callback body, so assigning the
    // result to `string[]` fails with `TS2322`.
    assert!(
        check_source_non_strict_codes(source).contains(&TS2322),
        "a callback-only type parameter must still be inferred (number here)"
    );
}
