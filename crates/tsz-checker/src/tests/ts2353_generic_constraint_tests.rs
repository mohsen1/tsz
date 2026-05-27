//! Tests for TS2353 excess-property checking on fresh object literals passed
//! to generic parameters with constraints (`T extends C`).
//!
//! Rule: when a fresh object literal is passed to a generic `T extends C`
//! parameter and the literal has properties not in C, tsc reports TS2353 at
//! the excess property instead of TS2345 at the argument span.

use crate::test_utils::check_source_diagnostics;

// ── Repro (issue #9728) ───────────────────────────────────────────────────

/// Primary repro: missing required property + excess property.
/// tsc: TS2353 at `name` (Object literal may only specify known properties).
/// tsz before fix: TS2345 at the argument span (wrong code and wrong span).
#[test]
fn ts2353_generic_constraint_excess_property_repro() {
    let diags = check_source_diagnostics(
        r#"
declare function g<T extends { id: number }>(x: T): T;
g({ name: "a" });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "expected TS2353 for excess property in generic-constrained arg; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 should be suppressed when TS2353 fires; got: {codes:?}"
    );
}

/// Renamed type parameter proves the rule is structural, not spelling-specific.
#[test]
fn ts2353_generic_constraint_excess_property_renamed_param() {
    let diags = check_source_diagnostics(
        r#"
declare function transform<U extends { count: number }>(input: U): U;
transform({ extra: "a" });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "TS2353 must fire for renamed type param; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 must be suppressed when TS2353 fires (renamed param); got: {codes:?}"
    );
}

/// Renamed constraint properties — same structural rule.
#[test]
fn ts2353_generic_constraint_excess_property_renamed_constraint_props() {
    let diags = check_source_diagnostics(
        r#"
declare function wrap<K extends { label: string }>(value: K): K;
wrap({ unknown: 1 });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "TS2353 must fire for renamed constraint properties; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 must be suppressed (renamed constraint props); got: {codes:?}"
    );
}

/// Constraint with optional property: excess property on unknown prop.
#[test]
fn ts2353_generic_constraint_optional_prop_excess() {
    let diags = check_source_diagnostics(
        r#"
declare function maybe<T extends { id?: number }>(x: T): T;
maybe({ extra: "a" });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "TS2353 must fire for excess property when constraint has optional prop; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 must be suppressed (optional constraint); got: {codes:?}"
    );
}

// ── Negative / regression guards ─────────────────────────────────────────

/// Non-generic parameter already works (regression guard — must stay clean).
#[test]
fn ts2353_non_generic_excess_property_regression_guard() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: { id: number }): void;
f({ name: "a" });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "non-generic excess-property check must still fire (regression guard); got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "TS2345 must be suppressed by TS2353 in non-generic case; got: {codes:?}"
    );
}

/// Literal satisfies the constraint (has `id`) with an extra property:
/// tsc accepts this because generic inference allows widening to a structural
/// supertype. No error expected from either tsc or tsz.
#[test]
fn ts2353_generic_constraint_satisfied_plus_extra_is_clean() {
    let diags = check_source_diagnostics(
        r#"
declare function g<T extends { id: number }>(x: T): T;
g({ id: 1, name: "a" });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "no error expected when constraint is satisfied and extra props are present; got: {codes:?}"
    );
}

/// Correct-arity non-generic call: no error.
#[test]
fn ts2353_non_generic_correct_literal_is_clean() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: { id: number }): void;
f({ id: 42 });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "correct literal must produce no error (non-generic); got: {codes:?}"
    );
}

/// Unconstrained generic `T` (no `extends` clause): no TS2353 expected,
/// because there is no constraint type to check excess properties against.
#[test]
fn ts2353_generic_no_constraint_no_excess_error() {
    let diags = check_source_diagnostics(
        r#"
declare function identity<T>(x: T): T;
identity({ anyProp: true });
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "unconstrained generic must not emit TS2353; got: {codes:?}"
    );
}
