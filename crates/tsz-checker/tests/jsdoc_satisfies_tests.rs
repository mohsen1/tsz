//! Tests for JSDoc @satisfies tag behavior.
//!
//! Verifies that @satisfies annotations provide contextual types for
//! object literal methods and arrow functions, and that assignability
//! checks are correctly performed.

use crate::test_utils::check_js_source_diagnostics;

/// @satisfies on parenthesized expression with method signature in typedef
/// provides contextual typing for method parameters.
#[test]
fn test_jsdoc_satisfies_typedef_method_contextual_typing() {
    let source = r#"
/** @typedef {{ move(distance: number): void }} Movable */

const car = /** @satisfies {Movable} */ ({
    move(d) {
        // d should be contextually typed as number
    }
});
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert_eq!(
        ts7006,
        0,
        "Expected no TS7006: @satisfies should provide contextual type to method params, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @satisfies on parenthesized expression with inline method signature
/// provides contextual typing.
#[test]
fn test_jsdoc_satisfies_inline_method_contextual_typing() {
    let source = r#"
const x = /** @satisfies {{ greet(name: string): void }} */ ({
    greet(n) {
        // n should be contextually typed as string
    }
});
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert_eq!(
        ts7006,
        0,
        "Expected no TS7006: inline method sig should provide contextual type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @satisfies on variable declaration provides contextual typing for arrow function parameters.
#[test]
fn test_jsdoc_satisfies_variable_decl_arrow_contextual_typing() {
    let source = r#"
/** @satisfies {{ f: (x: string) => string }} */
const t1 = { f: (s) => s.toLowerCase() };
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert_eq!(
        ts7006,
        0,
        "Expected no TS7006: @satisfies on var decl should provide contextual type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @satisfies on variable declaration detects excess properties.
#[test]
fn test_jsdoc_satisfies_variable_decl_excess_property() {
    let source = r#"
/** @satisfies {{ f: (x: string) => string }} */
const t2 = { g: "oops" };
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2353 = diagnostics.iter().filter(|d| d.code == 2353).count();
    assert!(
        ts2353 >= 1,
        "Expected TS2353 for excess property 'g', got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// JSDoc inline object type with method signature `{ name(param: T): R }` is parsed correctly.
#[test]
fn test_jsdoc_inline_object_method_signature_parsing() {
    let source = r#"
/** @type {{ greet(name: string): string }} */
var obj;
obj.greet("hello");
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339: method 'greet' should exist on the object type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @typedef with inline object containing method signatures works correctly.
#[test]
fn test_jsdoc_typedef_inline_method_signature() {
    let source = r#"
/** @typedef {{ start(): void; stop(): void; run(speed: number): void }} Engine */
/** @type {Engine} */
var engine;
engine.start();
engine.stop();
engine.run(42);
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339: all methods should exist on Engine typedef type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Export-default `@satisfies` should not introduce TS7022; keep behavior aligned
/// with explicit `satisfies` expression typing.
#[test]
fn test_jsdoc_satisfies_export_default_skips_self_reference_circularity() {
    let source = r#"
/**
 * @typedef {Object} Foo
 * @property {number} a
 */
export default /** @satisfies {Foo} */ ({});
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts7022 = diagnostics.iter().filter(|d| d.code == 7022).count();
    assert_eq!(
        ts7022,
        0,
        "Expected no TS7022 for export-default @satisfies wrapper, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// JSDoc @param types on exported functions should suppress TS7006.
/// The JSDoc comment is before `export`, but function pos is at `function`.
/// Regression test: find_jsdoc_for_function must walk up to ExportDeclaration.
#[test]
fn test_jsdoc_param_suppresses_ts7006_exported_function() {
    let source = r#"
/**
 * @param {number} a
 * @param {number} b
 */
export function d(a, b) { return null; }
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts7006: Vec<_> = diagnostics.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006.is_empty(),
        "Expected no TS7006 for exported function with JSDoc @param types, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
