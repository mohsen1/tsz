//! Tests for optional-property subtype compatibility.
//!
//! Rule under test:
//!
//! > In standard mode (no `exactOptionalPropertyTypes`), an optional source property
//! > `{a?: T}` IS a subtype of a required target `{a: T | undefined}`, because
//! > `optional_property_type()` widens the source type to `T | undefined`, making
//! > the type comparison `T | undefined <: T | undefined` = true.
//! >
//! > In `exactOptionalPropertyTypes` mode, an optional source property cannot
//! > satisfy a required target property at all (absence is disallowed).
//!
//! These tests prove the fix is structural (not name-specific): they vary
//! property names, type parameter names, and shapes.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_codes, check_with_options, has_diagnostic_code};

// ── Standard mode ────────────────────────────────────────────────────────────

/// `{a?: string}` is a subtype of `{a: string | undefined}` in standard mode.
/// tsc evaluates the conditional as `true`.
#[test]
fn optional_source_is_subtype_of_required_with_undefined_standard_mode() {
    let source = r#"
type A = { a?: string };
type B = { a: string | undefined };
type C = A extends B ? true : false;
const c: true = (null as any as C);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: {{a?: string}} should be subtype of {{a: string | undefined}} in standard mode"
    );
}

/// `{flag?: number}` is a subtype of `{flag: number | undefined}` — same rule
/// with different property name, verifying the fix is not name-hardcoded.
#[test]
fn optional_source_subtype_of_required_undefined_alt_property_name() {
    let source = r#"
type X = { flag?: number };
type Y = { flag: number | undefined };
type R = X extends Y ? true : false;
const r: true = (null as any as R);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: {{flag?: number}} should be subtype of {{flag: number | undefined}} in standard mode"
    );
}

/// `{a?: string}` is NOT a subtype of `{a: string}` in standard mode.
/// The widened source type is `string | undefined`, which is not `<: string`.
#[test]
fn optional_source_is_not_subtype_of_required_without_undefined() {
    let source = r#"
type A = { a?: string };
type B = { a: string };
type C = A extends B ? true : false;
const c: false = (null as any as C);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: {{a?: string}} should NOT be subtype of {{a: string}} in standard mode"
    );
}

/// Generic optional property: `{prop?: T}` is a subtype of `{prop: T | undefined}`
/// for any `T`, confirming the rule applies to type-parameterized shapes.
#[test]
fn generic_optional_source_subtype_of_required_with_undefined() {
    let source = r#"
type WithOptional<T> = { prop?: T };
type WithRequired<T> = { prop: T | undefined };
type Check<T> = WithOptional<T> extends WithRequired<T> ? true : false;
type Result = Check<string>;
const r: true = (null as any as Result);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: generic optional property should be subtype of required with undefined"
    );
}

/// Multiple optional properties: all must satisfy widened required targets.
#[test]
fn multiple_optional_properties_subtype_of_required_with_undefined() {
    let source = r#"
type Src = { x?: string; y?: number };
type Tgt = { x: string | undefined; y: number | undefined };
type R = Src extends Tgt ? true : false;
const r: true = (null as any as R);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: multiple optional properties should be subtypes of required-with-undefined"
    );
}

/// Assignment: a value with optional properties should be assignable to a
/// variable type with required-but-possibly-undefined properties.
#[test]
fn assignment_optional_to_required_with_undefined_standard_mode() {
    let source = r#"
type Src = { a?: string };
type Tgt = { a: string | undefined };
declare const src: Src;
const tgt: Tgt = src;
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: assignment of optional-property type to required-undefined-property type"
    );
}

// ── exactOptionalPropertyTypes mode ──────────────────────────────────────────

/// With `exactOptionalPropertyTypes`, `{a?: string}` must NOT be a subtype of
/// `{a: string | undefined}`, because the target is required (must be present).
#[test]
fn optional_source_is_not_subtype_of_required_with_undefined_exact_mode() {
    let source = r#"
type A = { a?: string };
type B = { a: string | undefined };
const b: B = (null as any as A);
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = check_with_options(source, options);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "expected TS2322: with exactOptionalPropertyTypes, {{a?: string}} should not be assignable to {{a: string | undefined}}"
    );
}

/// With `exactOptionalPropertyTypes`, `{flag?: number}` must NOT be assignable
/// to `{flag: number | undefined}` — same rule with different property name.
#[test]
fn exact_optional_mode_rejects_optional_to_required_with_undefined_alt_name() {
    let source = r#"
type P = { flag?: number };
type Q = { flag: number | undefined };
const q: Q = (null as any as P);
"#;
    let options = CheckerOptions {
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = check_with_options(source, options);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "expected TS2322 with exactOptionalPropertyTypes for {{flag?: number}} assigned to {{flag: number | undefined}}"
    );
}

/// Standard mode: no error for conditional extends (canonical `propTypeValidatorInference` pattern).
/// Optional-property mapped types from `@types/prop-types` use
/// `{[K in keyof V]?: ...} extends {[K in keyof V]: ...}` conditional patterns
/// that require optional-to-required-with-undefined subtyping.
#[test]
fn prop_types_style_optional_required_conditional_pattern() {
    let source = r#"
type RequiredKeys<V> = {
    [K in keyof V]-?: undefined extends V[K] ? never : K
}[keyof V];

type IsRequired<V, K extends keyof V> = K extends RequiredKeys<V> ? true : false;

type Obj = { a: string; b?: number };
type AIsRequired = IsRequired<Obj, 'a'>;
type BIsRequired = IsRequired<Obj, 'b'>;

const _a: true = (null as any as AIsRequired);
const _b: false = (null as any as BIsRequired);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors for prop-types-style optional/required detection pattern"
    );
}
