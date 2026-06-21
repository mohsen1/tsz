//! Interface heritage merge must not eagerly evaluate an alias `Application`
//! whose arguments are still-unsubstituted type parameters of the interface
//! being merged. When `Generator<Y, R> extends Iterator<Y, R>` overrides
//! `next(): IteratorResult<Y, R>`, the alias `IteratorResult` Application keeps
//! its `Y`/`R` arguments deferred so the receiver's type-arg substitution
//! applies at member-access time; otherwise `g.next().value` collapsed to a
//! wrong arm and a false TS2322 fired. (#14235)

use super::super::core::*;

/// The witness: `g.next().value` is `Y | R` (the `value` field of
/// `IteratorResult<Y, R>` distributed over its union), which is exactly the
/// declared return `Y | R`. No TS2322 is expected.
#[test]
fn generator_next_value_assignable_to_yield_or_return_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type IteratorResult<T, TReturn = any> =
  | { done?: false; value: T }
  | { done: true; value: TReturn };
interface Iterator<T, TReturn = any> {
  next(): IteratorResult<T, TReturn>;
}
interface Generator<Y, R> extends Iterator<Y, R> {
  next(): IteratorResult<Y, R>;
}
export function take<Y, R>(g: Generator<Y, R>): Y | R {
  return g.next().value;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `g.next().value` is `Y | R`, assignable to the \
         declared return `Y | R`. Actual: {diagnostics:#?}"
    );
}

/// Renamed-binder variant (anti-hardcoding): the same heritage-merge shape with
/// non-lib names — no global merge with the lib's `Iterator`/`Generator` — must
/// also stay clean, proving the result does not depend on the lib collision or
/// the specific identifiers.
#[test]
fn renamed_heritage_alias_application_value_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type StepResult<A, B = any> =
  | { done?: false; payload: A }
  | { done: true; payload: B };
interface Stepper<A, B = any> {
  step(): StepResult<A, B>;
}
interface Walker<P, Q> extends Stepper<P, Q> {
  step(): StepResult<P, Q>;
}
export function drive<P, Q>(w: Walker<P, Q>): P | Q {
  return w.step().payload;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `w.step().payload` is `P | Q`, assignable to the \
         declared return `P | Q` (renamed, no lib collision). Actual: {diagnostics:#?}"
    );
}

/// Negative control: a genuine return-type mismatch must still emit TS2322.
/// `g.next().value` is `Y | R`, not assignable to `string`.
#[test]
fn generator_next_value_not_assignable_to_string_emits_ts2322() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type IteratorResult<T, TReturn = any> =
  | { done?: false; value: T }
  | { done: true; value: TReturn };
interface Iterator<T, TReturn = any> {
  next(): IteratorResult<T, TReturn>;
}
interface Generator<Y, R> extends Iterator<Y, R> {
  next(): IteratorResult<Y, R>;
}
export function bad<Y, R>(g: Generator<Y, R>): string {
  return g.next().value;
}
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "TS2322 expected — `g.next().value` is `Y | R`, not assignable to \
         `string`. Actual: {diagnostics:#?}"
    );
}
