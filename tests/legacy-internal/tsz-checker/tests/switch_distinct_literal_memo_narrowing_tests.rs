//! Behavior-preservation tests for the per-switch all-distinct-literals memo
//! (`switch_case_block_all_distinct_literals`).
//!
//! `narrow_by_switch_case_clause` takes a no-exclusion fast path when every
//! earlier case is a distinct literal. That predecessor check used to re-scan
//! all earlier clauses on every clause (O(N^2) over an N-arm switch); it is now
//! decided once per switch via a memo. These tests pin the *narrowing result*
//! so the optimization cannot drift from the per-clause behavior: each arm of a
//! distinct-literal discriminated-union switch must narrow to exactly its
//! variant (matching properties accessible, foreign properties TS2339), and the
//! mixed-switch fallback (default clause, duplicate labels) must be unchanged.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn distinct_literal_switch_narrows_each_arm_to_its_variant() {
    // Multi-arm switch where every label is a distinct literal: each arm must
    // see only its own variant's properties.
    let cs = codes(&diags(
        r#"
type Ev =
  | { kind: "a"; av: number }
  | { kind: "b"; bv: string }
  | { kind: "c"; cv: boolean };
function f(e: Ev): unknown {
  switch (e.kind) {
    case "a": return e.av;
    case "b": return e.bv;
    case "c": return e.cv;
  }
}
"#,
    ));
    assert!(
        cs.is_empty(),
        "distinct-literal arms must narrow without diagnostics; got: {cs:?}"
    );
}

#[test]
fn distinct_literal_switch_rejects_foreign_member_access() {
    // Accessing another variant's property inside a narrowed arm must still
    // produce TS2339 — the fast path must not widen the narrowed type.
    let cs = codes(&diags(
        r#"
type Ev =
  | { kind: "a"; av: number }
  | { kind: "b"; bv: string };
function f(e: Ev): unknown {
  switch (e.kind) {
    case "a": return e.bv; // bv not on { kind: "a" }
    case "b": return e.av; // av not on { kind: "b" }
  }
}
"#,
    ));
    assert_eq!(
        cs.iter().filter(|&&c| c == 2339).count(),
        2,
        "each foreign-member access must emit TS2339; got: {cs:?}"
    );
}

#[test]
fn distinct_literal_switch_with_renamed_binders_narrows() {
    // Anti-hardcoding: rename the binder/discriminant/labels — behavior must be
    // structural, not name-driven.
    let cs = codes(&diags(
        r#"
type Shape =
  | { tag: "alpha"; px: number }
  | { tag: "beta"; py: string };
function g(zzz: Shape): unknown {
  switch (zzz.tag) {
    case "alpha": return zzz.px;
    case "beta": return zzz.py;
  }
}
"#,
    ));
    assert!(
        cs.is_empty(),
        "renamed distinct-literal switch must still narrow; got: {cs:?}"
    );
}

#[test]
fn switch_with_default_narrows_default_branch() {
    // The memo declines (default clause present), so the per-clause fallback
    // runs. The default branch must narrow to the remaining variant.
    let cs = codes(&diags(
        r#"
type Ev =
  | { kind: "a"; av: number }
  | { kind: "b"; bv: string }
  | { kind: "c"; cv: boolean };
function f(e: Ev): unknown {
  switch (e.kind) {
    case "a": return e.av;
    case "b": return e.bv;
    default: return e.cv; // narrowed to { kind: "c" }
  }
}
"#,
    ));
    assert!(
        cs.is_empty(),
        "default branch must narrow to the residual variant; got: {cs:?}"
    );
}

#[test]
fn switch_with_default_rejects_foreign_member_in_default() {
    let cs = codes(&diags(
        r#"
type Ev =
  | { kind: "a"; av: number }
  | { kind: "b"; bv: string }
  | { kind: "c"; cv: boolean };
function f(e: Ev): unknown {
  switch (e.kind) {
    case "a": return e.av;
    case "b": return e.bv;
    default: return e.bv; // bv not on { kind: "c" }
  }
}
"#,
    ));
    assert_eq!(
        cs.iter().filter(|&&c| c == 2339).count(),
        1,
        "default-branch foreign access must emit TS2339; got: {cs:?}"
    );
}

#[test]
fn direct_switch_expression_narrows_distinct_literals() {
    // The switch expression is itself the reference (not a discriminant
    // property): the all-distinct-literals fast path applies to the union of
    // string literals directly.
    let cs = codes(&diags(
        r#"
function f(s: "a" | "b" | "c"): number {
  switch (s) {
    case "a": return 1;
    case "b": return 2;
    case "c": return 3;
  }
}
"#,
    ));
    assert!(
        cs.is_empty(),
        "exhaustive distinct-literal switch must type-check; got: {cs:?}"
    );
}
