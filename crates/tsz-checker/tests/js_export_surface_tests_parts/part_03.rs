#[test]
fn test_module_exports_instance_with_late_property_writes() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.ts",
        r#"
import npmlog = require("./npmlog.js");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_instance_with_late_property_writes_js_require() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.js",
        r#"
var npmlog = require("./npmlog.js");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for JS require() of instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_instance_with_late_property_writes_js_require_no_extension() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.js",
        r#"
var npmlog = require("./npmlog");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for extensionless JS require() of instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_arrow_function() {
    // module.exports = () => 42;
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = function() { return 42; };"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var result = lib();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports (function expr), got: {ts2349:#?}"
    );
}

// --- exports.foo = X regression tests ---

#[test]
fn test_exports_foo_function_value() {
    // exports.foo = function() {}; — function-valued export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.add = function(a, b) { return a + b; };
exports.sub = function(a, b) { return a - b; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.add(1, 2);
lib.sub(3, 1);
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports.foo function values, got: {ts2339:#?}"
    );
    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for exports.foo function calls, got: {ts2349:#?}"
    );
}

#[test]
fn test_exports_foo_object_value() {
    // exports.config = { debug: true, port: 3000 }; — object-valued export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.config = { debug: true, port: 3000 };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var d = lib.config.debug;
var p = lib.config.port;
"#,
        "./lib.js",
    );

    let ts2339_config: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("'config'"))
        .collect();
    assert!(
        ts2339_config.is_empty(),
        "Expected no TS2339 for exports.config, got: {ts2339_config:#?}"
    );
}

// --- Prototype property assignment tests ---

#[test]
fn test_prototype_method_types_preserved_cross_file() {
    // Constructor with prototype methods exported cross-file.
    // Import side should be able to `new` and access methods without errors.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function EventEmitter() {
    this.listeners = [];
}
EventEmitter.prototype.on = function(event, cb) { this.listeners.push(cb); };
EventEmitter.prototype.emit = function(event) {};
module.exports = EventEmitter;
"#,
        "consumer.ts",
        r#"
import EventEmitter = require("./lib.js");
var ee = new EventEmitter();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor with prototype methods, got: {ts2351:#?}"
    );
}

#[test]
fn test_prototype_assignment_multiple_constructors() {
    // Multiple constructor functions with prototypes in same file.
    // Only the exported one matters for cross-file imports.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Dog() { this.name = "dog"; }
Dog.prototype.bark = function() { return "woof"; };
function Cat() { this.name = "cat"; }
Cat.prototype.meow = function() { return "meow"; };
module.exports = Dog;
"#,
        "consumer.ts",
        r#"
import Dog = require("./lib.js");
var d = new Dog();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for multi-constructor file, got: {ts2351:#?}"
    );
}

// --- Constructor function + property assignment merge tests ---

#[test]
fn test_constructor_with_static_method_and_instance_props() {
    // Constructor with this.props, static methods, and prototype methods.
    // Consumer should be able to construct and call static method.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Counter(initial) { this.count = initial || 0; }
Counter.prototype.increment = function() { this.count++; };
Counter.create = function(n) { return new Counter(n); };
module.exports = Counter;
"#,
        "consumer.ts",
        r#"
import Counter = require("./lib.js");
var c = new Counter(0);
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+static+prototype, got: {ts2351:#?}"
    );
}

#[test]
fn test_module_exports_function_with_property_augmentation() {
    // module.exports = fn; module.exports.version = "1.0";
    // Should be callable AND have the property.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function doWork() { return true; }
module.exports = doWork;
module.exports.version = "1.0";
"#,
        "consumer.ts",
        r#"
import doWork = require("./lib.js");
doWork();
doWork.version;
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable export with properties, got: {ts2349:#?}"
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("version"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports.version augmentation, got: {ts2339:#?}"
    );
}

// --- Import-side lookup of surface-synthesized shapes ---

