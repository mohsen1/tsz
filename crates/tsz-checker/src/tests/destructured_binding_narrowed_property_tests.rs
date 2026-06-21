//! Flow-narrowing of an object-binding-pattern element through a truthiness /
//! type guard on the source property.
//!
//! When a property is narrowed by a guard (e.g. `if (!obj.p) return;`) and then
//! destructured (`const { p } = obj`), tsc gives the binding element the
//! narrowed apparent-property type, exactly as direct member access (`obj.p`)
//! would. tsz computed the binding element type from the source object's
//! *declared* type, then relied on `narrow_destructured_binding_via_source` to
//! re-apply the guard's narrowing at the binding's use site. That re-narrowing
//! walked the flow condition chain but unwrapped a `PrefixUnaryExpression`
//! (`!obj.p`) with the wrong arena accessor, so it never reached the property
//! access and the element kept its un-narrowed declared type — yielding false
//! TS2322 / TS18047.
//!
//! The fix reads the prefix-unary operand via `get_unary_expr` (the
//! `UnaryExprData` pool) instead of `get_unary_expr_ex`. These tests pin the
//! structural rule and vary the binder names so the fix is keyed on shape, not
//! identifier spelling (CLAUDE.md anti-hardcoding checklist).

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

// ── Positives: guard narrows the destructured element ──────────────────────

#[test]
fn truthiness_guard_narrows_destructured_element() {
    // `if (!input.query) return;` narrows `input.query` to `string`, so the
    // destructured `query` must be `string`, not `string | null`.
    let diags = diags(
        r#"
function f(input: { query: string | null }) {
  if (!input.query) return;
  const { query } = input;
  const x: string = query;
  return x;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "destructured `query` should be narrowed to `string`; got: {diags:?}"
    );
}

#[test]
fn truthiness_guard_narrows_renamed_element() {
    // Renamed binder (`query: q`) — fix is keyed on structure, not spelling.
    let diags = diags(
        r#"
function f(input: { query: string | null }) {
  if (!input.query) return;
  const { query: q } = input;
  const x: string = q;
  return x;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "renamed destructured `q` should be narrowed to `string`; got: {diags:?}"
    );
}

#[test]
fn truthiness_guard_narrows_renamed_element_alt_names() {
    // Different identifier spellings entirely — still narrowed.
    let diags = diags(
        r#"
function g(payload: { token: string | null }) {
  if (!payload.token) return;
  const { token: secret } = payload;
  const x: string = secret;
  return x;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "renamed destructured `secret` should be narrowed to `string`; got: {diags:?}"
    );
}

#[test]
fn positive_if_block_guard_narrows_element() {
    let diags = diags(
        r#"
function f(input: { query: string | null }) {
  if (input.query) {
    const { query } = input;
    const x: string = query;
    return x;
  }
  return "";
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "destructured `query` inside positive guard should be `string`; got: {diags:?}"
    );
}

#[test]
fn typeof_guard_narrows_destructured_element() {
    let diags = diags(
        r#"
function f(input: { val: string | number }) {
  if (typeof input.val !== "string") return;
  const { val } = input;
  const x: string = val;
  return x;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "destructured `val` should be narrowed to `string` via typeof; got: {diags:?}"
    );
}

#[test]
fn equality_null_guard_narrows_destructured_element() {
    let diags = diags(
        r#"
function f(input: { p: string | null }) {
  if (input.p === null) return;
  const { p } = input;
  const x: string = p;
  return x;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "destructured `p` should be narrowed to `string` via `=== null` guard; got: {diags:?}"
    );
}

#[test]
fn nested_guard_narrows_nested_destructured_element() {
    let diags = diags(
        r#"
function f(input: { a: { b: number | undefined } }) {
  if (input.a.b === undefined) return;
  const { a: { b } } = input;
  const y: number = b;
  return y;
}
"#,
    );
    assert!(
        !codes(&diags).contains(&2322),
        "nested destructured `b` should be narrowed to `number`; got: {diags:?}"
    );
}

// ── Negatives: narrowing must NOT apply ────────────────────────────────────

#[test]
fn no_guard_keeps_declared_type() {
    // Without a guard, `query` is `string | null` and the assignment errors.
    let diags = diags(
        r#"
function f(input: { query: string | null }) {
  const { query } = input;
  const x: string = query;
  return x;
}
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "without a guard `query` is `string | null` and must error; got: {diags:?}"
    );
}

#[test]
fn guard_on_other_property_keeps_declared_type() {
    // Guarding `a` does not narrow the destructured `b`.
    let diags = diags(
        r#"
function f(input: { a: string | null; b: string | null }) {
  if (!input.a) return;
  const { b } = input;
  const x: string = b;
  return x;
}
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "guarding `a` must not narrow `b`; got: {diags:?}"
    );
}

#[test]
fn destructured_sibling_unchanged() {
    // The narrowed property is `query`; an unrelated sibling keeps its type.
    let diags = diags(
        r#"
function f(input: { query: string | null; other: number }) {
  if (!input.query) return;
  const { other } = input;
  const x: string = other;
  return x;
}
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "`other: number` must not be assignable to `string`; got: {diags:?}"
    );
}

#[test]
fn parameter_destructure_unchanged() {
    // Function-parameter destructuring has no narrowing; declared type holds.
    let diags = diags(
        r#"
function f({ query }: { query: string | null }) {
  const x: string = query;
  return x;
}
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "parameter destructure keeps declared `string | null`; got: {diags:?}"
    );
}
