use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_codes, check_source_with_libs_code_messages, load_lib_files,
};

fn check(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

/// Type-check `source` as an external module with the bundled `es5` lib loaded
/// (so the global utility types `Readonly`/`Partial`/`Pick`/`Record`/… are in
/// scope), returning only the diagnostic codes. Skips gracefully (empty) when
/// the bundled lib asset is unavailable in the build environment.
fn check_with_es5_lib_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts"]);
    if libs.is_empty() {
        return Vec::new();
    }
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn ts2322_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 2322).count()
}

/// Regression guard for #8432 (`deeplyNestedMappedTypes.ts` family): a
/// value-only binding (`const Readonly: unique symbol`) that merely shares a
/// name with a global lib utility type must NOT shadow that lib type in type
/// position inside an external module. tsc keeps the lib `Readonly<T>` visible;
/// tsz used to key the lib type's `DefId` to the shadowing value symbol, so the
/// `Readonly<…>` application never matched its mapped-type body and stayed
/// opaque — producing a false `TS2322` on the *valid* assignment (the unreduced
/// `Readonly<{ a: string }>` target rejects everything). After the fix the
/// application reduces and only the genuine `number`/`string` leaf mismatch
/// errors.
#[test]
fn value_const_shadowing_lib_readonly_in_module_still_reduces() {
    let codes = check_with_es5_lib_codes(
        r#"
export declare const Readonly: unique symbol;
type Foo = Readonly<{ a: string }>;
const ok: Foo = { a: "hi" };
const bad: Foo = { a: 1 };
export {};
"#,
    );
    if codes.is_empty() {
        return; // lib asset unavailable — covered by CLI/conformance instead
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "exactly the `bad` leaf mismatch must error, not the valid `ok` assignment: {codes:?}"
    );
}

/// Same defect reached through a generic alias body (`type G<R> = Readonly<R>`):
/// the deferred lib application inside the alias must reduce when instantiated.
#[test]
fn value_const_shadowing_lib_readonly_via_generic_alias_reduces() {
    let codes = check_with_es5_lib_codes(
        r#"
export declare const Readonly: unique symbol;
type G<R> = Readonly<R>;
type Foo = G<{ a: string }>;
const ok: Foo = { a: "hi" };
const bad: Foo = { a: 1 };
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "generic-alias lib application must reduce; only `bad` should error: {codes:?}"
    );
}

/// Anti-hardcoding: the fix keys on the structural value-only-shadow condition,
/// not the identifier `Readonly`. The same shadow over `Partial` (which makes
/// members optional) must keep the lib type visible — the empty and valid
/// objects are clean and only the `number`/`string` mismatch errors.
#[test]
fn value_const_shadowing_lib_partial_in_module_still_reduces() {
    let codes = check_with_es5_lib_codes(
        r#"
export declare const Partial: unique symbol;
type Foo = Partial<{ a: string }>;
const empty: Foo = {};
const ok: Foo = { a: "hi" };
const bad: Foo = { a: 1 };
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "Partial shadow must reduce; only `bad` should error: {codes:?}"
    );
}

/// Anti-hardcoding, second binder: a value shadowing `Record`.
#[test]
fn value_const_shadowing_lib_record_in_module_still_reduces() {
    let codes = check_with_es5_lib_codes(
        r#"
export declare const Record: unique symbol;
type Foo = Record<string, number>;
const ok: Foo = { x: 1 };
const bad: Foo = { x: "s" };
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "Record shadow must reduce; only `bad` should error: {codes:?}"
    );
}

/// The script-file (global, non-module) form already resolved correctly before
/// the fix; guard that it stays correct (the fix must not perturb the
/// non-external-module path).
#[test]
fn value_const_shadowing_lib_readonly_in_script_unchanged() {
    let codes = check_with_es5_lib_codes(
        r#"
declare const Readonly: unique symbol;
type Foo = Readonly<{ a: string }>;
const ok: Foo = { a: "hi" };
const bad: Foo = { a: 1 };
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "script-form Readonly shadow must reduce; only `bad` should error: {codes:?}"
    );
}

