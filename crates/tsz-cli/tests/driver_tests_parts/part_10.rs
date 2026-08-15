// `checked_js_direct_file_jsdoc_import_string_literal_export_names_resolve`
// moved to `part_15.rs` to keep this shard under the 2000-line arch-size budget.

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
fn global_nan_equality_condition_reports_ts2845_in_project_compile() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "noEmit": true
  },
  "files": ["test.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"declare const x: number;

if (x === NaN) {}
if (NaN !== x) {}

function t1(value: number, NaN: number) {
    return value === NaN;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2845_count = result.diagnostics.iter().filter(|d| d.code == 2845).count();

    assert_eq!(
        ts2845_count, 2,
        "Expected global NaN conditions, but not the shadowed parameter, to report TS2845; got: {result:?}"
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
fn script_empty_html_element_interface_augments_dom_global() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "interface HTMLElement {}\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "script HTMLElement interface should augment lib.dom without diagnostics, got: {result:?}"
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
fn compile_resolves_commonjs_require_with_whitespace_before_paren() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": [
            "main-space.js",
            "main-tight-control.js"
          ]
        }"#,
    );
    write_file(
        &base.join("main-space.js"),
        r#"// @ts-check

const depSpace = require ("./dep-space");

depSpace.value.toUpperCase();
"#,
    );
    write_file(
        &base.join("dep-space.js"),
        r#"// @ts-check

exports.value = 1;
"#,
    );
    write_file(
        &base.join("main-tight-control.js"),
        r#"// @ts-check

const depTight = require("./dep-tight");

depTight.value.toUpperCase();
"#,
    );
    write_file(
        &base.join("dep-tight.js"),
        r#"// @ts-check

exports.value = 1;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let missing_property_files: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|diag| diag.file.as_str())
        .collect();

    assert!(
        missing_property_files
            .iter()
            .any(|file| file.contains("main-space.js")),
        "Expected spaced require dependency to produce TS2339 in main-space.js, got: {result:?}"
    );
    assert!(
        missing_property_files
            .iter()
            .any(|file| file.contains("main-tight-control.js")),
        "Expected tight require control to produce TS2339 in main-tight-control.js, got: {result:?}"
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
fn missing_external_globals_without_types_use_types_field_diagnostics() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"process.cwd();
$("body");
describe("suite", () => {});
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    for code in [
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2,
    ] {
        assert!(
            codes.contains(&code),
            "Expected missing external global diagnostic {code}, got diagnostics: {:?}",
            result.diagnostics
        );
    }
    for code in [
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
    ] {
        assert!(
            !codes.contains(&code),
            "Did not expect install-only diagnostic {code}, got diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn missing_external_globals_with_types_use_types_field_diagnostics() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true,
            "types": []
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"process.cwd();
$("body");
describe("suite", () => {});
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    for code in [
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2,
    ] {
        assert!(
            codes.contains(&code),
            "Expected types-field diagnostic {code}, got diagnostics: {:?}",
            result.diagnostics
        );
    }
    for code in [
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA,
        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
    ] {
        assert!(
            !codes.contains(&code),
            "Did not expect install-only diagnostic {code}, got diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn checked_js_node_globals_match_tsc_scope() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "module": "commonjs",
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check
process.cwd();
require("x");
module.exports = {};
__dirname.toUpperCase();
__filename.toUpperCase();
Buffer.from("x");
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    let ts2591 = diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2;

    assert_eq!(
        codes.iter().filter(|&&code| code == ts2591).count(),
        2,
        "Expected TS2591 for process and Buffer in checked JS, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 for unresolved require(\"x\"), got diagnostics: {:?}",
        result.diagnostics
    );
    for name in ["__dirname", "__filename"] {
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME
                    && d.message_text.contains(name)),
            "Expected TS2304 for {name}, got diagnostics: {:?}",
            result.diagnostics
        );
    }
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| !(d.code == diagnostic_codes::CANNOT_FIND_NAME
                && d.message_text.contains("'module'"))),
        "Did not expect TS2304 for module.exports in checked JS, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_esm_commonjs_globals_require_node_types() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("repro.js"),
        r#"export {};
module.exports = {};
require;
"#,
    );

    let mut args = default_args();
    args.no_emit = true;
    args.allow_js = true;
    args.check_js = true;
    args.module = Some(crate::args::Module::EsNext);
    args.files = vec![PathBuf::from("repro.js")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2591 = diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2;

    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|d| d.code == ts2591)
            .count(),
        2,
        "Expected TS2591 for module and require in checked JS ESM, got diagnostics: {:?}",
        result.diagnostics
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
fn cli_removed_target_value_emits_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--target",
        "es5",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 for direct --target es5, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_removed_always_strict_false_emits_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--alwaysStrict",
        "false",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 for direct --alwaysStrict false, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_removed_allow_synthetic_default_imports_false_emits_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--allowSyntheticDefaultImports",
        "false",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 for direct --allowSyntheticDefaultImports false, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_allow_umd_global_access_suppresses_module_global_ts2686() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(
        &base.join("lib.d.ts"),
        r#"export as namespace UmdLib;
export function run(): void;
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"import "./lib";

export {};

UmdLib.run();
"#,
    );

    let without_flag = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--strict",
        "--target",
        "es2020",
        "--module",
        "esnext",
        "--noEmit",
        "--pretty",
        "false",
        "main.ts",
        "lib.d.ts",
    ])
    .expect("CLI args should parse");
    let without_flag_result = compile(&without_flag, base).expect("compile should succeed");
    assert!(
        without_flag_result.diagnostics.iter().any(|d| {
            d.code
                == diagnostic_codes::REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPOR
        }),
        "Expected TS2686 without --allowUmdGlobalAccess, got: {:#?}",
        without_flag_result.diagnostics
    );

    let with_flag = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--strict",
        "--target",
        "es2020",
        "--module",
        "esnext",
        "--allowUmdGlobalAccess",
        "--noEmit",
        "--pretty",
        "false",
        "main.ts",
        "lib.d.ts",
    ])
    .expect("CLI args should parse");
    let with_flag_result = compile(&with_flag, base).expect("compile should succeed");
    assert!(
        with_flag_result.diagnostics.is_empty(),
        "Expected --allowUmdGlobalAccess to suppress TS2686, got: {:#?}",
        with_flag_result.diagnostics
    );
}

