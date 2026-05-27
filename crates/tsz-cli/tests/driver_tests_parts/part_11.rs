#[test]
fn js_checkjs_define_property_module_exports_preserve_augmented_shape() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["index.js", "validate.ts"]
        }"#,
    );

    write_file(
        &base.join("index.js"),
        r#"const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "lastName", { value: "Smith", writable: false });
Object.defineProperty(x, "zip", { get() { return 98122 }, set(_) { /*ignore*/ } });
Object.defineProperty(x, "houseNumber", { get() { return 21.75 } });
Object.defineProperty(x, "zipStr", {
    /** @param {string} str */
    set(str) {
        this.zip = Number(str)
    }
});

/**
 * @param {{name: string}} named
 */
function takeName(named) { return named.name; }

takeName(x);

/** @type {number} */
var a = x.zip;

/** @type {number} */
var b = x.houseNumber;

const returnExemplar = () => x;
const needsExemplar = (_ = x) => void 0;

const expected = /** @type {{name: string, readonly middleInit: string, readonly lastName: string, zip: number, readonly houseNumber: number, zipStr: string}} */(/** @type {*} */(null));

/**
 * @param {typeof returnExemplar} a
 * @param {typeof needsExemplar} b
 */
function match(a, b) {}

match(() => expected, (x = expected) => void 0);

module.exports = x;
"#,
    );

    write_file(
        &base.join("validate.ts"),
        r#"import "./";
import x = require("./");

x.name;
x.middleInit;
x.lastName;
x.zip;
x.houseNumber;
x.zipStr;

x.name = "Another";
x.zip = 98123;
x.zipStr = "OK";

x.lastName = "should fail";
x.houseNumber = 12;
x.zipStr = 12;
x.middleInit = "R";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let ts2540: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for defineProperty-augmented shape, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts2345.is_empty(),
        "Expected no TS2345 when passing defineProperty-augmented object, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for invalid writable assignments, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty members, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_plain_function_self_alias_prototype_method_preserves_members() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noImplicitAny": true,
            "strictNullChecks": true,
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"function Foonly() {
    var self = this
    self.x = 1
    self.m = function() {
        console.log(self.x)
    }
}
Foonly.prototype.mreal = function() {
    var self = this
    self.y = 2
}
const foo = new Foonly()
foo.x
foo.y
foo.m()
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Unexpected TS2339 for self-alias prototype members in project mode: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_commonjs_export_alias_define_property_overlap_reports_ts2323() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["namespacey.js", "namespacer.js"]
        }"#,
    );
    write_file(
        &base.join("namespacey.js"),
        r#"const A = {};
A.bar = class Q {};
module.exports = A;
"#,
    );
    write_file(
        &base.join("namespacer.js"),
        r#"const B = {};
B.NS = require("./namespacey");
Object.defineProperty(B, "NS", { value: "why though", writable: true });
module.exports = B;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2323: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE
                && d.message_text.contains("'NS'")
        })
        .collect();
    assert_eq!(
        ts2323.len(),
        2,
        "Expected TS2323 on overlapping CommonJS alias defineProperty exports, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_commonjs_export_property_overlap_with_ambient_module_reports_ts2323() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["requires.d.ts", "mod1.js", "a.js"]
        }"#,
    );
    write_file(
        &base.join("requires.d.ts"),
        r#"declare var module: { exports: any };
declare function require(name: string): any;
"#,
    );
    write_file(
        &base.join("mod1.js"),
        r#"/// <reference path='./requires.d.ts' />
module.exports.bothBefore = 'string'
A.justExport = 4
A.bothBefore = 2
A.bothAfter = 3
module.exports = A
function A() {
    this.p = 1
}
module.exports.bothAfter = 'string'
module.exports.justProperty = 'string'
"#,
    );
    write_file(
        &base.join("a.js"),
        r#"/// <reference path='./requires.d.ts' />
var mod1 = require('./mod1')
mod1.justExport.toFixed()
mod1.bothBefore.toFixed()
mod1.bothAfter.toFixed()
mod1.justProperty.length
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2323: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE)
        .collect();
    assert_eq!(
        ts2323.len(),
        4,
        "Expected TS2323 on both overlapping CommonJS export properties, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_define_property_commonjs_exports_make_js_files_module_scoped() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true,
            "module": "commonjs",
            "target": "es2020",
            "types": []
          },
          "files": ["exporter.js", "importer.ts", "exporter-module.js", "importer-module.ts"]
        }"#,
    );
    write_file(
        &base.join("exporter.js"),
        r#"// @ts-check
const URL = 1;

Object.defineProperty(exports, "value", {
  value: URL,
});
"#,
    );
    write_file(
        &base.join("importer.ts"),
        r#"import { value } from "./exporter";

const n: number = value;
const s: string = value;
n;
s;
"#,
    );
    write_file(
        &base.join("exporter-module.js"),
        r#"// @ts-check
const Headers = 1;

Object.defineProperty(module.exports, "value", {
  value: Headers,
});
"#,
    );
    write_file(
        &base.join("importer-module.ts"),
        r#"import { value } from "./exporter-module";

const n: number = value;
const s: string = value;
n;
s;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE),
        "Object.defineProperty CommonJS exporters should be module-scoped, got diagnostics: {:?}",
        result.diagnostics
    );

    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected only importer assignment errors, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_nested_commonjs_exports_make_checked_js_files_module_scoped() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true,
            "module": "commonjs",
            "target": "es2020",
            "types": []
          },
          "files": ["a.js"]
        }"#,
    );
    write_file(
        &base.join("a.js"),
        r#"// @ts-check
const URL = 1;
function publish() {
  module.exports.value = URL;
}
/** @type {number} */
const force = "x";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE),
        "nested CommonJS exporters should be module-scoped, got diagnostics: {:?}",
        result.diagnostics
    );

    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected only the JSDoc assignment error, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_checked_js_jsdoc_numeric_literal_type_accepts_exponent_syntax() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "pretty": false,
            "strict": true
          },
          "files": ["test.js"]
        }"#,
    );
    write_file(
        &base.join("test.js"),
        r#"// @ts-check
/** @type {1e3} */
let bad = 999;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains("'999'")
                && d.message_text.contains("'1000'")
        }),
        "Expected TS2322 for JSDoc 1e3 numeric literal type, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_static_expando_members_from_assignments_across_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["a.js", "global.js", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.js"),
        r#"export class C1 { }
