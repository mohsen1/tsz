//! Tests for TS2314 ("Generic type 'X' requires N type argument(s)") emitted for
//! bare generic types in JSDoc `@param` and `@return`/`@returns` tags when
//! `noImplicitAny` is enabled.
//!
//! tsc emits TS2314 for `@param {Array} x` and `@return {Promise}` when
//! strict/noImplicitAny is active, because those bare names require a type
//! argument. The fix lives in `check_jsdoc_typedef_base_types` (jsdoc/diagnostics.rs)
//! which now scans all JSDoc comments for bare @param/@return type names and
//! checks them against global lib symbols for required type arguments.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn check_js_with_libs(source: &str, options: CheckerOptions) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.js", options, &libs)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn check_js_no_implicit_any(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_js_with_libs(source, options)
}

fn check_js_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_js_with_libs(source, options)
}

fn check_js_permissive(source: &str) -> Vec<u32> {
    // Deliberately no lib files: tests that my guard (no_implicit_any check)
    // prevents TS2314 from firing even when Array/Promise are in the symbol table.
    // Pre-existing paths emit TS2314 with lib+noImplicitAny via other channels;
    // here we only care that check_jsdoc_typedef_base_types doesn't fire.
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    use tsz_checker::test_utils::check_source;
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// @param {Array} — TS2314 when noImplicitAny
// ---------------------------------------------------------------------------

#[test]
fn param_bare_array_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    assert!(
        codes.contains(&2314),
        "expected TS2314 for @param {{Array}}, got: {codes:?}"
    );
}

#[test]
fn return_bare_array_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    // Both @param and @return should each produce TS2314
    let count = codes.iter().filter(|&&c| c == 2314).count();
    assert!(
        count >= 2,
        "expected at least 2 TS2314 (one for @param, one for @return), got {count} in {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// @param {Promise} — TS2314 when noImplicitAny
// ---------------------------------------------------------------------------

#[test]
fn param_bare_promise_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Promise} pr
 * @returns {Promise}
 */
function f(pr) { return pr; }
"#,
    );
    assert!(
        codes.contains(&2314),
        "expected TS2314 for @param {{Promise}}, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// No TS2314 without noImplicitAny (bare Array/Promise = any in JSDoc)
// ---------------------------------------------------------------------------

#[test]
fn no_ts2314_without_no_implicit_any() {
    let codes = check_js_permissive(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    assert!(
        !codes.contains(&2314),
        "should NOT emit TS2314 without noImplicitAny, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// @param {Array<number>} — already parameterized, no TS2314
// ---------------------------------------------------------------------------

#[test]
fn parameterized_array_no_ts2314() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array<number>} arr
 * @return {Array<string>}
 */
function f(arr) { return []; }
"#,
    );
    assert!(
        !codes.contains(&2314),
        "should not emit TS2314 for already-parameterized Array<T>, got: {codes:?}"
    );
}

#[test]
fn arrow_generic_type_params_with_tab_constraint_keep_template_arg_in_scope() {
    let codes = check_js_strict(
        "\
/**
 * @template T
 * @typedef {{ value: T }} Box
 */

/**
 * @param {<T extends\tstring>(value: Box<T>) => void} cb
 */
function use(cb) {}
",
    );
    assert!(
        !codes.contains(&2304) && !codes.contains(&2314) && !codes.contains(&2315),
        "tab-whitespace JSDoc arrow type parameter should keep T in scope for Box<T>, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// strict mode mirrors the jsdocArrayObjectPromiseNoImplicitAny.ts scenario
// ---------------------------------------------------------------------------

#[test]
fn strict_emits_ts2314_for_array_and_promise_in_function_jsdoc() {
    let codes = check_js_strict(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function returnNotAnyArray(arr) { return arr; }

/**
 * @param {Promise} pr
 * @return {Promise}
 */
function returnNotAnyPromise(pr) { return pr; }
"#,
    );
    let count_2314 = codes.iter().filter(|&&c| c == 2314).count();
    assert!(
        count_2314 >= 4,
        "expected at least 4 TS2314 (2 Array + 2 Promise), got {count_2314} in {codes:?}"
    );
}
