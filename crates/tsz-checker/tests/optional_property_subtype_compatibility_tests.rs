//! Tests for optional-property subtype compatibility.
//!
//! Rule under test:
//!
//! Optional source properties do not satisfy required target properties, even
//! when the required property type explicitly includes `undefined`. In standard
//! mode, `optional_property_type()` widens the read type to `T | undefined`;
//! it does not make an absent property count as present.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_codes, check_with_options, has_diagnostic_code};

// ── Standard mode ────────────────────────────────────────────────────────────

#[test]
fn optional_source_is_not_subtype_of_required_with_undefined_standard_mode() {
    let source = r#"
type A = { a?: string };
type B = { a: string | undefined };
type C = A extends B ? true : false;
const c: false = (null as any as C);
declare const a: A;
const b: B = a;
"#;
    let diagnostics = check_source_codes(source);
    assert!(
        diagnostics.contains(&2322),
        "expected TS2322 for assigning optional source to required-with-undefined target. Got: {diagnostics:#?}"
    );
}

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

#[test]
fn generic_optional_source_is_not_subtype_of_required_with_undefined() {
    let source = r#"
type WithOptional<T> = { prop?: T };
type WithRequired<T> = { prop: T | undefined };
type Check<T> = WithOptional<T> extends WithRequired<T> ? true : false;
type Result = Check<string>;
const r: false = (null as any as Result);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: generic optional property should not be subtype of required with undefined"
    );
}

#[test]
fn multiple_optional_properties_are_not_subtype_of_required_with_undefined() {
    let source = r#"
type Src = { x?: string; y?: number };
type Tgt = { x: string | undefined; y: number | undefined };
type R = Src extends Tgt ? true : false;
const r: false = (null as any as R);
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected no errors: multiple optional properties should not satisfy required-with-undefined targets"
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

/// Standard mode: no error for conditional required-key detection used by
/// `propTypeValidatorInference`-style helpers.
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

#[test]
fn infer_type_extracts_marker_from_inherited_validator_interface() {
    let source = r#"
declare const nominalTypeHack: unique symbol;
interface Validator<T> { [nominalTypeHack]?: T; }
interface Requireable<T> extends Validator<T> {
    isRequired: Validator<NonNullable<T>>;
}
type InferType<V> = V extends Validator<infer T> ? T : any;
type B = InferType<Requireable<boolean>>;
const b: boolean = null as any as B;
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected infer to extract T through Requireable<T> extends Validator<T>"
    );
}

#[test]
fn prop_types_shape_inference_matches_expected_props() {
    let source = r#"
declare const nominalTypeHack: unique symbol;
type IsOptional<T> = undefined | null extends T ? true : undefined extends T ? true : null extends T ? true : false;
type RequiredKeys<V> = { [K in keyof V]-?: Exclude<V[K], undefined> extends Validator<infer T> ? IsOptional<T> extends true ? never : K : never }[keyof V];
type OptionalKeys<V> = Exclude<keyof V, RequiredKeys<V>>;
type InferPropsInner<V> = { [K in keyof V]-?: InferType<V[K]>; };
interface Validator<T> { [nominalTypeHack]?: T; }
interface Requireable<T> extends Validator<T> {
    isRequired: Validator<NonNullable<T>>;
}
type ValidationMap<T> = { [K in keyof T]?: Validator<T[K]> };
type InferType<V> = V extends Validator<infer T> ? T : any;
type InferProps<V> = InferPropsInner<Pick<V, RequiredKeys<V>>> & Partial<InferPropsInner<Pick<V, OptionalKeys<V>>>>;
declare const any: Requireable<any>;
declare const bool: Requireable<boolean>;
declare const string: Requireable<string>;
declare function shape<P extends ValidationMap<any>>(type: P): Requireable<InferProps<P>>;
type Expected = { foo: string; bar?: boolean; baz?: any };
const innerProps = { foo: string.isRequired, bar: bool, baz: any };
const assign: Validator<Expected> = shape(innerProps).isRequired;
"#;
    assert!(
        check_source_codes(source).is_empty(),
        "expected prop-types shape inference to match the declared props shape"
    );
}