C1.staticProp = 0;

export function F1() { }
F1.staticProp = 0;

export var C2 = class { };
C2.staticProp = 0;

export let F2 = function () { };
F2.staticProp = 0;
"#,
    );
    write_file(
        &base.join("global.js"),
        r#"class C3 { }
C3.staticProp = 0;

function F3() { }
F3.staticProp = 0;

var C4 = class { };
C4.staticProp = 0;

let F4 = function () { };
F4.staticProp = 0;
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import * as a from "./a";
var n: number;

var n = a.C1.staticProp;
var n = a.C2.staticProp;
var n = a.F1.staticProp;
var n = a.F2.staticProp;

var n = C3.staticProp;
var n = C4.staticProp;
var n = F3.staticProp;
var n = F4.staticProp;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected JS static expando reads across files to stay error-free, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_class_static_expando_after_constructor_function_merge() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["file1.js", "file2.js"]
        }"#,
    );
    write_file(
        &base.join("file1.js"),
        r#"var SomeClass = function () {
    this.otherProp = 0;
};

new SomeClass();
"#,
    );
    write_file(
        &base.join("file2.js"),
        r#"class SomeClass { }
SomeClass.prop = 0;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected JS class static expando after constructor-function merge to avoid TS2339, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_enum_cross_file_export_keeps_nested_jsdoc_namespace_properties() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["enumDef.js", "index.js"]
        }"#,
    );
    write_file(
        &base.join("enumDef.js"),
        r#"var Host = {};
Host.UserMetrics = {};
/** @enum {number} */
Host.UserMetrics.Action = {
    WindowDocked: 1,
    WindowUndocked: 2,
    ScriptsBreakpointSet: 3,
    TimelineStarted: 4,
};
/**
 * @typedef {string} Host.UserMetrics.Bargh
 */
/**
 * @typedef {string}
 */
Host.UserMetrics.Blah = {
    x: 12
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"var Other = {};
Other.Cls = class {
    /**
     * @param {!Host.UserMetrics.Action} p
     */
    method(p) {}
    usage() {
        this.method(Host.UserMetrics.Action.WindowDocked);
    }
}

/**
 * @type {Host.UserMetrics.Bargh}
 */
var x = "ok";

/**
 * @type {Host.UserMetrics.Blah}
 */
var y = "ok";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected nested JS enum/JSDoc namespace writes to avoid TS2339, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_enum_object_frozen_value_type_survives_jsdoc_references() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "module": "commonjs"
          },
          "files": ["index.js", "usage.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"/** @enum {string} */
const Thing = Object.freeze({
    a: "thing",
    b: "chill"
});

exports.Thing = Thing;

/**
 * @param {Thing} x
 */
function useThing(x) {}

exports.useThing = useThing;

/**
 * @param {(x: Thing) => void} x
 */
function cbThing(x) {}

exports.cbThing = cbThing;
"#,
    );
    write_file(
        &base.join("usage.js"),
        r#"const { Thing, useThing, cbThing } = require("./index");

useThing(Thing.a);

/**
 * @typedef {Object} LogEntry
 * @property {string} type
 * @property {number} time
 */

cbThing(type => {
    /** @type {LogEntry} */
    const logEntry = {
        time: Date.now(),
        type,
    };
});
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code
                != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected JSDoc @enum references on Object.freeze exports to resolve to the enum value type, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsdoc_enum_initializer_values_are_checked() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["enum.js"]
        }"#,
    );
    write_file(
        &base.join("enum.js"),
        r#"
// @ts-check
/** @enum {number} */
const E = { A: "x" };

/** @enum {string} */
const Frozen = Object.freeze({ A: 1 });
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        2,
        "Expected TS2322 for mismatched JSDoc @enum object values, got: {:?}",
        result.diagnostics
    );
    assert!(
        ts2322.iter().any(
            |diag| diag.message_text.contains("string") && diag.message_text.contains("number")
        ),
        "Expected string-to-number enum initializer diagnostic, got: {ts2322:?}"
    );
    assert!(
        ts2322.iter().any(
            |diag| diag.message_text.contains("number") && diag.message_text.contains("string")
        ),
        "Expected number-to-string Object.freeze enum initializer diagnostic, got: {ts2322:?}"
    );
}

