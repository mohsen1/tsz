//! Regression tests for mapped+infer evaluation when the inferred type is
//! subsequently substituted into another generic application.
//!
//! Pattern (from `compiler/conformance/jsx/tsxLibraryManagedAttributes.tsx`):
//!
//! ```ts
//! interface PropTypeChecker<U, TRequired = false> {
//!     [checkedType]: TRequired extends true ? U : U | null | undefined;
//! }
//! type InferredPropTypes<P> = {
//!     [K in keyof P]: P[K] extends PropTypeChecker<infer T, infer U>
//!         ? PropTypeChecker<T, U>[typeof checkedType]
//!         : {}
//! };
//! ```
//!
//! For `P = { bar: PropTypeChecker<X, true> }`, tsc evaluates the per-key
//! conditional to the `true` branch, producing `PropTypeChecker<X, true>[checkedType]`
//! and finally `X`. tsz currently falls through to `{}`, indicating the
//! mapped per-key infer pattern fails to match the substituted application.

use crate::test_utils::{check_source_diagnostics, check_source_strict};

fn first_2322(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322 = diags.iter().find(|d| d.code == 2322).unwrap_or_else(|| {
        panic!(
            "Expected TS2322, got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        )
    });
    ts2322.message_text.clone()
}

/// Direct alias `type N = (typeof node)[typeof checkedType]` — no mapped
/// type, just the conditional substitution. tsc preserves `ReactNode`.
#[test]
fn mapped_infer_substituted_alias_preserved_via_indexed_conditional() {
    let msg = first_2322(
        r#"
type ReactNode = string | number | object;
declare const checkedType: unique symbol;
interface PropChecker<U, R = false> {
    [checkedType]: R extends true ? U : U | null | undefined;
}
declare const node: PropChecker<ReactNode, true>;
type N = (typeof node)[typeof checkedType];
declare let x: N;
x = null;
"#,
    );
    assert!(
        msg.contains("'ReactNode'") || msg.contains("'N'"),
        "Direct conditional substitution should preserve ReactNode (or wrapper N). Got: {msg}"
    );
}

/// The full mapped+infer pattern from the failing tsxLibraryManagedAttributes
/// test. tsz currently falls through to `{}`; tsc evaluates correctly.
#[test]
fn mapped_per_key_infer_with_substitution_resolves_true_branch() {
    let msg = first_2322(
        r#"
type ReactNode = string | number | object;
declare const checkedType: unique symbol;
interface PropTypeChecker<U, TRequired = false> {
    [checkedType]: TRequired extends true ? U : U | null | undefined;
}
type InferredPropTypes<P> = {
    [K in keyof P]: P[K] extends PropTypeChecker<infer T, infer U>
        ? PropTypeChecker<T, U>[typeof checkedType]
        : {}
};

declare const propTypes: { bar: PropTypeChecker<ReactNode, true> };
type Props = InferredPropTypes<typeof propTypes>;
declare let bar: Props["bar"];
bar = null;
"#,
    );
    assert!(
        msg.contains("'ReactNode'"),
        "Mapped per-key infer should resolve to 'ReactNode' (true-branch via TRequired=true). Got: {msg}"
    );
    assert!(
        !msg.contains("type '{}'"),
        "Mapped per-key infer must NOT fall through to '{{}}' branch. Got: {msg}"
    );
}

/// Anti-hardcoding cover: same pattern with renamed identifiers.
/// If the fix relies on a hardcoded user-chosen name (`P`, `T`, `U`,
/// `K`, `TRequired`), this test breaks.
#[test]
fn mapped_per_key_infer_with_substitution_resolves_true_branch_renamed() {
    let msg = first_2322(
        r#"
type Renderable = string | number | object;
declare const tag: unique symbol;
interface Checker<V, R = false> {
    [tag]: R extends true ? V : V | null | undefined;
}
type Inferred<S> = {
    [Q in keyof S]: S[Q] extends Checker<infer X, infer Y>
        ? Checker<X, Y>[typeof tag]
        : never
};

declare const checks: { item: Checker<Renderable, true> };
type Result = Inferred<typeof checks>;
declare let item: Result["item"];
item = null;
"#,
    );
    assert!(
        msg.contains("'Renderable'"),
        "Renamed variant: must resolve to 'Renderable'. Got: {msg}"
    );
    assert!(
        !msg.contains("type 'never'"),
        "Renamed variant: must NOT fall through to 'never' branch. Got: {msg}"
    );
}

#[test]
fn mapped_conditional_infer_evaluates_inline_exclude_index_access() {
    let diags = check_source_strict(
        r#"
interface Validator<T> {
    p?: T;
}
type Exclude<T, U> = T extends U ? never : T;
type Keys<V> = {
    [K in keyof V]-?: Exclude<V[K], undefined> extends Validator<infer T>
        ? K
        : never
}[keyof V];

type Obj = {
    array?: Validator<string[]>;
    bool?: Validator<boolean>;
};
type RK = Keys<Obj>;

const rk1: RK = "array";
const rk2: RK = "bool";
const rkBad: RK = "nope";
"#,
    );
    let ts2322 = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "Only the invalid key should emit TS2322; got: {diags:?}"
    );
    assert!(
        ts2322[0].contains("\"nope\""),
        "The remaining TS2322 should be for the invalid key, got: {ts2322:?}"
    );
}

