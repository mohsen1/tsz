//! Regression tests for comparability of instantiable indexed-access operands.
//!
//! A switch discriminant or `===`/`!==` operand whose type is an instantiable
//! indexed access such as `Parameters<F>["length"]` must be reduced through the
//! object's base constraint before the comparability relation runs. For
//! `F extends (...args: any[]) => any`, `Parameters<F>` reduces to `any[]`, so
//! `Parameters<F>["length"]` reduces to `number` and overlaps numeric literals
//! — no false TS2678 (switch/case) or TS2367 (`===`/`!==`).
//!
//! The reduction must NOT over-accept: a `string`/`boolean` case over a reduced
//! `number` is still incomparable (true-positive TS2678/TS2367 preserved).
//!
//! These fixtures reference `Parameters<T>` (from `lib.es5.d.ts`), so they run
//! through the lib-loading harness ([`check_source_with_libs_code_messages`]
//! with [`load_default_lib_files`]); the plain `check_source_strict` harness
//! does not install the lib that defines `Parameters`.
use crate::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

fn check_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// `switch (arity)` where `arity: Parameters<F>["length"]` with numeric `case`s
/// is clean in tsc; the indexed access reduces to `number`.
#[test]
fn switch_on_parameters_length_numeric_cases_no_ts2678() {
    let source = r#"
function f<DataFirst extends (...args: any[]) => any>(
  arity: Parameters<DataFirst>["length"],
): void {
  switch (arity) {
    case 0:
    case 1:
      break;
    case 2:
      return;
  }
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2678),
        "numeric cases overlap reduced `number`; expected no TS2678, got: {codes:?}"
    );
}

/// `n === 2` / `n !== 5` where `n: Parameters<F>["length"]` is clean in tsc
/// (reduces to `number`, which overlaps numeric literals).
#[test]
fn equality_on_parameters_length_numeric_no_ts2367() {
    let source = r#"
function g<F extends (...a: any[]) => any>(n: Parameters<F>["length"]) {
  if (n === 2) {}
  if (n !== 5) {}
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2367),
        "`number` overlaps `2`/`5`; expected no TS2367, got: {codes:?}"
    );
}

/// A `string`-literal `case` over a reduced `number` is a true-positive
/// TS2678 — the reduction must not suppress it.
#[test]
fn switch_on_parameters_length_string_case_still_ts2678() {
    let source = r#"
function f<DataFirst extends (...args: any[]) => any>(
  arity: Parameters<DataFirst>["length"],
): void {
  switch (arity) {
    case "hello":
      break;
  }
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2678),
        "`string` does not overlap reduced `number`; expected TS2678, got: {codes:?}"
    );
}

/// A `boolean`-literal `case` over a reduced `number` is still incomparable.
#[test]
fn switch_on_parameters_length_boolean_case_still_ts2678() {
    let source = r#"
function f<DataFirst extends (...args: any[]) => any>(
  arity: Parameters<DataFirst>["length"],
): void {
  switch (arity) {
    case true:
      break;
  }
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2678),
        "`boolean` does not overlap reduced `number`; expected TS2678, got: {codes:?}"
    );
}

/// An `n === "x"` over a reduced `number` is a true-positive TS2367.
#[test]
fn equality_on_parameters_length_string_still_ts2367() {
    let source = r#"
function g<F extends (...a: any[]) => any>(n: Parameters<F>["length"]) {
  if (n === "x") {}
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2367),
        "`string` does not overlap reduced `number`; expected TS2367, got: {codes:?}"
    );
}

/// A `string` discriminant with a numeric `case` is unaffected by the
/// indexed-access reduction — still TS2678 (no spurious acceptance).
#[test]
fn switch_on_string_numeric_case_still_ts2678() {
    let source = r#"
function h(s: string): void {
  switch (s) {
    case 1:
      break;
  }
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2678),
        "`number` does not overlap `string`; expected TS2678, got: {codes:?}"
    );
}

/// A concrete (non-generic) `Parameters<(a, b) => void>["length"]` reduces to
/// the tuple length literal `2`; `case 2` is comparable but `case 3` is not.
#[test]
fn switch_on_concrete_parameters_length_rejects_wrong_literal() {
    let source = r#"
function k(n: Parameters<(a: number, b: string) => void>["length"]) {
  switch (n) {
    case 2:
      return;
    case 3:
      return;
  }
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2678),
        "`3` does not overlap reduced `2`; expected TS2678, got: {codes:?}"
    );
}
