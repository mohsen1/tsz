//! Regression tests for issue #3502: module-local declarations should only
//! shadow lib globals in the namespace they occupy.
//!
//! TypeScript resolves names through three namespaces (value, type, namespace).
//! A module-local `interface Symbol {}` only contributes to the TYPE namespace,
//! so the global VALUE binding `Symbol: SymbolConstructor` must remain visible.
//! Conversely, `const Array = 1` only contributes to the VALUE namespace, so
//! the global TYPE `Array<T>` must remain visible. Without this, tsz erroneously
//! emits TS2339 / TS2749 for code that tsc accepts.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn value_only_local_const_array_does_not_shadow_global_type_array() {
    // tsc: 0 errors. The local `const Array = 1` only takes over the VALUE
    // namespace; the global TYPE `Array<T>` is still visible for `xs: Array<number>`.
    let codes = diagnostic_codes(
        r#"
export {};
const Array = 1;
let xs: Array<number>;
"#,
    );
    assert!(
        !codes.contains(&2749),
        "TS2749 must not fire when local const Array shadows only the VALUE namespace; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2304),
        "TS2304 must not fire — global TYPE Array<T> is still visible; got: {codes:?}"
    );
}

#[test]
fn value_only_local_const_promise_does_not_shadow_global_type_promise() {
    // Same shape as Array, but with Promise from es2015.promise.
    let codes = diagnostic_codes(
        r#"
export {};
const Promise = 1;
let p: Promise<number>;
"#,
    );
    assert!(
        !codes.contains(&2749),
        "TS2749 must not fire for Promise<number> when local const Promise shadows only VALUE; got: {codes:?}"
    );
}

#[test]
fn type_only_local_interface_symbol_does_not_shadow_global_value_symbol() {
    // tsc: 0 errors. The local `interface Symbol {}` only takes over the TYPE
    // namespace; the global VALUE `Symbol: SymbolConstructor` remains visible
    // for `Symbol.iterator`.
    let codes = diagnostic_codes(
        r#"
export {};
interface Symbol {}
const x = Symbol.iterator;
"#,
    );
    assert!(
        !codes.contains(&2339),
        "TS2339 must not fire on Symbol.iterator when local interface Symbol shadows only TYPE; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2693),
        "TS2693 must not fire — global VALUE Symbol is still visible; got: {codes:?}"
    );
}

#[test]
fn local_value_array_is_still_addressable_as_value() {
    // Sanity: the local `const Array = 1` is a usable VALUE in expression position.
    let codes = diagnostic_codes(
        r#"
export {};
const Array = 1;
const k: number = Array;
"#,
    );
    let expected_clean: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|&c| c == 2322 || c == 2304 || c == 2749)
        .collect();
    assert!(
        expected_clean.is_empty(),
        "no name/typing errors expected for local const usage; got: {codes:?}"
    );
}

#[test]
fn local_type_symbol_is_still_addressable_as_type() {
    // Sanity: the local `interface Symbol {}` shadows the global TYPE Symbol,
    // so `let s: Symbol = ...` references the local empty interface.
    let codes = diagnostic_codes(
        r#"
export {};
interface Symbol {}
const s: Symbol = {} as Symbol;
"#,
    );
    let blocking: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|&c| c == 2304 || c == 2749 || c == 2693)
        .collect();
    assert!(
        blocking.is_empty(),
        "no name/typing errors expected for local interface usage as type; got: {codes:?}"
    );
}

/// Regression test for #4687: a module-local `unique symbol` const that
/// shadows a lib type alias must not entangle two unrelated user types
/// whose computed property keys reference the shadow symbol.
///
/// Before the fix in this PR, the lib's `type Readonly<T>` declaration was
/// copied onto the user's shadow symbol's `declarations` vec. Subsequent
/// type-alias evaluations whose computed property keys resolved the user's
/// `Readonly` value walked those polluted declarations and conflated
/// independent types `Input` and `Output`. The structural fix is to record
/// the lib origin via `lib_shadow_origin` instead of polluting the user
/// symbol's declarations vec; the checker falls back to the lib symbol's
/// declarations only when the user's own declarations don't supply the
/// required namespace.
#[test]
fn unique_symbol_shadow_does_not_conflate_independent_types() {
    // tsc emits TS2322 at the `return ors;` line because `Input[]` (with
    // only `foo`) is not assignable to `Output[]` (with `foo` and `bar`).
    // Pre-fix tsz failed to detect this assignability error: evaluation of
    // `type Input` wound up reusing `type Output`'s shape due to the
    // polluted shadow symbol.
    let codes = diagnostic_codes(
        r#"
export declare const Readonly: unique symbol;
export declare const Kind: unique symbol;

export interface TKind { [Kind]: string }
export interface TSchema extends TKind {
    [Readonly]?: string
    params: unknown[]
    static: unknown
}

export type Evaluate<T> = T extends infer O ? { [K in keyof O]: O[K] } : never

export type ReadonlyKeys<T extends TProperties> =
    { [K in keyof T]: T[K] extends TReadonly<TSchema> ? K : never }[keyof T]
export type RequiredKeys<T extends TProperties> =
    keyof Omit<T, ReadonlyKeys<T>>
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' }

export type PropertiesReducer<T extends TProperties, R extends Record<keyof any, unknown>> =
    Evaluate<(
        Readonly<Pick<R, ReadonlyKeys<T>>> &
        Required<Pick<R, RequiredKeys<T>>>
    )>
export type PropertiesReduce<T extends TProperties, P extends unknown[]> =
    PropertiesReducer<T, { [K in keyof T]: Static<T[K], P> }>
export type TPropertyKey = string | number
export type TProperties = Record<TPropertyKey, TSchema>
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object'
    static: PropertiesReduce<T, this['params']>
    type: 'object'
    properties: T
}
export interface TString extends TSchema {
    [Kind]: 'String'
    static: string
    type: 'string'
}
export type Static<T extends TSchema, P extends unknown[] = []> =
    (T & { params: P; })['static']

declare namespace Type {
    function Object<T extends TProperties>(object: T): TObject<T>
    function String(): TString
}

export type Input = Static<typeof Input>
export const Input = Type.Object({ foo: Type.String() })

export type Output = Static<typeof Output>
export const Output = Type.Object({ foo: Type.String(), bar: Type.String() })

function problematicFunction1(ors: Input[]): Output[] {
    return ors;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "TS2322 must fire when returning Input[] (smaller shape) where Output[] is expected; \
         pre-fix bug: shadow lib `type Readonly<T>` polluted user's `declarations`, \
         conflating Input and Output evaluations. Got: {codes:?}"
    );
}

/// Regression test for #4687: shadowing with `interface T {}` (TYPE-only)
/// must keep using the lib's INTERFACE methods reachable through
/// `lib_shadow_origin`, even with the declarations-vec pollution removed.
///
/// The "T" iteration variable name is intentionally chosen here to verify
/// the fix doesn't depend on a specific user-chosen name (anti-hardcoding
/// directive §25).
#[test]
fn type_only_local_interface_array_does_not_strip_lib_array_value() {
    let codes = diagnostic_codes(
        r#"
export {};
interface Array<T> { extra: T }
const xs = [1, 2, 3];
const len: number = xs.length;
"#,
    );
    // `xs.length` must resolve through the lib's `Array.length` value/property.
    // No TS2339 should fire on `length`.
    assert!(
        !codes.contains(&2339),
        "TS2339 must not fire on Array.length when local `interface Array<T>` shadows only TYPE; got: {codes:?}"
    );
}
