//! Tests for homomorphic `-?` mapped types where the value template is a
//! conditional type that references `T[K]` (issue #9759).
//!
//! Structural rule: when a homomorphic mapped type has `-?` (remove optional)
//! and the template contains `T[K]`, tsc evaluates `T[K]` as the DECLARED
//! property type (without `| undefined` from optionality), not the read type.
//! This ensures the conditional result does not inadvertently carry `undefined`
//! from the source property's optional nature.

use tsz_checker::test_utils::check_source_diagnostics;

fn no_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no diagnostics, got:\n{relevant:#?}\nSource:\n{source}"
    );
}

fn has_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        !relevant.is_empty(),
        "Expected at least one diagnostic, got none\nSource:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// Reported repro: T[K] extends object conditional (issue #9759)
// ---------------------------------------------------------------------------

/// Primary repro: `-?` with a conditional value type that returns `T[K]` in
/// both branches. tsc: no error; tsz was emitting TS2322 (false positive).
#[test]
fn remove_optional_with_conditional_object_extends_no_error() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends object ? T[K] : T[K] };
type R = M<{ a?: { b: number } }>;
declare const r: R;
const a: { b: number } = r.a;
"#,
    );
}

/// Same rule, renamed iteration variable `P` instead of `K`.
/// The fix must be structural (keyed on semantics, not the variable name).
#[test]
fn remove_optional_with_conditional_renamed_var_no_error() {
    no_errors(
        r#"
type M<T> = { [P in keyof T]-?: T[P] extends object ? T[P] : T[P] };
type R = M<{ x?: { b: number } }>;
declare const r: R;
const x: { b: number } = r.x;
"#,
    );
}

/// Renamed iteration variable `X`, primitive conditional (`extends string`).
/// Source: `{ x?: number }`. Declared type is `number`.
/// Template: `T[X] extends string ? "s" : T[X]` with declared `number` → `number`.
#[test]
fn remove_optional_primitive_conditional_renamed_var_no_error() {
    no_errors(
        r#"
type M<T> = { [X in keyof T]-?: T[X] extends string ? "s" : T[X] };
type R = M<{ x?: number }>;
declare const r: R;
const x: number = r.x;
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative control: without -? the optional undefined is preserved
// ---------------------------------------------------------------------------

/// Without `-?`, the optional source property keeps `| undefined`. The
/// property remains optional and an assignment without `| undefined` is
/// a type error. (Proves `-?` is the operator driving the de-optionalization.)
#[test]
fn without_remove_optional_keeps_undefined_is_error() {
    has_errors(
        r#"
type M<T> = { [K in keyof T]: T[K] extends object ? T[K] : T[K] };
type R = M<{ a?: { b: number } }>;
declare const r: R;
const a: { b: number } = r.a;
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative control: conditional that never re-introduces the source value
// ---------------------------------------------------------------------------

/// When the conditional's value never flows back from `T[K]` (both branches
/// are constant), the output type is determined by the conditional alone —
/// no undefined can creep in from optionality. Both tsz and tsc give the
/// same result regardless of `-?` on the source.
#[test]
fn remove_optional_conditional_constant_branches_no_error() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends number ? true : false };
type R = M<{ a?: number; b?: string }>;
declare const r: R;
const a: true = r.a;
const b: false = r.b;
"#,
    );
}

// ---------------------------------------------------------------------------
// Multi-property source: each property is de-optionalized independently
// ---------------------------------------------------------------------------

/// Source with multiple optional properties of different types. Each is
/// de-optionalized independently; the conditional result must not carry
/// undefined from any of them.
#[test]
fn remove_optional_multi_property_conditional_no_error() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends object ? T[K] : T[K] };
type R = M<{ a?: { b: number }; c?: { d: string } }>;
declare const r: R;
const a: { b: number } = r.a;
const c: { d: string } = r.c;
"#,
    );
}

// ---------------------------------------------------------------------------
// Non-optional source properties are unaffected
// ---------------------------------------------------------------------------

/// Required properties (not optional) should not be affected by `-?`.
/// Their declared type is used as-is; no `| undefined` was ever added.
#[test]
fn remove_optional_required_properties_unaffected() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends object ? T[K] : T[K] };
type R = M<{ a: { b: number }; c: number }>;
declare const r: R;
const a: { b: number } = r.a;
const c: number = r.c;
"#,
    );
}

// ---------------------------------------------------------------------------
// Mixed optional and required properties
// ---------------------------------------------------------------------------

/// Source with a mix of optional and required properties. The fix must
/// de-optionalize only the optional ones, leave required ones untouched.
#[test]
fn remove_optional_mixed_optional_required_no_error() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends object ? T[K] : T[K] };
type R = M<{ a?: { b: number }; c: string }>;
declare const r: R;
const a: { b: number } = r.a;
const c: string = r.c;
"#,
    );
}

// ---------------------------------------------------------------------------
// Union template: T[K] | null with -?
// ---------------------------------------------------------------------------

/// Template is a union `T[K] | null` (not a conditional). The `T[K]` part
/// must use the declared type when `-?` is present.
#[test]
fn remove_optional_union_template_no_error() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] | null };
type R = M<{ a?: number }>;
declare const r: R;
const a: number | null = r.a;
"#,
    );
}
