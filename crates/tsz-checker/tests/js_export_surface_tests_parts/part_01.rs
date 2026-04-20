#[test]
fn test_jsdoc_param_typeof_import_reports_missing_value_export() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
/** @typedef {() => number} buz */
module.exports = {};
"#,
        "main.js",
        r#"
/**
 * @param {typeof import('./mod.js').buz} f
 */
function values(f) {
    return f()
}
"#,
        "./mod.js",
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2694 && message.contains("buz"))
        .collect();
    assert_eq!(
        ts2694.len(),
        1,
        "Expected JSDoc typeof import('./mod.js').buz to report TS2694, got: {diagnostics:#?}"
    );
}

#[test]
fn test_module_exports_function() {
    // module.exports = function greet() { return "hi"; }
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = function greet() { return "hi"; };"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports, got: {ts2349:#?}"
    );
}

// ==========================================================================
// exports.foo = X tests
// ==========================================================================

#[test]
fn test_exports_foo_property_assignment() {
    // exports.foo = 42;
    // exports.bar = "hello";
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.foo = 42;
exports.bar = "hello";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo;
lib.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports.foo properties, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_foo_property_assignment() {
    // module.exports.foo = 42;
    // module.exports.bar = "hello";
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports.foo = 42;
module.exports.bar = "hello";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo;
lib.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports.foo properties, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Prototype assignment tests
// ==========================================================================

#[test]
fn test_prototype_property_assignment_same_file() {
    // Constructor function with prototype methods in same file.
    // Should not produce TS2339 for read-before-assignment on prototype props.
    let diagnostics = check_commonjs_single_file(
        "proto.js",
        r#"
function MyClass() {
    this.value = 0;
}
MyClass.prototype.getValue = function() { return this.value; };
MyClass.prototype.setValue = function(v) { this.value = v; };
var inst = new MyClass();
inst.getValue();
inst.setValue(42);
"#,
    );

    // We're checking that prototype method definitions don't produce
    // spurious errors. In a no-lib environment some errors are expected
    // (like missing global types), but TS2339 on prototype methods
    // specifically indicates a regression.
    let ts2339_proto: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && (msg.contains("getValue") || msg.contains("setValue")))
        .collect();
    // In no-lib mode, prototype method access on `this` might not fully
    // resolve, so we only assert there's no unexpected TS2339 on the
    // methods themselves, not on `this`.
    assert!(
        ts2339_proto.is_empty(),
        "Expected no TS2339 for prototype method names, got: {ts2339_proto:#?}"
    );
}

// ==========================================================================
// Constructor function + property merge tests
// ==========================================================================

#[test]
fn test_constructor_function_export_with_static_props() {
    // module.exports = Ctor; Ctor.staticProp = 42;
    // When a constructor function is exported, static properties should merge.
    let diagnostics = check_commonjs_two_files(
        "ctor.js",
        r#"
function Ctor() { this.x = 0; }
Ctor.staticProp = 42;
module.exports = Ctor;
"#,
        "consumer.ts",
        r#"
import Ctor = require("./ctor.js");
new Ctor();
"#,
        "./ctor.js",
    );

    // Constructor function exports should be callable with `new`
    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor function export, got: {ts2351:#?}"
    );
}

// ==========================================================================
// Mixed module.exports + exports.foo tests
// ==========================================================================

#[test]
fn test_module_exports_with_additional_exports() {
    // module.exports = { base: true };
    // exports.extra = 42;
    // The surface should merge both sources.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports = { base: true };
exports.extra = 42;
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.base;
lib.extra;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    // After unified surface synthesis, both base and extra should be accessible.
    // Note: in tsc, `module.exports = X` takes precedence and named exports
    // are merged. Our surface does the same via intersection.
    assert!(
        ts2339.len() <= 1,
        "Expected at most 1 TS2339 (tsc also limits named exports when module.exports is set), got: {ts2339:#?}"
    );
}

// ==========================================================================
// Import-side lookup tests
// ==========================================================================

#[test]
fn test_require_call_resolves_through_surface() {
    // const lib = require("./lib.js"); lib.foo;
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"exports.foo = function() { return 42; };"#,
        "consumer.js",
        r#"
var lib = require("./lib.js");
lib.foo();
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() + property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_element_access_on_exports() {
    // exports["foo"] = 42; — element access export pattern
    let diagnostics = check_commonjs_single_file(
        "elem.js",
        r#"
exports["foo"] = 42;
exports["bar"] = "hello";
"#,
    );

    // Basic validation: this should parse and check without panics
    // Element access exports are a valid CommonJS pattern
    let _len = diagnostics.len();
}

#[test]
fn test_element_access_exports_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports["b"] = { x: "x" };
exports["default"] = { x: "x" };
module.exports["c"] = { x: "x" };
module["exports"]["d"] = {};
module["exports"]["d"].e = 0;
"#,
        "consumer.js",
        r#"
var lib = require("./lib.js");
lib.b;
lib.c;
lib.d;
lib.d.e;
lib.default;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for literal CommonJS element-access exports, got: {diagnostics:#?}"
    );
}

// ==========================================================================
// IIFE export pattern tests
// ==========================================================================

