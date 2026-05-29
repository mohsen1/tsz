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
fn test_shadowed_object_define_property_does_not_create_same_file_commonjs_export() {
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
const Object = {
    defineProperty(target, key, descriptor) {
        target[key] = descriptor.value;
    }
};

Object.defineProperty(exports, "shadowed", { value: 1 });
/** @type {string} */
var s = exports.shadowed;
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, message)| *code != 2322 || !message.contains("number")),
        "Expected shadowed local `Object.defineProperty` not to synthesize numeric CommonJS export `shadowed`, got: {diagnostics:#?}"
    );
}

#[test]
fn test_chained_undefined_export_assignment_reports_outer_implicit_any() {
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
exports.first = exports.second = exports.third = undefined;
exports.direct = undefined;
"#,
    );

    let ts7005: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7005)
        .collect();
    assert_eq!(
        ts7005.len(),
        2,
        "Expected TS7005 only for non-terminal chained export assignments, got: {diagnostics:#?}"
    );

    let messages: Vec<_> = ts7005.iter().map(|(_, message)| message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Variable 'first' implicitly has an 'any' type.")),
        "Expected TS7005 for `exports.first`, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Variable 'second' implicitly has an 'any' type.")),
        "Expected TS7005 for `exports.second`, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("'third'") && !message.contains("'direct'")),
        "Terminal/direct undefined export assignments should not report TS7005, got: {messages:#?}"
    );
}

#[test]
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

    // `tsc --allowJs --checkJs --strict --module commonjs` accepts this whole
    // sequence. CommonJS export-property assignments are declaration-like, and
    // same-file reads use the most recent prior assignment without checking
    // every write against the final property shape.
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for same-file CommonJS export-property reassignments, got: {ts2322:#?}"
    );
}

#[test]
fn test_current_file_module_exports_reads_use_prior_assignment_types() {
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
module.exports.run = undefined;
function makeRunner() { return 1; }
module.exports.run = makeRunner;
module.exports.run();
module.exports.run = { ok: true };
var ok = module.exports.run.ok;
module.exports.run = "done";
"#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2349))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no assignment/read/call diagnostics for same-file `module.exports` reassignments, got: {relevant:#?}"
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

#[test]
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

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_renamed_commonjs_named_class() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
class Widget {
    values() {
        return new Widget();
    }
}
module.exports.Widget = Widget;
"#,
        "main.js",
        r#"
const { Widget: LocalWidget } = require("./mod1");
/** @param {LocalWidget} k */
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
        "Expected renamed destructured CommonJS named class JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_nested_commonjs_class() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
var NS = {};
NS.K = class {
    values() {
        return new NS.K();
    }
};
exports.K = NS.K;
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
        .filter(|(code, _)| matches!(*code, 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured nested CommonJS class JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
fn test_commonjs_named_class_export_assignment_keeps_constructor_side() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
class K {
    values() {
        return new K();
    }
}
exports.K = K;
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected CommonJS named class export assignment to keep constructor-side typing, got: {relevant:#?}"
    );
}

#[test]
fn test_commonjs_nested_class_expando_assignment_keeps_constructor_side() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
var NS = {};
NS.K = class {
    values() {
        return new NS.K();
    }
};
exports.K = NS.K;
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected nested CommonJS class expando assignment to keep constructor-side typing, got: {relevant:#?}"
    );
}

#[test]
fn test_commonjs_module_exports_nested_namespace_keeps_nested_constructor_side() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
module.exports.n = {};
module.exports.n.K = function C() {
    this.x = 10;
};
module.exports.Classic = class {
    constructor() {
        this.p = 1;
    }
};
"#,
        "use.js",
        r#"
import * as s from "./mod";

var k = new s.n.K();
k.x;
var classic = new s.Classic();

/** @param {s.n.K} c
    @param {s.Classic} classic */
function f(c, classic) {
    c.x;
    classic.p;
}
"#,
        "./mod",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected nested CommonJS module.exports namespace constructor access to stay typed, got: {relevant:#?}"
    );
}

