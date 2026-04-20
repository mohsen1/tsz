#[test]
fn test_iife_export_assignments() {
    // Exports inside IIFEs should be recognized
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
(function() {
    exports.fromIife = function() { return 1; };
})();
exports.direct = 42;
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.fromIife;
lib.direct;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for IIFE + direct export mix, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Caching validation
// ==========================================================================

#[test]
fn test_multiple_require_of_same_module_consistent() {
    // Two imports of the same module should get the same type
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"exports.value = 42;"#,
        "consumer.ts",
        r#"
import a = require("./lib.js");
import b = require("./lib.js");
a.value;
b.value;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected consistent resolution for repeated require(), got: {ts2339:#?}"
    );
}

// ==========================================================================
// Nested property assignments shaping exports
// ==========================================================================

#[test]
fn test_nested_property_assignment_exports() {
    // exports.utils = {}; exports.utils.helper = function() {};
    // The nested pattern should be collected by the surface as a top-level export.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.utils = {};
exports.config = { debug: false };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.utils;
lib.config;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for nested property assignment exports, got: {ts2339:#?}"
    );
}

// ==========================================================================
// module.exports = primitive + property augmentation
// ==========================================================================

#[test]
fn test_module_exports_string_primitive() {
    // module.exports = "hello";
    // Consumer should see a string type.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = "hello";"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var x = lib;
"#,
        "./lib.js",
    );

    // Should not crash or produce TS2307 (module not found)
    let ts2307: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2307).collect();
    assert!(
        ts2307.is_empty(),
        "Expected no TS2307 for module.exports = primitive, got: {ts2307:#?}"
    );
}

// ==========================================================================
// Constructor-function + prototype/property assignment merges
// ==========================================================================

#[test]
fn test_constructor_with_prototype_methods_cross_file() {
    // Producer defines constructor + prototype methods;
    // consumer should be able to `new` it without TS2351.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Widget() { this.name = "widget"; }
Widget.prototype.getName = function() { return this.name; };
Widget.prototype.setName = function(n) { this.name = n; };
module.exports = Widget;
"#,
        "consumer.ts",
        r#"
import Widget = require("./lib.js");
var w = new Widget();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+prototype export, got: {ts2351:#?}"
    );
}

#[test]
fn test_constructor_static_and_instance_merge() {
    // A constructor function with both static properties and prototype methods.
    // The surface should merge both into the exported shape.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Logger() {}
Logger.prototype.log = function(msg) {};
Logger.level = "info";
module.exports = Logger;
"#,
        "consumer.ts",
        r#"
import Logger = require("./lib.js");
new Logger();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+static+prototype export, got: {ts2351:#?}"
    );
}

// ==========================================================================
// Import-side named import lookup through surface
// ==========================================================================

#[test]
fn test_named_import_from_exports_property() {
    // Named import `{ foo }` from a file that uses `exports.foo = ...`
    // should resolve through the unified surface without TS2305.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.greet = function(name) { return "hello " + name; };
exports.VERSION = "1.0.0";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.greet("world");
lib.VERSION;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for named property imports via surface, got: {ts2339:#?}"
    );
}

// ==========================================================================
// module.exports with export property assignment merge
// ==========================================================================

#[test]
fn test_module_export_with_export_property_assignment() {
    // module.exports = function() {};
    // module.exports.helper = function() {};
    // The surface should merge direct export + named property.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports = function main() { return 1; };
module.exports.helper = function() { return 2; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib();
lib.helper();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports + property merge, got: {ts2349:#?}"
    );
}

// ==========================================================================
// Export alias patterns (var x = exports; x.foo = ...)
// ==========================================================================

#[test]
fn test_export_alias_variable() {
    // var e = exports; e.foo = 42;
    // The alias pattern should be recognized by the surface.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
var e = exports;
e.myFunc = function() { return 42; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.myFunc;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for export alias pattern, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Phase 2: Additional regression tests for surface-routed consumers
// ==========================================================================

// --- module.exports = X (direct export) regression tests ---

#[test]
fn test_module_exports_class_instance() {
    // module.exports = new SomeClass(); should export the instance shape
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Foo() { this.x = 10; this.y = 20; }
module.exports = new Foo();
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var a = lib.x;
var b = lib.y;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && (msg.contains("'x'") || msg.contains("'y'")))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports = new Foo(), got: {ts2339:#?}"
    );
}