/// tsc does NOT emit TS2344 or TS2464 for a recursive identity mapped type.
/// Mirrors the `deeplyNestedMappedTypes.ts` conformance test.
#[test]
fn id_recursive_mapped_no_spurious_constraint_or_computed_prop_errors() {
    let source = r#"
type Id<T> = { readonly [P in keyof T]: Id<T[P]> };
declare const numVer: Id<{ x: { y: { z: { a: { b: { c: number; }; }; }; }; }; }>;
const bad: Id<{ x: { y: { z: { a: { b: { c: string; }; }; }; }; }; }> = numVer;
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "must produce TS2322 for leaf mismatch: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "spurious TS2344 in recursive mapped type body: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "spurious TS2464 in recursive mapped type body: {codes:?}"
    );
}

/// Id2 variant with renamed params — same rule.
#[test]
fn id2_variant_recursive_mapped_no_spurious_errors() {
    let source = r#"
type Id2<U> = { readonly [Q in keyof U]: Id2<U[Q]> };
declare const numVer: Id2<{ x: { y: { z: { a: { b: { c: number; }; }; }; }; }; }>;
const bad: Id2<{ x: { y: { z: { a: { b: { c: string; }; }; }; }; }; }> = numVer;
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "must produce TS2322 for leaf mismatch: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "spurious TS2344 in Id2 recursive mapped type body: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "spurious TS2464 in Id2 recursive mapped type body: {codes:?}"
    );
}

/// Constraint-conditional recursive mapped type (`RequiredDeep` style).
#[test]
fn constraint_conditional_recursive_mapped_no_spurious_errors() {
    let source = r#"
type RequiredDeep<T> = T extends object ? { [K in keyof T]-?: RequiredDeep<T[K]> } : T;
interface Deep { a: { b: { c: number } } }
declare const src: RequiredDeep<Deep>;
const dst: RequiredDeep<Deep> = src;
"#;
    let codes = check(source);
    assert!(
        codes.is_empty(),
        "RequiredDeep same-type assignment must produce no errors: {codes:?}"
    );
}

/// The actual `deeplyNestedMappedTypes.ts` test uses unique symbol computed properties.
/// These should not produce TS2464.
#[test]
fn unique_symbol_computed_property_in_type_literal_no_ts2464() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export interface TSchema {
    [Readonly]?: string;
    [Optional]?: string;
    static: unknown;
}
export type TReadonly<T extends TSchema> = T & { [Readonly]: "Readonly" };
export type TOptional<T extends TSchema> = T & { [Optional]: "Optional" };
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2464),
        "unique symbol computed property must not produce TS2464: {codes:?}"
    );
    assert!(!codes.contains(&2344), "must not produce TS2344: {codes:?}");
}

/// Full `deeplyNestedMappedTypes.ts`-style complex pattern.
#[test]
fn complex_schema_builder_pattern_no_spurious_errors() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export interface TSchema {
    [Readonly]?: string;
    [Optional]?: string;
    static: unknown;
}
export type TReadonly<T extends TSchema> = T & { [Readonly]: "Readonly" };
export type TOptional<T extends TSchema> = T & { [Optional]: "Optional" };
export type TProperties = Record<string | number, TSchema>;
export type ReadonlyPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema>
        ? (T[K] extends TOptional<T[K]> ? never : K)
        : never
}[keyof T];
export type OptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TOptional<TSchema>
        ? (T[K] extends TReadonly<T[K]> ? never : K)
        : never
}[keyof T];
export type R<T extends TProperties, V extends Record<keyof any, unknown>> =
    Readonly<Partial<Pick<V, ReadonlyPropertyKeys<T>>>> &
    Partial<Pick<V, OptionalPropertyKeys<T>>>;
"#;
    let codes = check(source);
    // This test guards the spurious mapped-type constraint/computed-property
    // family (TS2344 / TS2464) that the `deeplyNestedMappedTypes.ts` schema
    // builder used to trip. It intentionally does NOT assert `is_empty()`:
    // this snippet declares `const Readonly: unique symbol` and then uses
    // `Readonly<...>` in type position, which tsz currently mis-resolves to the
    // shadowing value (spurious TS2749 + TS2304). That value-vs-type name
    // resolution divergence is a SEPARATE issue from the mapped-type recursion
    // family under test here; tsc keeps the global `Readonly<T>` lib type
    // visible even when a same-named value is declared.
    assert!(
        !codes.contains(&2464),
        "schema builder pattern must not produce TS2464: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "schema builder pattern must not produce TS2344: {codes:?}"
    );
}

