//! A conditional type whose CHECK type is concrete (`[]`) but whose EXTENDS type
//! carries a type parameter (`[T, ...T[]]`) must not be left deferred when the
//! false branch is definitive under permissive instantiation (T -> any). tsc
//! resolves `[] extends [T, ...T[]] ? "yes" : "no"` to its false branch (`"no"`);
//! tsz previously deferred (because the extends type carried `T`), leaving the
//! alias opaque so a later `"no"` assignment produced a false TS2322. (#14232)

use super::super::core::*;

/// The witness: `A = [] extends [T, ...T[]] ? "yes" : "no"` resolves to `"no"`
/// (an empty tuple is never a non-empty tuple, even under `T -> any`), so
/// `const a: A = "no"` type-checks with no TS2322.
#[test]
fn concrete_check_generic_extends_resolves_false_branch_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "no";
  return a;
}
export { f };
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `[] extends [T, ...T[]]` resolves to its false branch \
         (`\"no\"`) under permissive instantiation. Actual: {diagnostics:#?}"
    );
}

/// Negative control: the conditional genuinely resolves to `"no"`, so assigning
/// the *true*-branch literal `"yes"` must still error — the fix resolves the
/// branch, it does not blanket-suppress assignability.
#[test]
fn concrete_check_generic_extends_true_branch_value_still_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "yes";
  return a;
}
export { f };
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "TS2322 expected — `A` resolves to `\"no\"`, so `\"yes\"` is not assignable. \
         Actual: {diagnostics:#?}"
    );
}
