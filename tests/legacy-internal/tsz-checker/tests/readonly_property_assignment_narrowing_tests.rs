//! Assignment-narrowing of a property/element-access reference must follow
//! tsc's `getTypeAtFlowAssignment`: a *union* declared type is reduced to the
//! members compatible with the assigned value (`getAssignmentReducedType`),
//! while a *non-union* declared type is kept verbatim — the reference does not
//! adopt the right-hand-side shape.
//!
//! Regression for the readonly soundness gap: after `r.a = { b: 2 }` tsz used
//! to set the flow type of `r.a` to the fresh object-literal type `{ b: number }`
//! (mutable), dropping the declared nested `readonly`, so the following
//! `r.a.b = 3` was no longer flagged TS2540 (false negative). tsc keeps the
//! declared `{ readonly b: number }`, so the nested write stays an error.
//!
//! The same rule also fixes two adjacent divergences that fell out of the same
//! code path: a non-union object property was over-narrowed to the assigned
//! shape (dropping declared property unions), and a literal-union property was
//! under-narrowed (the assigned literal failed to reduce the union).
//!
//! Tests vary binder names so the fix is keyed on shape, not identifier
//! spelling (CLAUDE.md anti-hardcoding checklist).

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

fn count(diags: &[crate::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

// ── Readonly soundness floor (the reported bug) ────────────────────────────

#[test]
fn nested_readonly_survives_object_literal_write() {
    // `r.a = { b: 2 }` is an invalid readonly write (TS2540 on `a`); it must NOT
    // narrow `r.a` to the mutable RHS shape, so `r.a.b = 3` is still TS2540.
    let diags = diags(
        r#"
interface R { readonly a: { readonly b: number } }
declare const r: R;
r.a = { b: 2 };
r.a.b = 3;
"#,
    );
    assert_eq!(
        count(&diags, 2540),
        2,
        "both the outer `a` and the nested `b` write must report TS2540; got: {diags:?}"
    );
}

#[test]
fn nested_readonly_survives_when_outer_is_mutable() {
    // Outer `a` is mutable, so `r.a = { b: 2 }` is a *valid* write — but the
    // declared `{ readonly b: number }` is non-union, so the reference keeps it
    // and the nested `r.a.b = 3` still reports TS2540 (matches tsc).
    let diags = diags(
        r#"
interface Outer { a: { readonly b: number } }
declare const outer: Outer;
outer.a = { b: 2 };
outer.a.b = 3;
"#,
    );
    assert_eq!(
        count(&diags, 2540),
        1,
        "the valid outer write must not strip nested readonly; got: {diags:?}"
    );
}

#[test]
fn nested_readonly_via_mapped_type_survives_write() {
    // An inline readonly mapped type reproduces the same gap once evaluated
    // (equivalent to the lib `Readonly<...>` utility, but self-contained so the
    // unit-test harness need not resolve the standard library).
    let diags = diags(
        r#"
type RO<T> = { readonly [K in keyof T]: T[K] };
type Cell = RO<{ b: number }>;
type Container = RO<{ a: Cell }>;
declare const ro: Container;
ro.a = { b: 2 };
ro.a.b = 3;
"#,
    );
    assert_eq!(
        count(&diags, 2540),
        2,
        "mapped-readonly nested write must report TS2540 twice; got: {diags:?}"
    );
}

#[test]
fn constructor_readonly_object_write_keeps_nested_readonly() {
    // A readonly object property assigned in its own constructor is a *valid*
    // write, but the nested `readonly y` is non-union and survives, so
    // `this.x.y = 2` still reports TS2540 (matches tsc).
    let diags = diags(
        r#"
class Box {
  readonly x: { readonly y: number };
  constructor() {
    this.x = { y: 1 };
    this.x.y = 2;
  }
}
"#,
    );
    assert_eq!(
        count(&diags, 2540),
        1,
        "nested readonly must survive a valid constructor write; got: {diags:?}"
    );
}

// ── Value narrowing must still match tsc (no regression) ───────────────────

#[test]
fn constructor_primitive_union_property_still_narrows() {
    // `this.v = 5` on a `number | undefined` (union) member must narrow to
    // `number`, so the following member access is sound (no TS18048).
    let diags = diags(
        r#"
class Holder {
  readonly v: number | undefined;
  constructor() {
    this.v = 5;
    this.v.toFixed();
  }
}
"#,
    );
    assert!(
        codes(&diags).is_empty(),
        "valid readonly ctor write must still narrow the union member; got: {diags:?}"
    );
}

#[test]
fn primitive_union_property_reduces_to_assigned_member() {
    // Declared union `number | string`; after `r.p = 5`, the reference reduces
    // to `number`, so assigning it to `string` is TS2322.
    let diags = diags(
        r#"
interface Cfg { p: number | string }
declare const cfg: Cfg;
cfg.p = 5;
const s: string = cfg.p;
"#,
    );
    assert_eq!(
        count(&diags, 2322),
        1,
        "union property must reduce to the assigned primitive; got: {diags:?}"
    );
}

#[test]
fn literal_union_property_reduces_to_assigned_literal() {
    // Declared `1 | 2 | 3`; `n.value = 2` reduces the union to `2`
    // (`getAssignmentReducedType`), so `const one: 1 = n.value` is TS2322 while
    // `const two: 2 = n.value` is accepted.
    let diags = diags(
        r#"
interface Num { value: 1 | 2 | 3 }
declare const n: Num;
n.value = 2;
const one: 1 = n.value;
const two: 2 = n.value;
"#,
    );
    assert_eq!(
        count(&diags, 2322),
        1,
        "literal union must reduce to exactly the assigned member; got: {diags:?}"
    );
}

#[test]
fn nonunion_object_property_keeps_declared_member_unions() {
    // Declared `{ b: number | string }` is non-union at the property level, so
    // `r.a = { b: 5 }` must NOT narrow `r.a.b` to `number`; reading it as
    // `string` stays a `string | number` mismatch (matches tsc).
    let diags = diags(
        r#"
interface Wrap { a: { b: number | string } }
declare const w: Wrap;
w.a = { b: 5 };
const s: string = w.a.b;
"#,
    );
    assert_eq!(
        count(&diags, 2322),
        1,
        "non-union object property must keep its declared member union; got: {diags:?}"
    );
}