#[test]
fn concrete_mapped_conditional_infer_evaluates_inline_exclude_index_access() {
    let diags = check_source_diagnostics(
        r#"
interface Validator<T> {
    p?: T;
}
type Exclude<T, U> = T extends U ? never : T;
type Obj = {
    array?: Validator<string[]>;
    bool?: Validator<boolean>;
};
type RK = {
    [K in keyof Obj]-?: Exclude<Obj[K], undefined> extends Validator<infer T>
        ? K
        : never
}[keyof Obj];

const rk1: RK = "array";
const rk2: RK = "bool";
const rkBad: RK = "nope";
"#,
    );
    let ts2322 = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "Only the invalid key should emit TS2322; got: {diags:?}"
    );
    assert!(
        ts2322[0].contains("\"nope\""),
        "The remaining TS2322 should be for the invalid key, got: {ts2322:?}"
    );
}

#[test]
fn direct_conditional_infer_evaluates_inline_exclude_index_access() {
    let diags = check_source_diagnostics(
        r#"
interface Validator<T> {
    p?: T;
}
type Exclude<T, U> = T extends U ? never : T;
type Obj = {
    array?: Validator<string[]>;
};
type R = Exclude<Obj["array"], undefined> extends Validator<infer T>
    ? "array"
    : never;

const rk1: R = "array";
const rkBad: R = "nope";
"#,
    );
    let ts2322 = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "Only the invalid key should emit TS2322; got: {diags:?}"
    );
    assert!(
        ts2322[0].contains("\"nope\""),
        "The remaining TS2322 should be for the invalid key, got: {ts2322:?}"
    );
}

#[test]
fn direct_conditional_infer_does_not_cache_early_unresolved_false_branch() {
    let diags = check_source_diagnostics(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type NonNullable<T> = T & {};
declare const nominalTypeHack: unique symbol;
interface Validator<T> {
    (props: object, propName: string, componentName: string, location: string, propFullName: string): object | null;
    [nominalTypeHack]?: T;
}
interface Requireable<T> extends Validator<T> {
    isRequired: Validator<NonNullable<T>>;
}
declare const array: Requireable<any[]>;

const source = array.isRequired;
type TArray = Exclude<typeof source, undefined> extends Validator<infer T>
    ? T
    : never;

const ok: TArray = [] as any[];
const bad: TArray = "nope";
"#,
    );
    let ts2322 = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "Only the invalid string assignment should emit TS2322; got: {diags:?}"
    );
    assert!(
        ts2322[0].contains("Type 'string'"),
        "The remaining TS2322 should reject the invalid string, got: {ts2322:?}"
    );
}

