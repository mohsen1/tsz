//! Regression tests for comparability/overlap through transparent wrappers.
//!
//! `NoInfer<T>` is a transparent wrapper: it preserves the underlying set of
//! values. A comparison or `switch`/`case` whose operand is `NoInfer<T>` for a
//! *constrained* type parameter `T` must consult `T`'s constraint exactly as a
//! bare `T` would.
//! Before the wrappers were peeled, `NoInfer<T>` was not recognized as
//! type-parameter-like, its constraint was never reached, and the comparison
//! was wrongly rejected with a false TS2367 (`===`/`!==`) or TS2678
//! (`switch`/`case`) — while tsc 6.0.3 accepts it.
//!
//! The unwrap must NOT over-accept: a literal *outside* the constraint is still
//! a true-positive TS2367/TS2678 (the wrapped parameter still recurses into the
//! constraint, which reports no overlap). Binder names are varied across the
//! fixtures so the behavior cannot key off a specific type-parameter identifier.
//!
//! These fixtures reference the `NoInfer<T>` global utility (from the default
//! lib), so they run through the lib-loading harness.
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

/// `input === true` where `input: NoInfer<T>` and `T extends string | true` is
/// clean in tsc: the constraint includes `true`. The wrapper must not hide it.
#[test]
fn noinfer_constrained_param_equality_against_constraint_member_no_ts2367() {
    let source = r#"
function search<TInput extends string | true>(input: NoInfer<TInput>) {
  if (input === true) {
    return 1;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2367),
        "`true` is in the constraint `string | true`; expected no TS2367, got: {codes:?}"
    );
}

/// `!==` exercises the same overlap predicate as `===`; also clean.
#[test]
fn noinfer_constrained_param_inequality_against_constraint_member_no_ts2367() {
    let source = r#"
function pick<K extends string | true>(value: NoInfer<K>) {
  if (value !== true) {
    return 1;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2367),
        "`!==` over a constraint member must not emit TS2367, got: {codes:?}"
    );
}

/// A string-literal constraint vs a matching literal overlaps — no TS2367.
#[test]
fn noinfer_string_literal_union_constraint_matching_literal_no_ts2367() {
    let source = r#"
function route<S extends "x" | "y">(seg: NoInfer<S>) {
  if (seg === "x") {
    return 1;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2367),
        "`\"x\"` is in the constraint; expected no TS2367, got: {codes:?}"
    );
}

/// A literal NOT in the constraint is a true-positive TS2367 — the unwrap must
/// not suppress it.
#[test]
fn noinfer_constrained_param_equality_against_non_member_still_ts2367() {
    let source = r#"
function guard<TName extends "a" | "b">(input: NoInfer<TName>) {
  if (input === "c") {
    return 1;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    assert!(
        codes.contains(&2367),
        "`\"c\"` is not in the constraint `\"a\" | \"b\"`; expected TS2367, got: {codes:?}"
    );
}

/// `switch` on a `NoInfer<T>` scrutinee: a `case` inside the constraint is
/// comparable (no TS2678) while a `case` outside it stays a true-positive
/// TS2678 — matching tsc 6.0.3.
#[test]
fn noinfer_switch_case_in_constraint_no_ts2678_out_of_constraint_ts2678() {
    let source = r#"
function dispatch<TKey extends "a" | "b">(input: NoInfer<TKey>) {
  switch (input) {
    case "a":
      return 1;
    case "z":
      return 2;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    let ts2678 = codes.iter().filter(|&&c| c == 2678).count();
    assert_eq!(
        ts2678, 1,
        "exactly one TS2678 expected (`\"z\"` out of constraint, `\"a\"` in); got: {codes:?}"
    );
}

/// `NoInfer<>` over a concrete literal union (no type parameter) was already
/// correct; keep it as a control: in-union literal is clean, out-of-union is
/// still TS2367.
#[test]
fn noinfer_over_concrete_union_preserves_overlap_decisions() {
    let ok = check_codes(
        r#"
function f(input: NoInfer<"a" | "b">) {
  if (input === "a") {}
}
"#,
    );
    assert!(
        !ok.contains(&2367),
        "`\"a\"` overlaps `\"a\" | \"b\"`; expected no TS2367, got: {ok:?}"
    );
    let bad = check_codes(
        r#"
function f(input: NoInfer<"a" | "b">) {
  if (input === "c") {}
}
"#,
    );
    assert!(
        bad.contains(&2367),
        "`\"c\"` does not overlap `\"a\" | \"b\"`; expected TS2367, got: {bad:?}"
    );
}

/// Control: the same comparison without the `NoInfer` wrapper has always been
/// accepted; the wrapped form must now match it.
#[test]
fn bare_constrained_param_equality_against_constraint_member_no_ts2367() {
    let source = r#"
function h<T extends string | true>(input: T) {
  if (input === true) {
    return 1;
  }
  return 0;
}
"#;
    let codes = check_codes(source);
    assert!(
        !codes.contains(&2367),
        "bare `T` against a constraint member must not emit TS2367, got: {codes:?}"
    );
}
