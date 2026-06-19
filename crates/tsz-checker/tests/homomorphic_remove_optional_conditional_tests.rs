//! Tests for homomorphic `-?` mapped types where the value template is a
//! conditional type that references `T[K]` (issue #9759).
//!
//! Structural rule: when a homomorphic mapped type has `-?` (remove optional),
//! tsc instantiates the template with the *read* type `T[K]` — which still
//! includes `| undefined` for an optional source key — and only removes the
//! resulting top-level `undefined` from the final property type afterwards
//! (`getTypeOfMappedSymbol` -> `getTypeWithFacts(type, NEUndefined)`). The
//! `undefined` is therefore visible to the template: a conditional check
//! `T[K] extends X` sees `Declared | undefined`, and a distributive template
//! `V extends W<infer U> ? ...` distributes over the `undefined` member. After
//! the template runs the top-level `undefined` is stripped, so for a template
//! that simply returns `T[K]` the net result is the de-optionalized type.

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
/// Source: `{ x?: number }`. The check `(number | undefined) extends string` is
/// `false`, so the value branch `T[X]` (= `number | undefined`) is taken; the
/// `-?` strip then removes the top-level `undefined`, leaving `number`.
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
// The conditional *check* sees `Declared | undefined`
// ---------------------------------------------------------------------------

/// Even with constant branches (`true`/`false`), the conditional's CHECK is
/// `T[K] extends number`, and `-?` does not de-optionalize the type fed into
/// the template: the check sees the read type `number | undefined`, which does
/// NOT extend `number`, so the branch taken is `false`. tsc evaluates `r.a` and
/// `r.b` as `false` (verified against tsc 6.0.x); a previous version of this
/// test asserted `r.a: true`, encoding tsz's old behavior of feeding the
/// de-optionalized declared type into the template.
#[test]
fn remove_optional_conditional_check_sees_optional_undefined() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends number ? true : false };
type R = M<{ a?: number; b?: string }>;
declare const r: R;
const a: false = r.a;
const b: false = r.b;
"#,
    );
    // The opposite annotation must now be a type error: `r.a` is `false`,
    // not `true`.
    has_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends number ? true : false };
type R = M<{ a?: number }>;
declare const r: R;
const a: true = r.a;
"#,
    );
}

/// A genuinely undefined-insensitive check confirms the strip still happens:
/// `number | undefined extends string | number | undefined` is `true` whether
/// or not `undefined` is present, so the value branch is selected and its
/// constant result is undefined-free.
#[test]
fn remove_optional_conditional_undefined_insensitive_check() {
    no_errors(
        r#"
type M<T> = { [K in keyof T]-?: T[K] extends string | number | undefined ? "yes" : "no" };
type R = M<{ a?: number; b?: string }>;
declare const r: R;
const a: "yes" = r.a;
const b: "yes" = r.b;
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

/// Template is a union `T[K] | null` (not a conditional). The template sees the
/// read type `number | undefined`, producing `number | undefined | null`; the
/// `-?` strip removes the top-level `undefined`, leaving `number | null`.
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