#[test]
fn compile_jsdoc_satisfies_malformed_tag_does_not_apply_later_braced_type() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "types": []
          },
          "files": ["satisfies-malformed.js"]
        }"#,
    );
    write_file(
        &base.join("satisfies-malformed.js"),
        r#"// @ts-check
/** @satisfies nope {{ a: number }} */
const value = { b: 1 };
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<_> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "Expected malformed @satisfies to keep TS1005, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Expected malformed @satisfies name to keep TS2304, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "Malformed @satisfies should not apply later braced type and emit TS2353, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsdoc_arg_aliases_type_checked_js_parameters() {
    for tag in ["arg", "argument"] {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;

        write_file(
            &base.join("tsconfig.json"),
            r#"{
              "compilerOptions": {
                "target": "es2020",
                "allowJs": true,
                "checkJs": true,
                "strict": true,
                "noEmit": true,
                "types": []
              },
              "files": ["index.js"]
            }"#,
        );
        write_file(
            &base.join("index.js"),
            &format!(
                r#"// @ts-check
/**
 * @{tag} {{number}} x
 */
function f(x) {{
  x.toFixed();
  x.toUpperCase();
}}

f("s");
"#
            ),
        );

        let args = default_args();
        let result = compile(&args, base).expect("compile should succeed");
        let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

        assert!(
            !codes.contains(&diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
            "@{tag} should suppress implicit-any for the annotated parameter, got diagnostics: {:?}",
            result.diagnostics
        );
        assert!(
            codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "@{tag} should type x as number and reject toUpperCase, got diagnostics: {:?}",
            result.diagnostics
        );
        assert!(
            codes.contains(
                &diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            ),
            "@{tag} should type the function parameter as number at call sites, got diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn compile_jsdoc_type_reference_to_ambient_value_keeps_construct_signature() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["foo.js"]
        }"#,
    );
    write_file(
        &base.join("foo.js"),
        r#"/** @param {Image} image */
function process(image) {
    return new image(1, 1)
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE),
        "Expected JSDoc type reference to ambient value `Image` to remain constructable in project mode, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsdoc_nested_object_return_types_are_type_checked() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "target": "es2020",
            "module": "commonjs",
            "noEmit": true,
            "types": []
          },
          "files": ["input.js"]
        }"#,
    );
    write_file(
        &base.join("input.js"),
        r#"// @ts-check
/** @returns {{ value: string }} */
function f() {
  return { value: 123 };
}

/**
 * @callback MakeBox
 * @returns {{ value: string }}
 */

/** @type {MakeBox} */
const g = () => ({ value: 123 });
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322.len() >= 2,
        "Expected both plain @returns and @callback nested object returns to report TS2322, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn direct_checker_with_real_default_libs_jsdoc_type_reference_to_ambient_value_keeps_construct_signature()
 {
    let files = vec![(
        "foo.js".to_string(),
        r#"/** @param {Image} image */
function process(image) {
    return new image(1, 1)
}
"#
        .to_string(),
    )];

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(file.source_file);

    assert!(
        checker
            .ctx
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE),
        "Expected direct merged-program checker path to keep ambient `Image` constructable, got diagnostics: {:?}",
        checker.ctx.diagnostics,
    );
}