// --- Surface caching correctness ---

#[test]
fn test_surface_cache_consistent_with_multiple_consumers() {
    // Two different consumer files importing the same producer.
    // The cached surface should produce consistent results.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.x = 1;
exports.y = 2;
exports.z = 3;
"#,
        "consumer.ts",
        r#"
import a = require("./lib.js");
import b = require("./lib.js");
a.x;
a.y;
b.z;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected consistent surface cache across multiple imports, got: {ts2339:#?}"
    );
}

// --- Object.defineProperty export tests ---

#[test]
fn test_define_property_export_cross_file() {
    // Object.defineProperty(exports, "foo", { value: 42 });
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
Object.defineProperty(exports, "myProp", { value: 42 });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.myProp;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("myProp"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for Object.defineProperty export, got: {ts2339:#?}"
    );
}

#[test]
fn test_define_property_export_preserves_write_type_and_readonly_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
Object.defineProperty(exports, "foo", { value: "ok", writable: true });
Object.defineProperty(exports, "bar", { value: "fixed" });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo = 1;
lib.bar = "nope";
"#,
        "./lib.js",
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    let ts2540: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2540).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for writable defineProperty export with string type, got: {diagnostics:#?}"
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty export, got: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_export_only_tracks_literal_names_cross_file() {
    // tsc's binder recognizes `Object.defineProperty(exports, X, ...)` as a
    // synthesizable export only when `X` is a syntactic literal. References
    // to `const`/`let` bindings — even ones initialized with a string literal
    // — are NOT propagated, so the corresponding properties never appear on
    // the synthesized exports type and import-side accesses surface as TS2339.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
const dynamicName = "other";
const constName = "prop";
Object.defineProperty(exports, "thing", { value: 42, writable: true });
Object.defineProperty(exports, dynamicName, { value: 42, writable: true });
Object.defineProperty(exports, constName, { value: 42, writable: true });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.thing;
lib.other;
lib.prop;
"#,
        "./lib.js",
    );

    let thing_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("thing"))
        .collect();
    assert!(
        thing_missing.is_empty(),
        "Expected literal defineProperty export to stay visible, got: {diagnostics:#?}"
    );

    let other_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("other"))
        .collect();
    let prop_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("prop"))
        .collect();
    assert!(
        !other_missing.is_empty(),
        "Expected TS2339 for binding-named defineProperty export 'other', got: {diagnostics:#?}"
    );
    assert!(
        !prop_missing.is_empty(),
        "Expected TS2339 for binding-named defineProperty export 'prop', got: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_export_malformed_descriptor_is_readonly_cross_file() {
    // tsc treats malformed/mixed `Object.defineProperty` descriptors as
    // readonly any-typed properties: an empty `{}`, a mixed accessor+data
    // (`get`+`value`), and a lone `writable: true` (no value, no accessor)
    // all produce properties that exist for read access but reject writes
    // with TS2540. Only a paired `value` + `writable: true` data descriptor
    // or an explicit `set` accessor makes the property writable.
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
Object.defineProperty(exports, "writableThing", { value: 42, writable: true });
Object.defineProperty(exports, "bad1", { });
Object.defineProperty(exports, "bad2", { get() { return 12 }, value: "no" });
Object.defineProperty(exports, "bad3", { writable: true });
"#,
        "importer.ts",
        r#"
import mod = require("./mod1");
mod.writableThing = 0;
mod.bad1 = 0;
mod.bad2 = 0;
mod.bad3 = 0;
"#,
        "./mod1",
    );

    let writable_thing_readonly: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2540 && msg.contains("writableThing"))
        .collect();
    assert!(
        writable_thing_readonly.is_empty(),
        "Expected no TS2540 for value+writable:true descriptor, got: {diagnostics:#?}"
    );

    for name in ["bad1", "bad2", "bad3"] {
        let readonly: Vec<_> = diagnostics
            .iter()
            .filter(|(c, msg)| *c == 2540 && msg.contains(name))
            .collect();
        assert!(
            !readonly.is_empty(),
            "Expected TS2540 for malformed descriptor '{name}', got: {diagnostics:#?}"
        );
    }
}

