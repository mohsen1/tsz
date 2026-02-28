//! Tests for JSDoc @type tag on class properties and function declarations.
//!
//! Verifies that @type annotations on class fields (including private fields)
//! are used for type checking initializers, and that @type function types
//! provide parameter types for function declarations in JS files.

use crate::test_utils::check_js_source_diagnostics;

/// @type {boolean} on class field with incompatible initializer → TS2322
#[test]
fn test_jsdoc_type_on_class_field_initializer_mismatch() {
    let source = r#"
class A {
    /** @type {boolean} */
    foo = 3
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to boolean @type field, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type {string} on class field with compatible initializer → no error
#[test]
fn test_jsdoc_type_on_class_field_compatible_initializer() {
    let source = r#"
class A {
    /** @type {string} */
    foo = "hello"
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for string assigned to string @type field, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// function(string): void Closure syntax is parsed correctly
#[test]
fn test_jsdoc_function_closure_syntax_contextual_typing() {
    let source = r#"
/** @type {function(string): void} */
var f = function(value) {
    value = 1
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type function, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type function type on function declaration provides parameter types
#[test]
fn test_jsdoc_type_on_function_declaration_provides_param_types() {
    let source = r#"
/** @type {(s: string) => void} */
function g(s) {
    s = 1
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type on function decl, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