#[test]
fn compile_jsdoc_arrow_expression_body_preserves_template_scope_for_nested_cast() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["mytest.js"]
        }"#,
    );
    write_file(
        &base.join("mytest.js"),
        r#"/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo1 = value => /** @type {string} */({ ...value });

/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2304: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2304.is_empty(),
        "Expected inline JSDoc nested cast to keep arrow @template scope in project mode, got TS2304 diagnostics: {:?}\nAll diagnostics: {:?}",
        ts2304,
        result.diagnostics
    );
    assert_eq!(
        ts2322.len(),
        2,
        "Expected the two existing TS2322 diagnostics from the cast mismatch shape, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsdoc_template_prefix_tag_does_not_create_type_parameter() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2020",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true,
            "types": []
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check

/**
 * @templatex T
 * @param {T} value
 */
function id(value) {
  return value;
}

id("not a number").toFixed();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        result.diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_NAME
                && d.message_text.contains("Cannot find name 'T'")
        }),
        "Expected @templatex to be ignored so {{T}} reports TS2304, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN),
        "Expected no TS2551 from treating @templatex as a real generic, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_default_import_class_static_enum_object_keeps_enum_members() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "noEmit": true
          },
          "files": ["a.ts", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"enum SomeEnum {
  one,
}
export default class SomeClass {
  public static E = SomeEnum;
}
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import {default as Def} from "./a"
let a = Def.E.one;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected default-imported class static enum object to keep enum members, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn module_augmentation_method_type_params_and_members_resolve_across_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "module": "commonjs",
            "noEmit": true
          },
          "files": ["observable.ts", "map.ts", "main.ts"]
        }"#,
    );
    write_file(
        &base.join("observable.ts"),
        r#"export declare class Observable<T> {
    filter(pred: (e: T) => boolean): Observable<T>;
}
"#,
    );
    write_file(
        &base.join("map.ts"),
        r#"import { Observable } from "./observable";

Observable.prototype.map = function (proj) {
    return this;
}

declare module "./observable" {
    interface Observable<T> {
        map<U>(proj: (e: T) => U): Observable<U>;
    }

    class Bar {}
    const y = 10;
    function z() { }
}
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"import { Observable } from "./observable";
import "./map";

const x = {} as Observable<number>;
x.map(e => e.toFixed());
let before: number;
before.toFixed();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected the real TS2454 to remain, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| {
            d.code != diagnostic_codes::CANNOT_FIND_NAME || !d.message_text.contains("'U'")
        }),
        "Unexpected TS2304 on augmentation method type parameter `U`: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Unexpected TS2339 for augmented `Observable.map`: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| d.code != 7006),
        "Unexpected TS7006 while contextual typing augmented `Observable.map`: {:?}",
        result.diagnostics
    );
}

