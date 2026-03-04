//! Tests for JSDoc @callback rest parameter and @typedef nested property handling.

use crate::test_utils::check_js_source_diagnostics;

/// @callback with @param {...string} should create a rest parameter accepting
/// variable string arguments. No TS2554 should be emitted for extra arguments.
#[test]
fn test_jsdoc_callback_rest_param_no_false_arity_error() {
    let source = r#"
/**
 * @callback Foo
 * @param {...string} args
 * @returns {number}
 */

/** @type {Foo} */
const x = () => 1
var res = x('a', 'b')
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for rest parameter call, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @callback with @param {...*} (rest of any) should accept any number of arguments.
#[test]
fn test_jsdoc_callback_rest_any_param() {
    let source = r#"
/**
 * @callback Handler
 * @param {...*} args
 * @returns {void}
 */

/** @type {Handler} */
const h = function() {}
h(1, 'a', true)
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for rest any parameter call, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @typedef with nested @property using dotted names should create nested object types.
/// @property {Object} icons followed by @property {string} icons.image32
/// should produce { icons: { image32: string } }, not { icons: any, "icons.image32": string }.
#[test]
fn test_jsdoc_typedef_nested_property() {
    let source = r#"
/** @typedef {Object} App
 * @property {string} name
 * @property {Object} icons
 * @property {string} icons.image32
 * @property {string} icons.image64
 */
var ex;

/** @type {App} */
const app = {
    name: 'name',
    icons: {
        image32: 'x.png',
        image64: 'y.png',
    }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2739 = diagnostics.iter().filter(|d| d.code == 2739).count();
    assert_eq!(
        ts2739,
        0,
        "Expected no TS2739: nested @property should create nested object, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Closure-compiler function type syntax with rest params:
/// function(boolean, string, ...*):void should accept variadic arguments.
#[test]
fn test_jsdoc_closure_function_type_rest_param() {
    let source = r#"
/**
 * @type {function(boolean, string, ...*):void}
 */
const foo = function (a, b) { };
foo(false, '', 1, 2, 3);
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for Closure function type rest param, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
