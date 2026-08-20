use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn diagnostic_messages<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

#[test]
fn constructor_parameters_rest_spread_is_iterable() {
    let diags = check_source_diagnostics(
        r#"
function create<T extends new (...args: any[]) => any>(
  ctor: T,
  ...args: ConstructorParameters<T>
): InstanceType<T> {
  return new ctor(...args);
}

class MyClass2 {
  constructor(public x: number) {}
}

const inst = create(MyClass2, 42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2488 || d.code == 2345 || d.code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected ConstructorParameters<T> rest spread to be accepted, got: {:?}",
        diagnostic_messages(&errors)
    );
}

fn spread_relation_codes(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code == 2488 || d.code == 2345 || d.code == 2322 || d.code == 2556)
        .collect()
}

/// `f(...params)` where `params: P extends Parameters<F>` and `F` is itself a
/// generic type parameter is a variadic spread, not a representative-element
/// materialization. `Parameters<F>` is a deferred conditional whose array base
/// (`never[]` here) only surfaces after tsc's `getConstraintFromConditionalType`
/// (substitute the check-type `F` with its constraint and re-evaluate). tsc
/// accepts this; tsz used to materialize a representative `any` and report a
/// false TS2345 (`any` not assignable to `never`). Issue #14217.
#[test]
fn spread_of_type_param_constrained_by_parameters_is_variadic() {
    let diags = check_source_diagnostics(
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
    let errors = spread_relation_codes(&diags);
    assert_eq!(
        errors.len(),
        0,
        "Expected spread of `P extends Parameters<F>` to be accepted, got: {:?}",
        diagnostic_messages(&errors)
    );
}

/// The same rule is structural, not name-based: renamed binders (`G`/`Q`) and
/// a `[...Parameters<G>]` tuple constraint behave identically.
#[test]
fn spread_of_type_param_constrained_by_spread_parameters_renamed_binders() {
    let diags = check_source_diagnostics(
        r#"
type Fn = (...rest: never[]) => unknown;
export function applyIt<G extends Fn, Q extends [...Parameters<G>]>(
  fn: G,
  rest: Q,
): unknown {
  return fn(...rest);
}
"#,
    );
    let errors = spread_relation_codes(&diags);
    assert_eq!(
        errors.len(),
        0,
        "Expected spread of `Q extends [...Parameters<G>]` to be accepted, got: {:?}",
        diagnostic_messages(&errors)
    );
}

/// A user infer-free conditional whose deferred constraint resolves to an
/// array (`T extends number ? unknown[] : never`) is also a valid variadic
/// spread source — its values are array-like, so neither TS2488 nor TS2345
/// should fire. tsc accepts this.
#[test]
fn spread_of_type_param_constrained_by_user_conditional_array_is_variadic() {
    let diags = check_source_diagnostics(
        r#"
export function callIt3<T, P extends (T extends number ? unknown[] : never)>(
  fn: (...a: unknown[]) => void,
  params: P,
): unknown {
  return fn(...params);
}
"#,
    );
    let errors = spread_relation_codes(&diags);
    assert_eq!(
        errors.len(),
        0,
        "Expected spread of a type param constrained by an array-resolving conditional to be accepted, got: {:?}",
        diagnostic_messages(&errors)
    );
}

/// Negative control: a type parameter constrained to a non-array type
/// (`P extends string`) is NOT a valid spread source. The constraint-resolution
/// path must only ever *recognize more* array/tuple constraints; a genuinely
/// invalid spread keeps its diagnostic (tsc reports the element relation
/// failure here).
#[test]
fn spread_of_type_param_constrained_by_string_still_errors() {
    let diags = check_source_diagnostics(
        r#"
export function bad<P extends string>(
  fn: (...a: never[]) => void,
  params: P,
): unknown {
  return fn(...params);
}
"#,
    );
    let errors = spread_relation_codes(&diags);
    assert!(
        !errors.is_empty(),
        "Expected spread of `P extends string` to be rejected, but no diagnostic was emitted",
    );
}