#[test]
fn cli_ignore_deprecations_does_not_suppress_direct_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--alwaysStrict",
        "false",
        "--ignoreDeprecations",
        "6.0",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 with direct --ignoreDeprecations 6.0, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn config_ignore_deprecations_does_not_suppress_direct_cli_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "ignoreDeprecations": "6.0",
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );

    let args = CliArgs::try_parse_from(["tsz", "--pretty", "false", "--target", "es5"])
        .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 with config ignoreDeprecations 6.0, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_ignore_deprecations_does_not_suppress_config_ts5108() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );

    let args = CliArgs::try_parse_from(["tsz", "--pretty", "false", "--ignoreDeprecations", "6.0"])
        .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5108),
        "Expected TS5108 with CLI --ignoreDeprecations 6.0, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn config_target_es3_emits_ts6046() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "es3",
            "ignoreDeprecations": "6.0"
          },
          "files": ["main.ts"]
        }"#,
    );

    let args =
        CliArgs::try_parse_from(["tsz", "--pretty", "false"]).expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE],
        "Expected only TS6046 for target=ES3, got: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics[0]
            .message_text
            .contains("Argument for '--target' option must be"),
        "Unexpected TS6046 message: {}",
        result.diagnostics[0].message_text
    );
}

#[test]
fn config_target_es3_keeps_source_semantic_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "let x: string = 1;\n");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "target": "es3"
          },
          "files": ["main.ts"]
        }"#,
    );

    let args =
        CliArgs::try_parse_from(["tsz", "--pretty", "false"]).expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE),
        "Expected config TS6046, got: {:#?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Config TS6046 must coexist with source TS2322, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_target_es3_emits_ts6046() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "let x: string = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--target",
        "ES3",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("--target ES3 should parse so config validation can report TS6046");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![diagnostic_codes::ARGUMENT_FOR_OPTION_MUST_BE],
        "Expected only TS6046 for direct --target ES3, got: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics[0]
            .message_text
            .contains("Argument for '--target' option must be"),
        "Unexpected TS6046 message: {}",
        result.diagnostics[0].message_text
    );
}

#[test]
fn cli_ts7_dropped_compiler_option_flags_emit_ts5023() {
    let cases: &[(&[&str], &str)] = &[
        (&["--noImplicitUseStrict"], "noImplicitUseStrict"),
        (&["--keyofStringsOnly"], "keyofStringsOnly"),
        (&["--charset", "utf8"], "charset"),
        (
            &["--suppressExcessPropertyErrors"],
            "suppressExcessPropertyErrors",
        ),
        (
            &["--suppressImplicitAnyIndexErrors"],
            "suppressImplicitAnyIndexErrors",
        ),
        (
            &["--importsNotUsedAsValues", "error"],
            "importsNotUsedAsValues",
        ),
        (&["--preserveValueImports"], "preserveValueImports"),
        (&["--noStrictGenericChecks"], "noStrictGenericChecks"),
    ];

    for (flag_args, option_name) in cases {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;
        write_file(&base.join("main.ts"), "const ok = 1;\n");
        std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

        let mut argv: Vec<&str> = vec![
            "tsz",
            "--noEmit",
            "--pretty",
            "false",
            "--typeRoots",
            "./empty-types",
            "main.ts",
        ];
        argv.extend_from_slice(flag_args);
        let args = CliArgs::try_parse_from(argv)
            .unwrap_or_else(|err| panic!("CLI args should parse for {flag_args:?}: {err}"));
        let result = compile(&args, base).expect("compile should succeed");
        let unknown_diags: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION)
            .collect();
        assert!(
            !unknown_diags.is_empty(),
            "expected TS5023 for dropped flag {flag_args:?}, got: {:#?}",
            result.diagnostics
        );
        assert!(
            unknown_diags
                .iter()
                .any(|d| d.message_text.contains(option_name)),
            "TS5023 message must mention {option_name:?}, got: {unknown_diags:#?}"
        );
    }
}