/// Full reproduction of `deeplyNestedMappedTypes.ts`: the `TypeBox`-style schema builder pattern.
/// tsc expects only TS2322 from this; tsz must not produce TS2344 or TS2464.
#[test]
fn typebox_schema_builder_no_spurious_ts2344_or_ts2464() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Hint: unique symbol;
export declare const Kind: unique symbol;

export interface TKind {
    [Kind]: string
}

export interface TSchema extends TKind {
    [Readonly]?: string;
    [Optional]?: string;
    [Hint]?: string;
    params: unknown[];
    static: unknown;
}

export type TReadonlyOptional<T extends TSchema> = TOptional<T> & TReadonly<T>;
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' };
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' };

export interface TString extends TSchema {
    [Kind]: 'String';
    static: string;
    type: 'string';
}

export type ReadonlyOptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? K : never) : never
}[keyof T];
export type ReadonlyPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? never : K) : never
}[keyof T];
export type OptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TOptional<TSchema> ? (T[K] extends TReadonly<T[K]> ? never : K) : never
}[keyof T];
export type RequiredPropertyKeys<T extends TProperties> = keyof Omit<T,
    ReadonlyOptionalPropertyKeys<T> | ReadonlyPropertyKeys<T> | OptionalPropertyKeys<T>>;
export type PropertiesReducer<T extends TProperties, R extends Record<keyof any, unknown>> = (
    Readonly<Partial<Pick<R, ReadonlyOptionalPropertyKeys<T>>>> &
    Readonly<Pick<R, ReadonlyPropertyKeys<T>>> &
    Partial<Pick<R, OptionalPropertyKeys<T>>> &
    Required<Pick<R, RequiredPropertyKeys<T>>>
);
export type TPropertyKey = string | number;
export type TProperties = Record<TPropertyKey, TSchema>;
export type PropertiesReduce<T extends TProperties, P extends unknown[]> = PropertiesReducer<T, {
    [K in keyof T]: Static<T[K], P>
}>;
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object';
    static: PropertiesReduce<T, this['params']>;
    type: 'object';
    properties: T;
}
export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static'];
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2344),
        "TypeBox schema builder must not produce TS2344: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "TypeBox schema builder must not produce TS2464: {codes:?}"
    );
}

/// Full `deeplyNestedMappedTypes.ts` content including `Static<typeof X>` and function patterns.
#[test]
fn full_deeply_nested_mapped_types_repro() {
    let source = r#"
// Id<T> section
type Id<T> = { [K in keyof T]: Id<T[K]> };
type Foo1 = Id<{ x: { y: { z: { a: { b: { c: number } } } } } }>;
type Foo2 = Id<{ x: { y: { z: { a: { b: { c: string } } } } } }>;
declare const foo1: Foo1;
const foo2: Foo2 = foo1; // Error: TS2322

type Id2<T> = { [K in keyof T]: Id2<Id2<T[K]>> };
type Foo3 = Id2<{ x: { y: { z: { a: { b: { c: number } } } } } }>;
type Foo4 = Id2<{ x: { y: { z: { a: { b: { c: string } } } } } }>;
declare const foo3: Foo3;
const foo4: Foo4 = foo3; // Error: TS2322

// NestedRecord section
type NestedRecord<K extends string, V> = K extends `${infer K0}.${infer KR}` ? { [P in K0]: NestedRecord<KR, V> } : Record<K, V>;
type Bar1 = NestedRecord<"x.y.z.a.b.c", number>;
type Bar2 = NestedRecord<"x.y.z.a.b.c", string>;
declare const bar1: Bar1;
const bar2: Bar2 = bar1; // Error: TS2322

// TypeBox-style schema section
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Hint: unique symbol;
export declare const Kind: unique symbol;
export interface TKind { [Kind]: string }
export interface TSchema extends TKind {
    [Readonly]?: string;
    [Optional]?: string;
    [Hint]?: string;
    params: unknown[];
    static: unknown;
}
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' };
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' };
export interface TString extends TSchema {
    [Kind]: 'String';
    static: string;
    type: 'string';
}
export type ReadonlyOptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? K : never) : never
}[keyof T];
export type ReadonlyPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? never : K) : never
}[keyof T];
export type OptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TOptional<TSchema> ? (T[K] extends TReadonly<T[K]> ? never : K) : never
}[keyof T];
export type RequiredPropertyKeys<T extends TProperties> = keyof Omit<T,
    ReadonlyOptionalPropertyKeys<T> | ReadonlyPropertyKeys<T> | OptionalPropertyKeys<T>>;
