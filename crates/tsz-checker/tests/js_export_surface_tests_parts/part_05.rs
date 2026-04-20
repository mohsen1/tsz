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
fn test_define_property_export_tracks_constant_names_cross_file() {
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
    let missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("thing") || msg.contains("other") || msg.contains("prop"))
        })
        .collect();
    assert!(
        thing_missing.is_empty(),
        "Expected literal defineProperty export to stay visible, got: {diagnostics:#?}"
    );
    assert!(
        missing.is_empty(),
        "Expected constant-name defineProperty exports to stay visible cross-file, got: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_export_supports_constant_names_and_malformed_descriptors_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
const obj = { value: 42, writable: true };
Object.defineProperty(exports, "thing", obj);

/** @type {string} */
let str = /** @type {string} */("other");
Object.defineProperty(exports, str, { value: 42, writable: true });

const propName = "prop";
Object.defineProperty(exports, propName, { value: 42, writable: true });

Object.defineProperty(exports, "bad1", { });
Object.defineProperty(exports, "bad2", { get() { return 12 }, value: "no" });
Object.defineProperty(exports, "bad3", { writable: true });
"#,
        "importer.js",
        r#"
const mod = require("./mod1");
mod.thing;
mod.other;
mod.prop;
mod.bad1;
mod.bad2;
mod.bad3;

mod.thing = 0;
mod.other = 0;
mod.prop = 0;
mod.bad1 = 0;
mod.bad2 = 0;
mod.bad3 = 0;
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2540))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected constant-name defineProperty exports and malformed descriptors to stay permissive cross-file, got: {diagnostics:#?}"
    );
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