#[test]
fn mapped_required_keys_evaluates_nested_optional_filter() {
    let diags = check_source_strict(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type IsOptional<T> = undefined | null extends T
    ? true
    : undefined extends T
        ? true
        : null extends T
            ? true
            : false;
declare const nominalTypeHack: unique symbol;
interface Validator<T> {
    (props: object, propName: string, componentName: string, location: string, propFullName: string): object | null;
    [nominalTypeHack]?: T;
}
type RequiredKeys<V> = {
    [K in keyof V]-?: Exclude<V[K], undefined> extends Validator<infer T>
        ? IsOptional<T> extends true
            ? never
            : K
        : never
}[keyof V];

type Obj = {
    optional?: Validator<string | undefined>;
    required?: Validator<boolean>;
};
type RK = RequiredKeys<Obj>;

const rk1: RK = "required";
const rkBad1: RK = "optional";
const rkBad2: RK = "nope";
"#,
    );
    let ts2322 = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        2,
        "Only the optional and invalid keys should emit TS2322; got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|msg| msg.contains("\"optional\""))
            && ts2322.iter().any(|msg| msg.contains("\"nope\"")),
        "The remaining TS2322s should reject optional/nope, got: {ts2322:?}"
    );
}

#[test]
fn prop_types_inferprops_annotation_assignable_to_unannotated_inferprops() {
    let diags = check_source_strict(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type NonNullable<T> = T & {};
declare const nominalTypeHack: unique symbol;
type IsOptional<T> = undefined | null extends T
    ? true
    : undefined extends T
        ? true
        : null extends T
            ? true
            : false;
type RequiredKeys<V> = {
    [K in keyof V]-?: Exclude<V[K], undefined> extends Validator<infer T>
        ? IsOptional<T> extends true
            ? never
            : K
        : never
}[keyof V];
type OptionalKeys<V> = Exclude<keyof V, RequiredKeys<V>>;
type InferPropsInner<V> = {
    [K in keyof V]-?: InferType<V[K]>;
};
interface Validator<T> {
    [nominalTypeHack]?: T;
}
interface Requireable<T> extends Validator<T> {
    isRequired: Validator<NonNullable<T>>;
}
type ValidationMap<T> = {
    [K in keyof T]?: Validator<T[K]>;
};
type InferType<V> = V extends Validator<infer T> ? T : any;
type InferProps<V> =
    InferPropsInner<Pick<V, RequiredKeys<V>>>
        & Partial<InferPropsInner<Pick<V, OptionalKeys<V>>>>;

declare const any: Requireable<any>;
declare const array: Requireable<any[]>;
declare const bool: Requireable<boolean>;
declare const string: Requireable<string>;
declare const number: Requireable<number>;
declare function shape<P extends ValidationMap<any>>(type: P): Requireable<InferProps<P>>;
declare function oneOfType<T extends Validator<any>>(types: T[]): Requireable<NonNullable<InferType<T>>>;

interface Props {
    any?: any;
    array: string[];
    bool: boolean;
    shape: { foo: string; bar?: boolean; baz?: any };
    oneOfType: string | boolean | { foo?: string; bar: number };
}
type PropTypesMap = ValidationMap<Props>;
const innerProps = { foo: string.isRequired, bar: bool, baz: any };
const arrayOfTypes = [string, bool, shape({ foo: string, bar: number.isRequired })];
const propTypes = {
    any,
    array: array.isRequired,
    bool: bool.isRequired,
    shape: shape(innerProps).isRequired,
    oneOfType: oneOfType(arrayOfTypes).isRequired,
} as PropTypesMap;
const propTypesWithoutAnnotation = {
    any,
    array: array.isRequired,
    bool: bool.isRequired,
    shape: shape(innerProps).isRequired,
    oneOfType: oneOfType(arrayOfTypes).isRequired,
};
type ExtractedProps = InferProps<typeof propTypes>;
type ExtractedPropsWithoutAnnotation = InferProps<typeof propTypesWithoutAnnotation>;

declare const annotated: ExtractedProps;
const unannotated: ExtractedPropsWithoutAnnotation = annotated;
"#,
    );
    assert!(
        diags.is_empty(),
        "Annotated InferProps should assign to unannotated InferProps; got: {diags:?}"
    );
}