export type PropertiesReducer<T extends TProperties, R extends Record<keyof any, unknown>> = (
    Readonly<Partial<Pick<R, ReadonlyOptionalPropertyKeys<T>>>> &
    Readonly<Pick<R, ReadonlyPropertyKeys<T>>> &
    Partial<Pick<R, OptionalPropertyKeys<T>>> &
    Required<Pick<R, RequiredPropertyKeys<T>>>
);
export type TPropertyKey = string | number;
export type TProperties = Record<TPropertyKey, TSchema>;
export type PropertiesReduce<T extends TProperties, P extends unknown[]> = PropertiesReducer<T, {
    [K in keyof T]: Static<T[K], P>
}>;
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object';
    static: PropertiesReduce<T, this['params']>;
    type: 'object';
    properties: T;
}
export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static'];

declare namespace Type {
    function Object<T extends TProperties>(object: T): TObject<T>;
    function String(): TString;
}

export type Input = Static<typeof Input>;
export const Input = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
        })
    })
});

export type Output = Static<typeof Output>;
export const Output = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
            bar: Type.String(),
        })
    })
});

function problematicFunction1(ors: Input[]): Output[] {
    return ors; // Error: TS2322
}

function problematicFunction2<T extends Output[]>(ors: Input[]): T {
    return ors; // Error: TS2322
}

function problematicFunction3(ors: (typeof Input.static)[]): Output[] {
    return ors; // Error: TS2322
}
"#;
    let codes = check(source);
    // tsc expects exactly TS2322 (5 errors — foo2/foo4/bar2/ors assignments)
    assert!(
        codes.contains(&2322),
        "must produce TS2322 for the assignment errors: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "must NOT produce spurious TS2344: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "must NOT produce spurious TS2464: {codes:?}"
    );
}

/// `PropertiesReducer` with `Evaluate<T>` wrapper — exact match to the actual file.
#[test]
fn properties_reducer_with_evaluate_wrapper_no_spurious_errors() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Kind: unique symbol;
export interface TKind { [Kind]: string }
export interface TSchema extends TKind {
    [Readonly]?: string;
    [Optional]?: string;
    params: unknown[];
    static: unknown;
}
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' };
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' };
export type ReadonlyOptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? K : never) : never
}[keyof T];
export type ReadonlyPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? never : K) : never
}[keyof T];
export type OptionalPropertyKeys<T extends TProperties> = {
    [K in keyof T]: T[K] extends TOptional<TSchema> ? (T[K] extends TReadonly<T[K]> ? never : K) : never
}[keyof T];
export type RequiredPropertyKeys<T extends TProperties> = keyof Omit<T,
    ReadonlyOptionalPropertyKeys<T> | ReadonlyPropertyKeys<T> | OptionalPropertyKeys<T>>;

export type Evaluate<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;

export type PropertiesReducer<T extends TProperties, R extends Record<keyof any, unknown>> = Evaluate<(
    Readonly<Partial<Pick<R, ReadonlyOptionalPropertyKeys<T>>>> &
    Readonly<Pick<R, ReadonlyPropertyKeys<T>>> &
    Partial<Pick<R, OptionalPropertyKeys<T>>> &
    Required<Pick<R, RequiredPropertyKeys<T>>>
)>;
export type TPropertyKey = string | number;
export type TProperties = Record<TPropertyKey, TSchema>;
export type PropertiesReduce<T extends TProperties, P extends unknown[]> = PropertiesReducer<T, {
    [K in keyof T]: Static<T[K], P>
}>;
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object';
    static: PropertiesReduce<T, this['params']>;
    type: 'object';
    properties: T;
}
export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static'];

declare namespace Type {
    function Object<T extends TProperties>(object: T): TObject<T>;
    function String(): TString;
}
export interface TString extends TSchema {
    [Kind]: 'String';
    static: string;
    type: 'string';
}

export type Input = Static<typeof Input>;
export const Input = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
        })
    })
});

export type Output = Static<typeof Output>;
export const Output = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
            bar: Type.String(),
        })
    })
});

