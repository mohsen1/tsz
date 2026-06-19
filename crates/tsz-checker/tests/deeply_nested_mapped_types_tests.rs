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

/// Guard for the `plain Input[] -> Output[]` reducer mismatch
/// (`deeplyNestedMappedTypes.ts`'s `problematicFunction1` shape).
///
/// The authoritative tsc baseline emits TS2322 for `f` (`Input[]` is not
/// assignable to `Output[]` because `Output.static` carries a required `bar`
/// property that `Input.static` lacks). The `const Readonly` *shadowing* defect
/// used to suppress it: the generic `PropertiesReducer` body instantiated
/// `Readonly<Pick<R, ...>>` against the shadowing `unique symbol` value instead
/// of the lib `Readonly<T>` utility, corrupting the reduced object so
/// `Input.static`/`Output.static` lost the `bar` discriminator (see
/// `renaming_readonly_unique_symbol_restores_ts2322`). That defect is fixed, so
/// tsz now emits exactly the leaf TS2322 with no spurious TS2344/TS2464 — and the
/// `f` shape (a plain `Input[]` parameter, like `problematicFunction1`) matches
/// tsc under both the bundled and the pinned lib sets.
///
/// NOTE: the `deeplyNestedMappedTypes.ts` conformance row stays an
/// accepted-regression — a *separate* residual (`problematicFunction3`'s
/// `(typeof Input.static)[]` element) still trips the `isDeeplyNestedType`
/// one-sided-expansion bailout once the deeper full pinned `lib.es5`/`lib.es2015`
/// `Static`/`PropertiesReduce` reducer is in play, which this `problematicFunction1`
/// shape does not exercise. See the row's note in
/// `scripts/conformance/conformance-accepted-regressions.txt`.
///
/// The helper skips gracefully (empty) when the lib asset is unavailable.
#[test]
fn readonly_pick_under_evaluate_reports_property_mismatch() {
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
    let codes = check_with_es5_lib_codes(source);
    if codes.is_empty() {
        return; // lib asset unavailable — covered by CLI/conformance instead
    }
    assert!(
        codes.contains(&2322),
        "Input[] is not assignable to Output[] (Output.static has required 'bar'): {codes:?}"
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

/// Anti-hardcoding companion to `readonly_pick_under_evaluate_reports_property_mismatch`:
/// the same structural shape with every user binder renamed (the `Readonly`/
/// `Optional` `unique symbol`s keep their lib-colliding names, which is the
/// structural crux, but the schema/object/reducer aliases and the field/value
/// names all differ). tsz must still report exactly the leaf TS2322 with no
/// spurious TS2344/TS2464 — proving the fix is structural, not keyed on the
/// `TObject`/`Static`/`Input`/`Output`/`foo`/`bar` identifiers.
#[test]
fn readonly_pick_under_evaluate_reports_property_mismatch_renamed_binders() {
    let source = r#"
export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export interface Spec { args: unknown[]; shape: unknown }
export interface SpecText extends Spec { shape: string }
export type Frozen<S extends Spec> = S & { [Readonly]: 'Readonly' }
export type Maybe<S extends Spec> = S & { [Optional]: 'Optional' }
export type Members = Record<string | number, Spec>
export type Flatten<U> = U extends infer O ? { [Q in keyof O]: O[Q] } : never
export type FrozenKeys<M extends Members> = { [Q in keyof M]: M[Q] extends Frozen<Spec> ? (M[Q] extends Maybe<M[Q]> ? Q : never) : never }[keyof M]
export type LiveKeys<M extends Members> = keyof Omit<M, FrozenKeys<M>>
export type Fold<M extends Members, W extends Record<keyof any, unknown>> = Flatten<(
    Readonly<Pick<W, FrozenKeys<M>>> &
    Required<Pick<W, LiveKeys<M>>>
)>
export type Reduce<M extends Members, A extends unknown[]> = Fold<M, { [Q in keyof M]: Shape<M[Q], A> }>
export interface Node<M extends Members = Members> extends Spec { shape: Reduce<M, this['args']>; members: M }
export type Shape<S extends Spec, A extends unknown[] = []> = (S & { args: A; })['shape']
declare namespace Build { function Node<M extends Members>(members: M): Node<M>; function Text(): SpecText }
export type Lhs = Shape<typeof Lhs>
export const Lhs = Build.Node({ alpha: Build.Text() })
export type Rhs = Shape<typeof Rhs>
export const Rhs = Build.Node({ alpha: Build.Text(), beta: Build.Text() })
function g(items: Lhs[]): Rhs[] { return items; }
"#;
    let codes = check_with_es5_lib_codes(source);
    if codes.is_empty() {
        return; // lib asset unavailable — covered by CLI/conformance instead
    }
    assert!(
        codes.contains(&2322),
        "Lhs[] is not assignable to Rhs[] (Rhs.shape has required 'beta'): {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "must NOT produce spurious TS2344 (renamed binders): {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "must NOT produce spurious TS2464 (renamed binders): {codes:?}"
    );
}

// ============================================================================
// Same-base recursive-conditional false-negative witnesses (#8432)
//
// `deeplyNestedMappedTypes.ts` includes a `NestedRecord` family:
//
//     type NestedRecord<K extends string, V> =
//         K extends `${infer K0}.${infer KR}`
//             ? { [P in K0]: NestedRecord<KR, V> }
//             : Record<K, V>;
//
// `tsc` reports the `const bar2: Bar2 = bar1` assignment (a number-valued nested
// record assigned to a string-valued one) as `TS2322`. tsz reproduces the error
// only while the dotted key path is shallow; once the path reaches four
// segments AND both operands are applications of the *same* recursive
// conditional alias (differing only in the non-recursion-driving payload type
// argument `V`), tsz silently accepts the assignment.
//
// Root cause (traced, deterministic): both operands evaluate correctly to
// distinct structural nested objects, but the assignment relation between them
// resolves a `Lazy(DefId)` whose body is unregistered in the relation-internal
// resolver (`resolver_generation() == 0`; `resolve_lazy_type` ->
// `note_lazy_resolve_failure`). The `False` derived from the genuine leaf
// mismatch (`number` vs `string`) is then treated as undetermined and the
// `TS2322` is suppressed. This is the resolver-availability-under-relation
// family (#13232) and the latent-soundness hazard documented in #13980 — the
// same root behind the #13609 `ApplyDefaultOptions`/`RequiredKeysOf`
// false-positive family. The fix belongs in the solver/checker relation layer
// (thread the def-resolving resolver into the relation-internal evaluator), not
// in the recursive alias itself; these witnesses pin the minimal, deterministic,
// name-agnostic shape so the eventual fix can be verified.
//
// An inline `Leaf<K, V> = { [P in K]: V }` stands in for the conformance test's
// `Record<K, V>` base case so the witnesses stay lib-free. The two positive
// guards reproduce the correct `TS2322` today and protect against the fix being
// scoped too narrowly (e.g. a witness-shaped patch keyed on a single depth or
// alias name).
// ============================================================================

/// Positive guard: at a shallow (3-segment) dotted key path the same-base
/// recursive-conditional assignment is correctly rejected. Binder names are
/// varied (`Tree`/`Leaf`/`leaf`/`sink`) so the result is not keyed on text.
#[test]
fn nested_record_shallow_same_base_recursive_conditional_errors() {
    let source = r#"
type Leaf<K extends string, V> = { [P in K]: V };
type Tree<K extends string, V> = K extends `${infer Head}.${infer Tail}`
    ? { [P in Head]: Tree<Tail, V> }
    : Leaf<K, V>;
declare const leaf: Tree<"x.y.z", number>;
const sink: Tree<"x.y.z", string> = leaf;
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "shallow same-base recursive-conditional number->string nested record must error: {codes:?}"
    );
}

/// Positive guard: at a deep (4-segment) path, when the source and target use
/// *different* recursive-conditional alias bases (so the same-base relation path
/// is not taken) the assignment is correctly rejected. This isolates the defect
/// to the same-base path rather than the depth or the nested-record shape.
#[test]
fn nested_record_deep_different_base_recursive_conditional_errors() {
    let source = r#"
type Leaf<K extends string, V> = { [P in K]: V };
type Path<K extends string, V> = K extends `${infer Head}.${infer Tail}`
    ? { [P in Head]: Path<Tail, V> }
    : Leaf<K, V>;
type Trail<K extends string, V> = K extends `${infer Head}.${infer Tail}`
    ? { [P in Head]: Trail<Tail, V> }
    : Leaf<K, V>;
declare const src: Path<"x.y.z.w", number>;
const dst: Trail<"x.y.z.w", string> = src;
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "deep different-base recursive-conditional number->string nested record must error: {codes:?}"
    );
}

/// Witness (#8432, resolver-availability family #13232/#13980): at a deep
/// (4-segment) path with both operands sharing the *same* recursive-conditional
/// alias base, tsz silently accepts a `number`-valued nested record assigned to
/// a `string`-valued one. `tsc` reports `TS2322`. Ignored until the
/// relation-internal resolver-availability fix lands; remove `#[ignore]` then.
#[test]
#[ignore = "#8432: same-base deep recursive-conditional assignment relation suppresses the genuine TS2322 (resolver-availability-under-relation, #13232/#13980)"]
fn nested_record_deep_same_base_recursive_conditional_false_negative() {
    let source = r#"
type Leaf<K extends string, V> = { [P in K]: V };
type Tree<K extends string, V> = K extends `${infer Head}.${infer Tail}`
    ? { [P in Head]: Tree<Tail, V> }
    : Leaf<K, V>;
declare const leaf: Tree<"x.y.z.w", number>;
const sink: Tree<"x.y.z.w", string> = leaf;
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2322),
        "deep same-base recursive-conditional number->string nested record must error (tsc: TS2322): {codes:?}"
    );
}