#[test]
fn nested_reexported_module_augmentation_preserves_original_members() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "noEmit": true
          },
          "files": ["file.ts", "reexport.ts", "augment.ts"]
        }"#,
    );
    write_file(
        &base.join("file.ts"),
        r#"export namespace Root {
    export interface Foo {
        x: number;
    }
}
"#,
    );
    write_file(&base.join("reexport.ts"), r#"export * from "./file";"#);
    write_file(
        &base.join("augment.ts"),
        r#"import * as ns from "./reexport";

declare module "./reexport" {
    export namespace Root {
        export interface Foo {
            self: Foo;
        }
    }
}

declare const f: ns.Root.Foo;

f.x;
f.self;
f.self.x;
f.self.self;
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected re-exported nested module augmentation to preserve original members, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn new_target_uses_enclosing_function_or_constructor_type() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "es2020"
          },
          "files": ["repro.ts"]
        }"#,
    );
    write_file(
        &base.join("repro.ts"),
        r#"function f() {
    const n: number = new.target;
}

class C {
    constructor() {
        const s: string = new.target;
    }
}
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        2,
        "Expected TS2322 for function and constructor new.target assignments, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn labeled_tuple_optional_marker_after_type_reports_ts5086() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "type T = [a: string?];\n");

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::A_LABELED_TUPLE_ELEMENT_IS_DECLARED_AS_OPTIONAL_WITH_A_QUESTION_MARK_AFTER_THE_N
        }),
        "Expected TS5086 for optional marker after labeled tuple type, got {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|diag| {
            diag.code
                != diagnostic_codes::AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE
        }),
        "Expected no TS17019 JSDoc nullable diagnostic, got {:?}",
        result.diagnostics
    );
}

// TS18003 should be emitted alongside TS5110 when no input files are found
// and module/moduleResolution are incompatible.
#[test]
fn ts18003_emitted_alongside_ts5110_when_no_inputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a tsconfig with incompatible module/moduleResolution and no source files
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          }
        }"#,
    );
    // No .ts files — should trigger TS18003

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for incompatible module/moduleResolution, got: {codes:?}"
    );
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 when no input files found alongside TS5110, got: {codes:?}"
    );
}

// TS18003 should NOT be emitted alongside TS5110 when input files exist
#[test]
fn ts18003_not_emitted_when_inputs_exist_with_ts5110() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        !codes.contains(&18003),
        "Should NOT emit TS18003 when input files exist, got: {codes:?}"
    );
}

