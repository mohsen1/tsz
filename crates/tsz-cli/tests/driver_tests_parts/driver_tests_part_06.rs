#[test]
fn namespace_import_alias_const_enum_member_condition_reports_ts2845() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "declaration": true
  },
  "files": ["internal.ts", "usage.ts"]
}"#,
    );
    write_file(
        &base.join("internal.ts"),
        r#"namespace My.Internal {
    export function getThing(): void {}
    export const enum WhichThing {
        A, B, C
    }
}
"#,
    );
    write_file(
        &base.join("usage.ts"),
        r#"/// <reference path="./internal.ts" preserve="true" />
namespace SomeOther.Thing {
    import Internal = My.Internal;
    export class Foo {
        private _which!: Internal.WhichThing;
        constructor() {
            Internal.getThing();
            Internal.WhichThing.A ? "foo" : "bar";
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2845_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2845)
        .collect();
    assert!(
        ts2845_diags
            .iter()
            .any(|diag| diag.message_text.contains("always return 'false'")),
        "Expected TS2845 for namespace-imported const enum member condition, got: {result:?}"
    );
}
#[test]
fn export_import_qualified_type_only_namespace_reports_ts2708() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["test.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"namespace x {
    interface c {
    }
}
export import a = x.c;
export = x;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2708_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .collect();
    assert!(
        ts2708_diags.iter().any(|diag| diag
            .message_text
            .contains("Cannot use namespace 'x' as a value")),
        "Expected TS2708 on the namespace qualifier in export import, got: {result:?}"
    );
}
#[test]
fn export_import_namespace_type_alias_without_export_equals_does_not_report_ts2708() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["test.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"export namespace a {
    export interface I {
    }
}

export import b = a.I;
export declare const x: b;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|d| d.code == 2708),
        "Did not expect TS2708 for namespace type alias without export=, got: {result:?}"
    );
}
#[test]
fn lib_replacement_honors_source_reference_subfiles() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@typescript/lib-dom/index.d.ts"),
        "// NOOP\n",
    );
    write_file(
        &base.join("node_modules/@typescript/lib-dom/iterable.d.ts"),
        "interface DOMIterable { abc: string }\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "libReplacement": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"/// <reference lib="dom.iterable" />
const a: DOMIterable = { abc: "Hello" };

window.localStorage;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        ts2552_diags.is_empty(),
        "Expected replacement dom.iterable lib to provide DOMIterable, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert_eq!(
        ts2304_diags.len(),
        1,
        "Expected only the replaced-out window global to fail, got: {result:?}"
    );
    assert!(
        ts2304_diags[0].message_text.contains("window"),
        "Expected TS2304 to target window, got: {result:?}"
    );
}
#[test]
fn types_entry_resolves_direct_declaration_file_from_type_root() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/phaser/types/phaser.d.ts"),
        "declare const a: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "typeRoots": ["node_modules/phaser/types"],
            "types": ["phaser"]
          },
          "files": ["a.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "a;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        ts2688_diags.is_empty(),
        "Expected direct declaration file under typeRoots to satisfy the types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected declarations from direct typeRoots file to be visible, got: {result:?}"
    );
}
#[test]
fn import_from_type_package_loaded_via_types_does_not_emit_ts2307() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("typings/phaser/types/phaser.d.ts"),
        "export const a2: number;\n",
    );
    write_file(
        &base.join("typings/phaser/package.json"),
        r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "typeRoots": ["typings"],
            "types": ["phaser"]
          },
          "files": ["a.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), r#"import { a2 } from "phaser";"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2307_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .collect();
    assert!(
        ts2307_diags.is_empty(),
        "Expected type package imports satisfied via types/typeRoots to avoid TS2307, got: {result:?}"
    );
}
#[test]
fn ts2307_emitted_for_commonjs_module() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "commonjs" }, "files": ["test.ts"] }"#,
    );
    write_file(
        &base.join("test.ts"),
        "import { thing } from \"non-existent-module\";\nthing();\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Should emit TS2307 (not TS2792) for CommonJS module kind
    let ts2307 = result.diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
    });
    assert!(
        ts2307,
        "Expected TS2307 for bare specifier with module: commonjs, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}
#[test]
fn ts1079_emitted_for_declare_import_without_ts2304_on_declare() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015" }, "files": ["test.ts"] }"#,
    );
    write_file(&base.join("test.ts"), "declare import a = b;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1079_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION
        })
        .collect();
    assert!(
        !ts1079_diags.is_empty(),
        "Expected TS1079 for `declare import`, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts1079_diags
            .iter()
            .any(|diag| diag.message_text.contains("'declare'")),
        "Expected TS1079 message to mention the declare modifier, got: {ts1079_diags:?}"
    );

    let declare_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("declare")
        })
        .collect();
    assert!(
        declare_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `declare`: {declare_ts2304_diags:?}"
    );
}
#[test]
fn ts2592_emitted_for_unresolved_jquery_global_without_ts2304() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015", "lib": ["es5"] }, "files": ["test.ts"] }"#,
    );
    write_file(&base.join("test.ts"), "const value = $(\".thing\");\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2592_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2
        })
        .collect();
    assert!(
        !ts2592_diags.is_empty(),
        "Expected TS2592 for unresolved jQuery global `$`, got diagnostics: {:?}",
        result.diagnostics
    );

    let jquery_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("'$'"))
        .collect();
    assert!(
        jquery_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `$`: {jquery_ts2304_diags:?}"
    );
}
#[test]
fn ts2552_emitted_for_type_only_export_typo() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015", "strict": true, "module": "commonjs" }, "files": ["test.ts"] }"#,
    );
    write_file(
        &base.join("test.ts"),
        "type RoomInterfae = {};\n\nexport type {\n    RoomInterface\n}\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        !ts2552_diags.is_empty(),
        "Expected TS2552 for the typo in `export type {{ RoomInterface }}`, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts2552_diags
            .iter()
            .any(|diag| diag.message_text.contains("RoomInterfae")),
        "Expected TS2552 to suggest `RoomInterfae`, got: {ts2552_diags:?}"
    );

    let room_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("RoomInterface")
        })
        .collect();
    assert!(
        room_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `RoomInterface`: {room_ts2304_diags:?}"
    );
}
#[test]
fn js_export_type_skips_follow_on_local_named_export_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "allowJs": true, "checkJs": true, "target": "es2015", "module": "commonjs" }, "files": ["index.js", "a.d.ts"] }"#,
    );
    write_file(&base.join("a.d.ts"), "export default interface A {}\n");
    write_file(
        &base.join("index.js"),
        "import type A from \"./a\";\nexport type { A };\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|d| {
            matches!(
                d.code,
                diagnostic_codes::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES
                    | diagnostic_codes::CANNOT_EXPORT_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE
                    | diagnostic_codes::CANNOT_FIND_NAME
            )
        }),
        "Did not expect TS18043/TS2661/TS2304 follow-on diagnostics for `export type` in JS, got diagnostics: {:?}",
        result.diagnostics
    );
}