#[test]
fn test_plain_object_define_property_augments_local_js_object_type() {
    let x_type = format_commonjs_single_file_symbol_type(
        "lib.js",
        r#"
const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "zipStr", {
  /** @param {string} str */
  set(str) {}
});
"#,
        "x",
    );
    let diagnostics = check_commonjs_single_file(
        "lib.js",
        r#"
const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "zipStr", {
  /** @param {string} str */
  set(str) {}
});
/** @param {{name: string}} named */
function takeName(named) { return named.name; }
takeName(x);
x.name = 12;
x.middleInit = "R";
x.zipStr = 12;
"#,
    );

    assert!(
        x_type.contains("name"),
        "Expected local symbol type to include defineProperty members, got: {x_type}"
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    let ts2345: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2345).collect();
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    let ts2540: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2540).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 after Object.defineProperty augments object type, got: {ts2339:#?}"
    );
    assert!(
        ts2345.is_empty(),
        "Expected no TS2345 for passing augmented object to typed consumer, got: {ts2345:#?}"
    );
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for setter-backed string property assignment, got: {diagnostics:#?}"
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty member assignment, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_is_union_typed_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
        "consumer.ts",
        r#"
import mod1 = require("./mod1");
declare function takesNumber(value: number): void;
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let number_mismatch_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        number_mismatch_errors.len() >= 2,
        "Expected overlapping CommonJS exports to stay union-typed and reject number-only consumers, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_reports_ts2323_in_js_file() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
    );

    let ts2323: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2323)
        .collect();
    assert_eq!(
        ts2323.len(),
        4,
        "Expected TS2323 on overlapping CommonJS exported property declarations, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_reports_ts2323_with_prelude_file() {
    let diagnostics = check_commonjs_file_with_prelude(
        "requires.d.ts",
        r#"
declare var module: { exports: any };
declare function require(name: string): any;
"#,
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
    );

    let ts2323: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2323)
        .collect();
    assert_eq!(
        ts2323.len(),
        4,
        "Expected TS2323 on overlapping CommonJS exported property declarations with a preceding declaration file, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_rejects_number_only_js_require_consumers() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
"#,
        "consumer.js",
        r#"
/** @param {number} value */
function takesNumber(value) {}
var mod1 = require("./mod1");
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() >= 2,
        "Expected JS require() consumer to see overlapping CommonJS exports as non-number-only, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_object_literal_overlap_rejects_number_only_js_require_consumers() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
module.exports = {
    justExport: 1,
    bothBefore: 2,
    bothAfter: 3,
};
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
        "consumer.js",
        r#"
/** @param {number} value */
function takesNumber(value) {}
var mod1 = require("./mod1");
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() >= 2,
        "Expected object-literal CommonJS overlap to stay union-typed for JS require() consumers, got: {diagnostics:#?}"
    );
}

// --- Mixed patterns: module.exports + exports.foo + prototype ---

#[test]
fn test_full_commonjs_pattern_mix() {
    // All three patterns in one file:
    // 1. Constructor function as module.exports
    // 2. Static property via module.exports.prop
    // 3. Prototype method
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Parser() { this.input = ""; }
Parser.prototype.parse = function(s) { this.input = s; return {}; };
Parser.defaultOptions = { strict: true };
module.exports = Parser;
module.exports.VERSION = "2.0";
"#,
        "consumer.ts",
        r#"
import Parser = require("./lib.js");
var p = new Parser();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for full CommonJS pattern mix, got: {ts2351:#?}"
    );
}