#[test]
fn ts5090_stops_before_follow_on_module_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "paths": {
              "@app/*": ["src/*"]
            }
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "import 'someModule';\n");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD
        ),
        "Should emit TS5090 for non-relative paths mapping without baseUrl, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            && !codes.contains(
                &diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF
            ),
        "Should stop before follow-on module diagnostics when TS5090 is present, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_implicit_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "nodenext",
            "allowJs": true
          }
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    // tsc's default include `["**/*"]` discovers .mts files, so with an .mts
    // present the project has inputs and TS18003 must NOT be emitted.
    // TS5110 is still expected from the module/moduleResolution mismatch.
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for module/moduleResolution mismatch, got: {codes:?}"
    );
    assert!(
        !codes.contains(&18003),
        "Should NOT emit TS18003 when .mts is discovered via implicit include, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_explicit_default_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "node16",
            "allowJs": true
          },
          "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 for explicit default include with only .mts input, got: {codes:?}"
    );
}

// TS6059: File not under rootDir should produce diagnostic
#[test]
fn ts6059_file_not_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a rootDir of "src" but put a file outside it
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("outside.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6059),
        "Should emit TS6059 for file outside rootDir, got: {codes:?}"
    );
}

// TS6059 should NOT be emitted when all files are under rootDir
#[test]
fn ts6059_not_emitted_when_all_files_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("src/utils.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6059),
        "Should NOT emit TS6059 when all files are under rootDir, got: {codes:?}"
    );
}

#[test]
fn phase_timings_are_populated_after_compilation() {
    let dir = TempDir::new().unwrap();
    let base = &dir.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "include": ["*.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 42;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let pt = &result.phase_timings;

    // All phase timings should be non-negative
    assert!(pt.io_read_ms >= 0.0, "io_read_ms should be non-negative");
    assert!(
        pt.load_libs_ms >= 0.0,
        "load_libs_ms should be non-negative"
    );
    assert!(
        pt.parse_bind_ms >= 0.0,
        "parse_bind_ms should be non-negative"
    );
    assert!(pt.check_ms >= 0.0, "check_ms should be non-negative");
    assert!(pt.emit_ms >= 0.0, "emit_ms should be non-negative");
    assert!(pt.total_ms > 0.0, "total_ms should be positive");
    // T0.2 sub-phase buckets: structurally present, default 0.0 until
    // the driver attributes work to them. Non-negative is the only
    // invariant they must satisfy today.
    assert!(
        pt.config_discovery_ms >= 0.0,
        "config_discovery_ms should be non-negative"
    );
    assert!(
        pt.source_discovery_ms >= 0.0,
        "source_discovery_ms should be non-negative"
    );
    assert!(
        pt.module_resolution_ms >= 0.0,
        "module_resolution_ms should be non-negative"
    );

    // Total should be >= sum of individual phases (wall-clock includes overhead).
    // Sub-phase buckets are subsets of the existing top-level buckets they
    // came out of (config/source/module-resolution land inside io_read; the
    // driver moves them up rather than creating new wall time), so we don't
    // double-count them here.
    let sum = pt.io_read_ms + pt.load_libs_ms + pt.parse_bind_ms + pt.check_ms + pt.emit_ms;
    assert!(
        pt.total_ms >= sum * 0.9, // allow small floating-point margin
        "total_ms ({}) should be >= sum of phases ({})",
        pt.total_ms,
        sum
    );
}

#[test]
fn compile_reports_outer_ts2345_for_block_body_contextual_callback_return_mismatch() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "es2015"
          },
          "include": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
interface Collection<T, U> {
    length: number;
    add(x: T, y: U): void;
    remove(x: T, y: U): boolean;
}

interface Combinators {
    map<T, U>(c: Collection<T, U>, f: (x: T, y: U) => any): Collection<any, any>;
    map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V): Collection<T, V>;
}

declare var _: Combinators;
declare var c2: Collection<number, string>;
var r5a = _.map<number, string, Date>(c2, (x, y) => { return x.toFixed() });
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2345),
        "Expected outer TS2345 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2322),
        "Expected no inner TS2322 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
}

