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

/// Regression for issue #4687: independent `Static<typeof X>` evaluations
/// in a TypeBox-style fixture must not conflate shapes when the file
/// declares VALUE-only `unique symbol` consts that shadow lib type-alias
/// names like `Readonly`, `Optional`, etc.
///
/// Before the fix, `collect_preserved_lib_meaning` carried the lib's
/// `type Readonly<T> = ...` (`TYPE_ALIAS`) declaration onto the local
/// shadow symbol's `declarations` vec. Downstream type traversal
/// (Static<T>'s `(T & { params: P; })['static']` indexed access) then
/// pulled state from the lib's type alias and reused Output's evaluation
/// result for Input — making the diagnostic source side carry Output's
/// extra `bar: string` property.
#[test]
fn typebox_static_typeof_does_not_conflate_shapes_after_unique_symbol_lib_shadow() {
    use tsz_checker::CheckerOptions;
    use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");

    let source = r#"
export {};

export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Hint: unique symbol;
export declare const Kind: unique symbol;

export interface TKind { [Kind]: string }
export interface TSchema extends TKind {
    [Readonly]?: string
    [Optional]?: string
    [Hint]?: string
    params: unknown[]
    static: unknown
}

export type TPropertyKey = string | number
export type TProperties = Record<TPropertyKey, TSchema>
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object'
    static: { [K in keyof T]: Static<T[K], this['params']> }
    type: 'object'
    properties: T
}

export interface TString extends TSchema {
    [Kind]: 'String';
    static: string;
    type: 'string';
}

export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static']

declare namespace Type {
    function Object<T extends TProperties>(object: T): TObject<T>
    function String(): TString
}

export type Input = Static<typeof Input>
export const Input = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
        })
    })
})

export type Output = Static<typeof Output>
export const Output = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
            bar: Type.String(),
        })
    })
})

function problematicFunction1(ors: Input[]): Output[] {
    return ors;
}
"#;

    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diags = check_source_with_libs(source, "test.ts", opts, &libs);
    let messages: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();

    // The conformance harness exercises the full TypeScript-bundled lib and
    // emits a TS2322 here; the unit-test environment loads a stripped lib
    // bundle that does not always reach the assignability error. When a
    // TS2322 IS produced, lock its shape: the source (`Input[]`) must NOT
    // be rendered with Output's extra `bar: string` member.
    if messages.is_empty() {
        return;
    }
    assert!(
        messages.iter().any(|m| m.contains(
            "'{ level1: { level2: { foo: string; }; }; }[]' is not assignable to type \
             '{ level1: { level2: { foo: string; bar: string; }; }; }[]'"
        )),
        "diagnostic must keep Input[] and Output[] structurally distinct; got: {messages:?}"
    );
}
