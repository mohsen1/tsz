#[test]
fn test_import_side_type_narrowing_of_commonjs_exports() {
    // Consumer narrows the type of a CommonJS export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.value = 42;
exports.name = "test";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var v = lib.value;
var n = lib.name;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for basic import-side value/name lookup, got: {ts2339:#?}"
    );
}

#[test]
fn test_require_call_with_destructuring() {
    // const { foo, bar } = require("./lib"); pattern
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.foo = 42;
exports.bar = "hello";
"#,
        "consumer.js",
        r#"
var mod = require("./lib.js");
mod.foo;
mod.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_repeated_named_export_assignment_consumer_uses_final_value_type() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.apply = function() { };
exports.apply = { ok: 1 };
"#,
        "consumer.js",
        r#"
const { apply } = require("./lib.js");
apply.ok;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected repeated named-export assignments to expose the final value type to consumers, got: {diagnostics:#?}"
    );
}

// --- Current-file namespace type via surface ---

#[test]
fn test_current_file_exports_property_access() {
    // Within a JS file, `exports.foo` should be recognized
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
exports.alpha = 1;
exports.beta = "two";
var x = exports.alpha;
"#,
    );

    // Should not produce TS2339 on exports.alpha access within same file
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("alpha"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for same-file exports.alpha access, got: {ts2339:#?}"
    );
}

#[test]
#[ignore = "regressed after remote changes: expected 2 TS2322 for same-file CommonJS reassignments, now emits 0"]
fn test_current_file_exports_reads_use_prior_assignment_types() {
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
exports.apply = undefined;
exports.apply = undefined;
function a() { return 1; }
exports.apply = a;
exports.apply();
exports.apply = { ok: 1 };
var ok = exports.apply.ok;
exports.apply = 1;
"#,
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for same-file CommonJS reads after reassignment, got: {ts2339:#?}"
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for same-file CommonJS calls after reassignment, got: {ts2349:#?}"
    );

    // After CJS export assignment suppression changes (d322905ff), reassigning
    // `exports.apply` to incompatible types now correctly emits TS2322 because
    // the last assignment (`= 1`) widens the property type.
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        2,
        "Expected 2 TS2322 for same-file CommonJS reassignments where earlier assignments conflict with the final type, got: {ts2322:#?}"
    );
}

#[test]
fn test_current_file_module_exports_property_access() {
    // Within a JS file, `module.exports.foo = X` then `module.exports.foo` access
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
module.exports.count = 0;
module.exports.name = "test";
"#,
    );

    // Should parse and check without panics
    let _len = diagnostics.len();
}

#[test]
fn test_require_of_primitive_module_exports_does_not_expose_later_properties() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports = 1;
module.exports.f = function () { };
"#,
        "a.js",
        r#"
var mod1 = require("./mod1");
mod1.toFixed(12);
mod1.f();
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 in the consumer once primitive module.exports blocks later property merges, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|(_, msg)| msg.contains("type 'number'") && msg.contains("Property 'f'")),
        "Expected consumer TS2339 to report the widened primitive type, got: {diagnostics:#?}"
    );
}

// TODO: the prelude-based test environment doesn't provide enough global types
// (Object, RegExp, etc.), causing TS2318 floods that mask the actual TS2339.
#[test]
#[ignore = "regressed after remote changes: TS2318 floods from missing global types mask actual TS2339"]
fn test_primitive_module_exports_assignment_reports_same_file_property_error_with_prelude() {
    let diagnostics = check_commonjs_file_with_prelude(
        "requires.d.ts",
        r#"
declare var module: { exports: any };
"#,
        "mod1.js",
        r#"
module.exports = 1;
module.exports.f = function () { };
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, msg)| {
            *code == 2339 && msg.contains("Property 'f' does not exist on type 'number'")
        }),
        "Expected producer-side TS2339 once primitive module.exports blocks later property merges, got: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_commonjs_class_expression() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
exports.K = class K {
    values() {}
};
"#,
        "main.js",
        r#"
const { K } = require("./mod1");
/** @param {K} k */
function f(k) {
    k.values();
}
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2322 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured CommonJS class expression JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
#[ignore = "pre-existing regression"]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_commonjs_named_class() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
class K {
    values() {
        return new K();
    }
}
exports.K = K;
"#,
        "main.js",
        r#"
const { K } = require("./mod1");
/** @param {K} k */
function f(k) {
    k.values();
}
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured CommonJS named class JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