#[test]
fn cli_ts7_dropped_compiler_option_flags_do_not_block_emit() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "export const ok: number = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--pretty",
        "false",
        "--typeRoots",
        "./empty-types",
        "--importsNotUsedAsValues",
        "preserve",
        "--preserveValueImports",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::UNKNOWN_COMPILER_OPTION),
        "expected TS5023 for dropped CLI flags, got: {:#?}",
        result.diagnostics
    );
    assert!(
        base.join("main.js").exists() && !result.emitted_files.is_empty(),
        "TS5023 should not stop emit without noEmitOnError: {result:#?}"
    );

    std::fs::remove_file(base.join("main.js")).expect("remove first emitted output");
    let no_emit_on_error_args = CliArgs::try_parse_from([
        "tsz",
        "--pretty",
        "false",
        "--typeRoots",
        "./empty-types",
        "--noEmitOnError",
        "--preserveValueImports",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let no_emit_on_error_result =
        compile(&no_emit_on_error_args, base).expect("compile should succeed");
    assert!(
        !base.join("main.js").exists() && no_emit_on_error_result.emitted_files.is_empty(),
        "TS5023 must honor noEmitOnError: {no_emit_on_error_result:#?}"
    );
}

#[test]
fn cli_parse_diagnostics_outrank_later_option_config_and_source_diagnostics() {
    let cases: &[(&[&str], &[u32])] = &[
        (&["--keyofStringsOnly", "--target", "ES5"], &[5023]),
        (&["--keyofStringsOnly", "--baseUrl", "."], &[5023]),
        (&["--target", "ES3", "--module", "AMD"], &[6046]),
        (&["--target", "ES3", "--keyofStringsOnly"], &[5023, 6046]),
        (&["--keyofStringsOnly", "--paths", "foo"], &[5023, 6064]),
        (&["--target", "ES3", "--paths", "foo"], &[6046, 6064]),
    ];

    for (flag_args, expected_codes) in cases {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;
        write_file(&base.join("main.ts"), "const n: number = \"s\";\n");
        std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

        let mut argv = vec![
            "tsz",
            "--noEmit",
            "--pretty",
            "false",
            "--typeRoots",
            "./empty-types",
        ];
        argv.extend_from_slice(flag_args);
        argv.push("main.ts");
        let args = CliArgs::try_parse_from(argv)
            .unwrap_or_else(|err| panic!("CLI args should parse for {flag_args:?}: {err}"));
        let result = compile(&args, base).expect("compile should succeed");
        let mut codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();
        codes.sort_unstable();

        assert_eq!(
            codes.as_slice(),
            *expected_codes,
            "direct parse diagnostics must outrank later option and source diagnostics for {flag_args:?}: {:#?}",
            result.diagnostics
        );
    }

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const n: number = \"s\";\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "baseUrl": ".",
            "noEmit": true,
            "typeRoots": ["./empty-types"]
          },
          "files": ["main.ts"]
        }"#,
    );

    let args = CliArgs::try_parse_from(["tsz", "--pretty", "false", "--keyofStringsOnly"])
        .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();
    assert_eq!(
        codes,
        vec![5023],
        "direct TS5023 must suppress config TS5102 and source TS2322: {:#?}",
        result.diagnostics
    );

    let temp = TempDir::new().expect("temp dir");
    let args = CliArgs::try_parse_from(["tsz", "--pretty", "false", "--keyofStringsOnly"])
        .expect("CLI args should parse");
    let result = compile(&args, &temp.path).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();
    assert_eq!(
        codes,
        vec![5023],
        "direct TS5023 must also outrank no-input TS18003: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_unrecognized_ignore_deprecations_does_not_emit_ts5103() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--ignoreDeprecations",
        "8.0",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    // The direct `--ignoreDeprecations` CLI flag routes through the same
    // tsconfig-shaped validator as the config path, so it inherits the
    // no-value-validation rule (#16228). Source checking must also still run:
    // TS5103 used to be classified as a fatal config diagnostic that stopped
    // the compile before the program was checked.
    assert!(
        !codes.contains(&5103),
        "Expected no TS5103 for direct --ignoreDeprecations, got: {:#?}",
        result.diagnostics
    );
    assert!(
        codes.is_empty(),
        "An unrecognized --ignoreDeprecations must not stop or perturb the \
         compile of a clean program, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cli_valid_ignore_deprecations_7_0_does_not_emit_ts5103() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("main.ts"), "const ok = 1;\n");
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--ignoreDeprecations",
        "7.0",
        "--typeRoots",
        "./empty-types",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&5103),
        "tsc 7.0.2 accepts --ignoreDeprecations 7.0 (#16217), got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn ts5108_not_suppressed_by_jsdoc_param_name_validation() {
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
        result.diagnostics.iter().any(|d| d.code == 5108),
        "Expected TS5108 for alwaysStrict=false, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| {
            d.code
                != diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME
        }),
        "Did not expect TS8024 alongside TS5108, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Did not expect follow-on TS2339 alongside TS5108, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn ts5108_es5_target_suppresses_accessor_call_follow_on_error() {
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
        codes.contains(&5108),
        "Expected TS5108 for removed ES5 target, got: {codes:?}"
    );
    assert!(
        !codes.contains(&6234),
        "Did not expect TS6234 alongside removed ES5 target, got: {codes:?}"
    );
}

