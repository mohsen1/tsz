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
