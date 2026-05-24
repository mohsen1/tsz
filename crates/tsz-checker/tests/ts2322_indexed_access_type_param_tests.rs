//! TS2322 coverage for indexed access on constrained type-parameter receivers.
//!
//! Regression for #9716: element access on a generic type parameter constrained
//! to an array, tuple, or chain thereof must resolve to the constraint's
//! apparent element type. Previously the solver rejected `T` as
//! `NotIndexable`, collapsed the access type to `ERROR`, and silently swallowed
//! TS2322.

use tsz_checker::context::CheckerOptions;

fn count_errors_with_code(source: &str, code: u32) -> usize {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

#[test]
fn array_constrained_type_param_emits_mismatch() {
    let source = r#"
function g<T extends number[]>(t: T) {
  const e = t[0];
  const s: string = e;
}
"#;
    assert_eq!(
        count_errors_with_code(source, 2322),
        1,
        "T[0] for T extends number[] must produce a TS2322 number-to-string mismatch"
    );
}

#[test]
fn unknown_array_constrained_type_param_emits_mismatch() {
    let source = r#"
function f<T extends unknown[]>(t: T) {
  const e = t[0];
  const n: number = e;
}
"#;
    assert_eq!(
        count_errors_with_code(source, 2322),
        1,
        "T[0] for T extends unknown[] must produce a TS2322 unknown-to-number mismatch"
    );
}

#[test]
fn renamed_array_constrained_type_param_emits_mismatch() {
    let source = r#"
function h<P extends string[]>(p: P) {
  const e = p[0];
  const n: number = e;
}
"#;
    assert_eq!(
        count_errors_with_code(source, 2322),
        1,
        "P[0] for P extends string[] must produce a TS2322 string-to-number mismatch"
    );
}

#[test]
fn tuple_constrained_type_param_emits_mismatch() {
    let source = r#"
function k<T extends [string, number]>(t: T) {
  const a = t[0];
  const b = t[1];
  const n: number = a;
  const s: string = b;
}
"#;
    assert_eq!(
        count_errors_with_code(source, 2322),
        2,
        "tuple positions must surface independently as TS2322 mismatches"
    );
}

#[test]
fn chained_array_constrained_type_param_emits_mismatch() {
    let source = r#"
function chain<T extends number[], U extends T>(u: U) {
  const e = u[0];
  const s: string = e;
}
"#;
    assert_eq!(
        count_errors_with_code(source, 2322),
        1,
        "U[0] for U extends T extends number[] must produce a TS2322 number-to-string mismatch"
    );
}
