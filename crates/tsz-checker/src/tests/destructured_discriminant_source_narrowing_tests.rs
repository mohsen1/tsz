//! Narrowing the source binding when a destructured discriminant is narrowed.
//!
//! When `const { kind } = s` destructures a discriminated union and `kind` is
//! narrowed by a condition (e.g., `if (kind === "a")`), the source binding `s`
//! should be narrowed to the matching union variant, making variant-specific
//! properties accessible without a TS2339 error.

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

// ── Core repro from the issue ──────────────────────────────────────────────

#[test]
fn basic_discriminant_narrows_source() {
    // `s` should be narrowed to `{ kind: "a"; a: number }` inside the `if`.
    let diags = diags(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
  const { kind } = s;
  if (kind === "a") {
    s.a;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "Expected no TS2339 for `s.a` inside `if (kind === 'a')`; got: {diags:?}"
    );
}

#[test]
fn false_branch_narrows_to_other_variant() {
    // In the else branch, `kind !== "a"`, so `s` is `{ kind: "b"; b: string }`.
    let diags = diags(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
  const { kind } = s;
  if (kind === "a") {
    s.a;
  } else {
    s.b;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "Expected no TS2339 for `s.a` or `s.b`; got: {diags:?}"
    );
}

// ── Renamed destructure ────────────────────────────────────────────────────

#[test]
fn renamed_discriminant_narrows_source() {
    // `const { kind: k } = s` — the binding name is `k` but property is `kind`.
    let diags = diags(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
  const { kind: k } = s;
  if (k === "a") {
    s.a;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        !cs.contains(&2339),
        "Expected no TS2339 for renamed destructure `{{ kind: k }}`; got: {diags:?}"
    );
}

// ── No false positives ─────────────────────────────────────────────────────

#[test]
fn wrong_branch_variant_property_still_errors() {
    // `s.b` inside `if (kind === "a")` should still be TS2339.
    let diags = diags(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
  const { kind } = s;
  if (kind === "a") {
    s.b;
  }
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        cs.contains(&2339),
        "Expected TS2339 for `s.b` inside `if (kind === 'a')`; got: {diags:?}"
    );
}

#[test]
fn unnarrowed_source_keeps_union_type() {
    // Outside any discriminant check, `s` should still be the full union.
    let diags = diags(
        r#"
type S = { kind: "a"; a: number } | { kind: "b"; b: string };
function f(s: S) {
  const { kind } = s;
  s.a;
}
"#,
    );
    let cs = codes(&diags);
    assert!(
        cs.contains(&2339),
        "Expected TS2339 for `s.a` without narrowing; got: {diags:?}"
    );
}
