//! Spreading a type parameter whose constraint is a *deferred* array/tuple-like
//! type (e.g. `P extends Parameters<F>`) must be treated as a variadic spread,
//! not destructured against the target's parameters. `tsz` previously only
//! recognized a *directly* array/tuple-like constraint, so `...params` where
//! `params: P` and `P extends Parameters<F>` fell through to a per-element check
//! that compared `any` against the rest parameter's `never` element, emitting a
//! false TS2345 ("'any' is not assignable to 'never'"). The gate now resolves a
//! deferred constraint (`Parameters<F>` -> its apparent base constraint) before
//! probing for array/tuple shape. (#14217)

use super::super::core::*;

/// The witness: `P extends Parameters<F>` is a deferred conditional constraint
/// (an unevaluated `Parameters` alias application). The spread `fn(...params)`
/// must be recognized as a variadic spread, so no TS2345 fires.
#[test]
fn spread_param_extends_parameters_of_fn_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type AnyFunction = (...params: never[]) => unknown;
export function callIt<F extends AnyFunction, P extends Parameters<F>>(
  fn: F,
  params: P,
): unknown {
  return fn(...params);
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "no TS2345 expected — `P extends Parameters<F>` is a deferred array/tuple-like \
         constraint, so `fn(...params)` is a variadic spread, not a destructured arg \
         list. Actual: {diagnostics:#?}"
    );
}

/// Adjacent case: the constraint is a *directly* tuple-like deferred reference
/// reached through a second type parameter (`P extends Q`, `Q extends [number]`).
/// The fallback resolution must not regress the already-working direct-tuple
/// path, so this stays clean too.
#[test]
fn spread_param_extends_indirect_tuple_constraint_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export function callIt<Q extends [number, number], P extends Q>(
  fn: (a: number, b: number) => unknown,
  params: P,
): unknown {
  return fn(...params);
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "no TS2345 expected — `P extends Q`, `Q extends [number, number]` is a \
         tuple-like spread. Actual: {diagnostics:#?}"
    );
}

/// Negative control: a real argument-type mismatch on a tuple-constrained spread
/// must still surface as TS2345. The spread is recognized as variadic, but the
/// tuple element type (`string`) is incompatible with the target parameter
/// (`number`), so the assignability error must fire — the fix only stops the
/// false `any`-vs-`never` element check, never a real one.
#[test]
fn spread_tuple_param_with_wrong_element_type_still_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export function callIt<P extends [string, string]>(
  fn: (a: number, b: number) => unknown,
  params: P,
): unknown {
  return fn(...params);
}
"#,
    );
    assert!(
        has_error(&diagnostics, 2345),
        "TS2345 expected — the variadic spread of a `[string, string]` tuple into a \
         `(number, number)` target is a real element-type mismatch. Actual: {diagnostics:#?}"
    );
}
