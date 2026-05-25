//! Object-spread results over discriminated unions should remain narrowable.
//!
//! When `{ ...x }` spreads a discriminated union, tsc preserves the correlation
//! between the discriminant property and sibling properties. A later
//! discriminant check on the inferred spread result should narrow the whole
//! spread binding, not only the discriminant property itself.

use tsz_common::options::checker::CheckerOptions;

fn diagnostics(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(source: &str) -> Vec<u32> {
    diagnostics(source).into_iter().map(|d| d.code).collect()
}

#[test]
fn object_spread_preserves_two_member_discriminant_correlation() {
    let codes = codes(
        r#"
type T = { k: "a"; a: number } | { k: "b"; b: string };
function f(x: T) {
  const y = { ...x };
  if (y.k === "a") {
    const t: number = y.a;
  }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "spread result should narrow to the `k === \"a\"` member; got codes: {codes:?}"
    );
}

#[test]
fn object_spread_preserves_renamed_three_member_discriminant_correlation() {
    let codes = codes(
        r#"
type T =
  | { tag: "add"; value: number }
  | { tag: "remove"; id: string }
  | { tag: "clear"; all: true };
function f(x: T) {
  const y = { ...x };
  if (y.tag === "remove") {
    const id: string = y.id;
  }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "spread result should narrow structurally for renamed discriminants; got codes: {codes:?}"
    );
}

#[test]
fn object_spread_wrong_branch_member_still_errors() {
    let codes = codes(
        r#"
type T = { k: "a"; a: number } | { k: "b"; b: string };
function f(x: T) {
  const y = { ...x };
  if (y.k === "a") {
    y.b;
  }
}
"#,
    );

    assert!(
        codes.contains(&2339),
        "spread result should still reject a member from the wrong discriminant branch; got codes: {codes:?}"
    );
}