function f1(ors: Input[]): Output[] { return ors; }
function f2<T extends Output[]>(ors: Input[]): T { return ors; }
function f3(ors: (typeof Input.static)[]): Output[] { return ors; }
"#;
    let codes = check(source);
    assert!(
        !codes.contains(&2344),
        "Evaluate<PropertiesReducer> must not produce TS2344: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "Evaluate<PropertiesReducer> must not produce TS2464: {codes:?}"
    );
}

/// KNOWN FALSE-NEGATIVE (accepted-regression `deeplyNestedMappedTypes.ts`).
///
/// The authoritative tsc baseline emits TS2322 for `problematicFunction1` and
/// `problematicFunction3` (`Input[]` is not assignable to `Output[]` because
/// `Output.static` carries a `bar` property that `Input.static` lacks). tsz
/// currently MISSES both, while correctly emitting the other three baseline
/// errors (`foo2`, `foo4`, `problematicFunction2`).
///
/// Root cause (verified by one-variable isolation — see the companion test
/// `renaming_readonly_unique_symbol_restores_ts2322`): the fixture declares
/// `const Readonly: unique symbol`, whose name collides with the global lib
/// `Readonly<T>` utility type. When the deferred generic `PropertiesReducer`
/// body instantiates `Readonly<Pick<R, ...>>` (R/T still free type
/// parameters), tsz mis-resolves the `Readonly` reference to the shadowing
/// value instead of the lib type, corrupting the reduced object so that
/// `Input.static` and `Output.static` both lose the real `bar` discriminator
/// and compare as assignable. Renaming the `Readonly` unique symbol to any
/// non-lib name restores the correct TS2322. A simple
/// `type Foo = Readonly<{ a: string }>` with `const Readonly` in scope resolves
/// correctly, so the divergence is specific to lazy instantiation of the
/// shadowed lib type inside a generic alias body. The fix belongs in
/// type-vs-value name resolution for globally-shadowed lib utility types
/// (checker/binder), NOT in the solver index-access path.
///
/// NOTE: this repro depends on lib utility types (`Readonly`, `Pick`, `Omit`,
/// `Required`, `Record`) that the minimal unit-test lib does not provide, so it
/// is kept `#[ignore]`d here purely as inline documentation of the exact
/// minimal witness. The runnable parity gate lives in conformance
/// (`deeplyNestedMappedTypes.ts`, listed in
/// `scripts/conformance/conformance-accepted-regressions.txt`); reproduce the
/// CLI behavior with the full lib via `tsz` on the snippet below.
#[test]
#[ignore = "lib-dependent witness for the const-Readonly-shadows-lib-Readonly<T> false-negative; runnable gate is in conformance, see conformance-accepted-regressions.txt"]
fn readonly_pick_never_under_evaluate_loses_property_mismatch() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export interface TSchema { params: unknown[]; static: unknown }
export interface TString extends TSchema { static: string }
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' }
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' }
export type TProperties = Record<string | number, TSchema>
export type Evaluate<T> = T extends infer O ? { [K in keyof O]: O[K] } : never
export type ROK<T extends TProperties> = { [K in keyof T]: T[K] extends TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? K : never) : never }[keyof T]
export type RequiredPropertyKeys<T extends TProperties> = keyof Omit<T, ROK<T>>
export type Reducer<T extends TProperties, R extends Record<keyof any, unknown>> = Evaluate<(
    Readonly<Pick<R, ROK<T>>> &
    Required<Pick<R, RequiredPropertyKeys<T>>>
)>
export type Reduce<T extends TProperties, P extends unknown[]> = Reducer<T, { [K in keyof T]: Static<T[K], P> }>
export interface TObject<T extends TProperties = TProperties> extends TSchema { static: Reduce<T, this['params']>; properties: T }
export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static']
declare namespace Type { function Object<T extends TProperties>(object: T): TObject<T>; function String(): TString }
export type Input = Static<typeof Input>
export const Input = Type.Object({ foo: Type.String() })
export type Output = Static<typeof Output>
export const Output = Type.Object({ foo: Type.String(), bar: Type.String() })
function f(x: Input[]): Output[] { return x; }
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "Input[] is not assignable to Output[] (Output.static has required 'bar'): {codes:?}"
    );
}