#[test]
fn ts5108_suppresses_arrow_line_terminator_follow_on_errors() {
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
        vec![5108],
        "Expected only TS5108 for removed strict expansion, got: {:#?}",
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
fn ignore_config_explicit_file_mode_implies_resolve_json_module_for_bundler() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("index.ts"),
        r#"import data from "./data.json";
const answer: number = data.answer;
void answer;
"#,
    );
    write_file(&base.join("data.json"), r#"{ "answer": 42 }"#);

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--pretty",
        "false",
        "index.ts",
    ]);
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no TS2732 for JSON import in no-config explicit-file mode, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn cts_json_namespace_import_default_property_is_json_object() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "moduleResolution": "node16",
            "resolveJsonModule": true,
            "noEmit": true
          },
          "files": ["index.cts"]
        }"#,
    );
    write_file(
        &base.join("index.cts"),
        r#"import * as pkg from "./package.json";

export const name = pkg.default.name;
"#,
    );
    write_file(
        &base.join("package.json"),
        r#"{ "name": "pkg", "default": "misedirection" }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().all(|d| d.code != 2339),
        "Did not expect TS2339 for JSON namespace default property, got diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn resolve_json_module_does_not_make_included_json_files_roots() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "moduleResolution": "node16",
            "resolveJsonModule": true,
            "types": [],
            "noEmit": true
          },
          "include": ["**/*"]
        }"#,
    );
    write_file(&base.join("app.ts"), "export const x = 1;\n");
    write_file(&base.join("data.json"), "{ not valid json }\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "unimported JSON matched by include should not be parsed as a root: {:#?}",
        result.diagnostics
    );
}

#[test]
fn property_diagnostic_does_not_use_conformance_fingerprint_rewrite() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true
          },
          "files": ["repro.ts"]
        }"#,
    );
    write_file(
        &base.join("repro.ts"),
        r#"
type A = { c: number };
type constr<Source, Tgt> = Source & Tgt;
declare const q: { [key: string]: A };
q["asd"].b;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "expected exactly one TS2339, got diagnostics: {:#?}",
        result.diagnostics
    );
    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'A'"),
        "TS2339 should preserve the user alias receiver, got: {message}"
    );
    assert!(
        !message.contains("{ a: string; }"),
        "TS2339 must not use the conformance fingerprint display, got: {message}"
    );
}

// A TS7-dropped option is unknown and stops before semantic diagnostics.
#[test]
fn cli_removed_option_suppresses_semantic_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    // `main.ts` has a genuine TS2322 that tsc suppresses behind the TS5102 notice.
    write_file(&base.join("main.ts"), "const n: number = \"s\";\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--keyofStringsOnly",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5023),
        "expected unknown-option TS5023, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2322),
        "unknown-option TS5023 must suppress semantic TS2322, got: {:#?}",
        result.diagnostics
    );
}

// TS7 rejects the dropped option before parsing source grammar.
#[test]
fn cli_removed_option_grammar_error_takes_precedence() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(&base.join("bad.ts"), "const x = ;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        "--keyofStringsOnly",
        "bad.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5023),
        "expected unknown-option TS5023, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&1109),
        "TS5023 must stop before the grammar error, got: {:#?}",
        result.diagnostics
    );
}