/// TS8002: `export import x = require(...)` in a JS file should report at the
/// `export` keyword (position 0), not the inner `import` keyword.
#[test]
fn ts8002_export_import_equals_reports_at_export_keyword_in_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "nodenext", "allowJs": true, "checkJs": true }, "files": ["index.js"] }"#,
    );
    // `export import` starts at position 0; the inner `import` starts at position 7
    write_file(
        &base.join("index.js"),
        "export import fs2 = require(\"fs\");\n",
    );
    write_file(&base.join("package.json"), r#"{ "type": "module" }"#);

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts8002_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES)
        .collect();

    assert!(
        !ts8002_diags.is_empty(),
        "Expected TS8002 for `export import` in JS file, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );

    // The error should start at position 0 (the `export` keyword), not position 7 (`import`)
    for d in &ts8002_diags {
        assert_eq!(
            d.start, 0,
            "TS8002 should report at `export` keyword (pos 0), not inner `import` (pos 7). Got start={}",
            d.start
        );
    }
}

/// TS2303: `import x = require(...)` in a JS file should NOT produce a
/// "Circular definition of import alias" error — tsc skips semantic analysis
/// for TS-only syntax in JS files.
#[test]
fn ts2303_not_emitted_for_import_equals_in_js_file() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "nodenext", "allowJs": true, "checkJs": true }, "files": ["index.js"] }"#,
    );
    // Self-referencing import = require: would normally trigger TS2303 circular check
    write_file(
        &base.join("index.js"),
        "import mod = require(\"./index.js\");\nmod;\n",
    );
    write_file(&base.join("package.json"), r#"{ "type": "module" }"#);

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts2303_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS)
        .collect();

    assert!(
        ts2303_diags.is_empty(),
        "TS2303 should not be emitted for `import = require()` in JS files (TS-only syntax). Got: {:?}",
        ts2303_diags
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // TS8002 SHOULD still be emitted though
    let has_ts8002 = result
        .diagnostics
        .iter()
        .any(|d| d.code == diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES);
    assert!(
        has_ts8002,
        "TS8002 should still be emitted for `import = require()` in JS file"
    );
}
#[test]
fn ts5107_not_suppressed_by_jsdoc_param_name_validation() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": false,
            "alwaysStrict": false,
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"/**
 * @param {object} obj
 * @param {string} obj.a
 * @param {string} obj.b
 * @param {string} x
 */
function bad1(x, {a, b}) {}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 5107),
        "Expected TS5107 for alwaysStrict=false, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| {
            d.code
                != diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME
        }),
        "Did not expect TS8024 alongside TS5107, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Did not expect follow-on TS2339 alongside TS5107, got diagnostics: {:?}",
        result.diagnostics
    );
}
#[test]
fn ts5107_es5_target_suppresses_accessor_call_follow_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"class Test24554 {
    get property(): number { return 1; }
}
function test24554(x: Test24554) {
    return x.property();
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5107),
        "Expected TS5107 for deprecated ES5 target, got: {codes:?}"
    );
    assert!(
        !codes.contains(&6234),
        "Did not expect TS6234 alongside deprecated ES5 target, got: {codes:?}"
    );
}
#[test]
fn ts5107_suppresses_arrow_line_terminator_follow_on_errors() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": false,
            "alwaysStrict": false,
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"var f = ()
    => { }
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![5107],
        "Expected only TS5107 for deprecated strict expansion, got: {:#?}",
        result.diagnostics
    );
}
#[test]
fn json_default_bindings_with_import_assertions_do_not_emit_ts2305() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "module": "esnext",
            "ignoreDeprecations": "6.0",
            "noEmit": true
          },
          "files": ["a.ts", "c.ts", "consumer.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"import { default as pkg } from "./package.json" assert { type: "json" };
export const pkgValue = pkg;
"#,
    );
    write_file(
        &base.join("c.ts"),
        r#"export { default as config } from "./config.json" assert { type: "json" };
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"import { config } from "./c";

const exact: { answer: number } = config;
void exact;
"#,
    );
    write_file(&base.join("package.json"), r#"{ "name": "tsz" }"#);
    write_file(&base.join("config.json"), r#"{ "answer": 1 }"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|d| d.code != 2305),
        "Did not expect TS2305 for JSON default import/re-export bindings, got diagnostics: {:#?}",
        result.diagnostics
    );
}
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
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
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

    // Total should be >= sum of individual phases (wall-clock includes overhead)
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
