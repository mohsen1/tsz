#[test]
fn cli_validates_direct_option_conflicts_and_dependencies() {
    assert_cli_option_validation_reports(
        &[
            "tsz",
            "--noEmit",
            "--emitDecoratorMetadata",
            "--pretty",
            "false",
            "--ignoreConfig",
            "decorator.ts",
        ],
        "decorator.ts",
        r#"
class C {
  @d
  m() {}
}
declare const d: MethodDecorator;
"#,
        diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION,
    );

    assert_cli_option_validation_reports(
        &[
            "tsz",
            "--declaration",
            "--emitDeclarationOnly",
            "--allowJs",
            "--isolatedDeclarations",
            "--pretty",
            "false",
            "--ignoreConfig",
            "a.js",
        ],
        "a.js",
        "export const x = 1;\n",
        diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITH_OPTION,
    );

    assert_cli_option_validation_reports(
        &[
            "tsz",
            "--noEmit",
            "--module",
            "amd",
            "--ignoreDeprecations",
            "6.0",
            "--verbatimModuleSyntax",
            "--pretty",
            "false",
            "--ignoreConfig",
            "plain.ts",
        ],
        "plain.ts",
        "const x = 1;\n",
        diagnostic_codes::OPTION_VERBATIMMODULESYNTAX_CANNOT_BE_USED_WHEN_MODULE_IS_SET_TO_UMD_AMD_OR_SYST,
    );

    assert_cli_option_validation_reports(
        &[
            "tsz",
            "--noEmit",
            "--declarationMap",
            "--pretty",
            "false",
            "--ignoreConfig",
            "a.ts",
        ],
        "a.ts",
        "export const x = 1;\n",
        diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION,
    );
}

#[test]
fn cli_rejects_tsconfig_only_options_on_command_line() {
    for (flag, value) in [("--paths", "@/*=src/*"), ("--plugins", "foo")] {
        assert_cli_option_validation_reports(
            &[
                "tsz",
                flag,
                value,
                "--noEmit",
                "--pretty",
                "false",
                "--ignoreConfig",
                "index.ts",
            ],
            "index.ts",
            "export {};\n",
            diagnostic_codes::OPTION_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LIN,
        );
    }
}

#[test]
fn source_file_test_pragmas_do_not_override_project_options() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "strictNullChecks": true,
            "allowJs": true,
            "checkJs": true,
            "noUnusedLocals": true,
            "types": ["ambient"]
          },
          "files": ["strict-off.js", "unused-off.ts", "no-types-off.ts"]
        }"#,
    );
    write_file(
        &base.join("node_modules/@types/ambient/index.d.ts"),
        "declare const injectedFromTypes: number;\n",
    );
    write_file(
        &base.join("strict-off.js"),
        r#"// @strict: false
function takesAny(value) {
  return value;
}

takesAny(1);
"#,
    );
    write_file(
        &base.join("unused-off.ts"),
        r#"// @noUnusedLocals: false
export {};

const unused = 1;
"#,
    );
    write_file(
        &base.join("no-types-off.ts"),
        r#"// @noTypesAndSymbols: true

injectedFromTypes.toFixed();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&7006),
        "source @strict pragma should not suppress project strict/noImplicitAny, got: {codes:?}"
    );
    assert!(
        codes.contains(&6133),
        "source @noUnusedLocals pragma should not suppress project noUnusedLocals, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2304),
        "source @noTypesAndSymbols pragma must not suppress tsconfig `types` resolution, got: {codes:?}"
    );
}

#[test]
fn global_this_type_env_prewarm_does_not_suppress_in_operator_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "target": "es2015"
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"
function unknownCase(x: unknown) {
  if ("a" in x) {
    x.a;
  }
  if (x && "b" in x) {
    x.b;
  }
}

function genericCase<T>(x: T) {
  if ("a" in x) {
    x.a;
  }
  if (x && "b" in x) {
    x.b;
  }
}

function globalThisCase(x: typeof globalThis, y: Window & typeof globalThis) {
  x = y;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::IS_OF_TYPE_UNKNOWN
                && diag.message_text.contains("'x' is of type 'unknown'")
        }),
        "expected TS18046 for `\"a\" in x` on unknown even with globalThis prewarm, got: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND
                && diag.message_text.contains("Type '{}' may represent a primitive value")
        }),
        "expected TS2638 for truthiness-narrowed unknown even with globalThis prewarm, got: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND
                && diag
                    .message_text
                    .contains("Type 'NonNullable<T>' may represent a primitive value")
        }),
        "expected TS2638 for truthiness-narrowed generic even with globalThis prewarm, got: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag
                    .message_text
                    .contains("Type 'T' is not assignable to type 'object'")
        }),
        "expected TS2322 for `\"a\" in x` on generic T even with globalThis prewarm, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn resolve_json_module_not_defaulted_for_node_resolution() {
    for (module, module_resolution) in [("commonjs", "node10"), ("node16", "node16")] {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;

        write_file(
            &base.join("tsconfig.json"),
            &format!(
                r#"{{
                  "compilerOptions": {{
                    "noEmit": true,
                    "module": "{module}",
                    "moduleResolution": "{module_resolution}",
                    "ignoreDeprecations": "6.0"
                  }},
                  "files": ["index.ts"]
                }}"#
            ),
        );
        write_file(
            &base.join("index.ts"),
            r#"import data from "./data.json";
const value: number = data.value;
"#,
        );
        write_file(&base.join("data.json"), r#"{"value":"x"}"#);

        let args = default_args();
        let result = compile(&args, base).expect("compilation should succeed");
        let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(
                &diagnostic_codes::CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E
            ),
            "expected TS2732 for {module_resolution}, got: {:?}",
            result.diagnostics
        );
        assert!(
            !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
            "JSON contents should not be type-checked when resolveJsonModule is omitted for {module_resolution}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn compile_default_always_strict_emits_prologue_and_false_suppresses() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    let source = r#"function f(this: void) {
  return this;
}

console.log(f() === undefined);
"#;

    let default_dir = base.join("default");
    write_file(
        &default_dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"esnext","outDir":"out"},"files":["index.ts"]}"#,
    );
    write_file(&default_dir.join("index.ts"), source);

    let args = default_args();
    let result = compile(&args, &default_dir).expect("default compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "default compile should not diagnose: {:?}",
        result.diagnostics
    );
    let js = fs::read_to_string(default_dir.join("out/index.js")).expect("read default output");
    assert!(
        js.starts_with("\"use strict\";\n"),
        "default emit should start with a strict prologue.\nOutput:\n{js}"
    );

    let false_dir = base.join("false");
    write_file(
        &false_dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"esnext","alwaysStrict":false,"ignoreDeprecations":"6.0","outDir":"out"},"files":["index.ts"]}"#,
    );
    write_file(&false_dir.join("index.ts"), source);

    let args = default_args();
    let result = compile(&args, &false_dir).expect("alwaysStrict false compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "alwaysStrict false compile should not diagnose: {:?}",
        result.diagnostics
    );
    let js = fs::read_to_string(false_dir.join("out/index.js")).expect("read false output");
    assert!(
        !js.starts_with("\"use strict\";\n"),
        "explicit alwaysStrict=false should suppress the strict prologue.\nOutput:\n{js}"
    );
}

#[test]
fn check_js_implies_allow_js_for_compilation() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "checkJs": true,
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check
const n = 1;
n();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION),
        "checkJs should imply allowJs instead of TS5052, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION
        ),
        "checkJs should imply allowJs instead of TS6504, got: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE),
        "expected JS semantic diagnostic after accepting index.js, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_public_break_modifier_recovery_suppresses_ts1105() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("repro.ts"), "public break;\n");

    let args = parse_args(&[
        "tsz",
        "--target",
        "es2015",
        "--pretty",
        "false",
        "--noEmitOnError",
        "repro.ts",
    ]);
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 for modifier recovery, got {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATE
        ),
        "expected no downstream TS1105 after modifier recovery, got {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_for_of_unknown_expression_reports_ts18046() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
declare const value: unknown;

for (const item of value) {
  item;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == diagnostic_codes::IS_OF_TYPE_UNKNOWN
                && diagnostic
                    .message_text
                    .contains("'value' is of type 'unknown'")
        }),
        "expected TS18046 for for-of over unknown, got {:?}",
        result.diagnostics
    );
}

#[test]
fn cli_check_js_implies_allow_js_for_root_js_file() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("index.js"),
        r#"/** @type {number} */
const x = "s";
"#,
    );

    let args = parse_args(&[
        "tsz",
        "--ignoreConfig",
        "--checkJs",
        "--noEmit",
        "--pretty",
        "false",
        "index.js",
    ]);
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION
        ),
        "CLI checkJs should imply allowJs instead of TS6504, got: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected checked-JS assignment diagnostic after accepting index.js, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn plain_js_suppresses_ts2774_without_check_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "noEmit": true,
            "strict": true
          },
          "files": ["plain.js", "checked.js"]
        }"#,
    );
    write_file(
        &base.join("plain.js"),
        r#"function f() {}
if (f) {}
"#,
    );
    write_file(
        &base.join("checked.js"),
        r#"// @ts-check
function g() {}
if (g) {}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let ts2774: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2774)
        .collect();
    assert_eq!(
        ts2774.len(),
        1,
        "plain JS should suppress TS2774, while @ts-check JS should keep it. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts2774[0].file.ends_with("checked.js"),
        "TS2774 should come from the @ts-check file, got: {:?}",
        ts2774[0]
    );
}

#[test]
fn compile_checked_js_yield_outside_generator_reports_ts1163() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "allowJs": true,
            "checkJs": true,
            "skipLibCheck": true,
            "lib": ["es2020"]
          },
          "files": ["yield-outside-generator.js"]
        }"#,
    );
    write_file(
        &base.join("yield-outside-generator.js"),
        r#"// @ts-check
function f() {
  yield 1;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY),
        "Expected TS1163 for yield in checked JS non-generator, got: {codes:?}"
    );
}

#[test]
fn instanceof_rhs_validation_uses_lib_function_when_local_type_shadows_function() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"export {};

type Function = { tag: string };

declare const value: object;
declare const fakeConstructor: { tag: string };

value instanceof fakeConstructor;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2359),
        "expected TS2359 for non-callable instanceof RHS despite local Function type alias, got: {:?}",
        result.diagnostics
    );
}

fn load_real_default_lib_files(target: ScriptTarget) -> Vec<Arc<tsz_binder::lib_loader::LibFile>> {
    let lib_paths = crate::config::resolve_default_lib_files(target).expect("default libs");
    let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
    tsz::parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs")
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    None
}

#[test]
fn compile_document_create_element_overload_augmentation_no_false_ts2430() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();
    write_file(
        &base.join("parserOverloadOnConstants1.ts"),
        r#"
interface Document {
    createElement(tagName: string): HTMLElement;
    createElement(tagName: 'canvas'): HTMLCanvasElement;
    createElement(tagName: 'div'): HTMLDivElement;
    createElement(tagName: 'span'): HTMLSpanElement;
}
"#,
    );

    let args = parse_args(&[
        "tsz",
        "--target",
        "es2015",
        "--noEmit",
        "--pretty",
        "false",
        "--ignoreConfig",
        "parserOverloadOnConstants1.ts",
    ]);
    let result = compile(&args, base).expect("compilation should succeed");
    let ts2430: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE)
        .collect();

    assert!(
        ts2430.is_empty(),
        "Expected DOM Document createElement overload augmentation to avoid false TS2430, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_duplicate_amd_module_name_directives_reports_ts2458() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"///<amd-module name='FirstModuleName'/>
///<amd-module name='SecondModuleName'/>
class Foo {
  x: number;
  constructor() {
    this.x = 5;
  }
}
export = Foo;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "amd"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2458),
        "Expected TS2458 for duplicate AMD module name directives, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_triple_slash_prefix_tags_are_comments() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("reference-path-prefix.ts"),
        r#"/// <referencex path="./missing-file" />

export const pathCase = 1;
"#,
    );
    write_file(
        &base.join("reference-types-prefix.ts"),
        r#"/// <referencex types="missing-prefix-types" />

export const typesCase = 1;
"#,
    );
    write_file(
        &base.join("malformed-reference-prefix.ts"),
        r#"/// <referencex path=./missing-file />

export const malformedCase = 1;
"#,
    );
    write_file(
        &base.join("amd-module-prefix.ts"),
        r#"/// <amd-modulex name="first" />
/// <amd-module name="second" />

export const amdCase = 1;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true
          },
          "files": [
            "reference-path-prefix.ts",
            "reference-types-prefix.ts",
            "malformed-reference-prefix.ts",
            "amd-module-prefix.ts"
          ]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    for unexpected in [1084, 2458, 2688, 6231] {
        assert!(
            !codes.contains(&unexpected),
            "prefix triple-slash tags should be comments; got diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn compile_extensionless_reference_path_probes_js_when_allow_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("main.ts"),
        r#"/// <reference path="dep" />
const marker = 1;
"#,
    );
    write_file(
        &base.join("dep.js"),
        r#"// @ts-check
const x = 1;
x();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS),
        "extensionless .js reference should resolve under allowJs: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE),
        "referenced dep.js should be included and checked: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_empty_triple_slash_reference_path_reports_ts6231() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("index.ts"),
        "/// <reference path=\"\" />\nexport {};\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|d| d.code == diagnostic_codes::COULD_NOT_RESOLVE_THE_PATH_WITH_THE_EXTENSIONS)
        .unwrap_or_else(|| {
            panic!(
                "expected TS6231 for empty reference path, got: {:?}",
                result.diagnostics
            )
        });

    assert_eq!(diagnostic.start, 21);
    assert_eq!(diagnostic.length, 0);
    assert!(
        diagnostic
            .message_text
            .contains(base.to_string_lossy().as_ref()),
        "TS6231 should report the containing directory for an empty path: {}",
        diagnostic.message_text
    );
}

#[test]
fn compile_source_reference_lib_unknown_name_reports_ts2726() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("invalid-lib.ts"),
        "/// <reference lib=\"notalib\" />\nlet x = 1;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "lib": ["es2020"]
          },
          "files": ["invalid-lib.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let lib_diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == diagnostic_codes::CANNOT_FIND_LIB_DEFINITION_FOR)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2726 for `notalib`, got: {:?}",
                result.diagnostics
            )
        });
    assert!(
        lib_diag.message_text.contains("'notalib'"),
        "TS2726 message should reference the offending lib name: {}",
        lib_diag.message_text
    );
    // Position should anchor at the lib value (byte 20 == column 21 for
    // `/// <reference lib="notalib" />`), matching tsc.
    assert_eq!(lib_diag.start, 20, "diagnostic anchored at lib value start");
    assert_eq!(lib_diag.length, 7, "length covers `notalib`");
    assert!(
        lib_diag.file.ends_with("invalid-lib.ts"),
        "file should be the user source: {}",
        lib_diag.file
    );
}

#[test]
fn compile_source_reference_lib_empty_name_reports_ts2726() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("empty-lib.ts"),
        "/// <reference lib=\"\" />\nlet x = 1;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "lib": ["es2020"]
          },
          "files": ["empty-lib.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let lib_diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == diagnostic_codes::CANNOT_FIND_LIB_DEFINITION_FOR)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2726 for empty lib name, got: {:?}",
                result.diagnostics
            )
        });
    assert!(
        lib_diag.message_text.contains("''"),
        "TS2726 message should render empty quotes: {}",
        lib_diag.message_text
    );
    assert_eq!(lib_diag.start, 20, "diagnostic anchored at empty lib value");
    assert_eq!(lib_diag.length, 0, "length is zero for empty lib value");
}

#[test]
fn compile_source_reference_lib_known_name_does_not_report_ts2726() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("ok-lib.ts"),
        "/// <reference lib=\"es2015\" />\nlet x = 1;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "lib": ["es2020"]
          },
          "files": ["ok-lib.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_LIB_DEFINITION_FOR),
        "known lib name should not trigger TS2726: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_lib_esnext_loads_disposable_symbols() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("resource.ts"),
        r#"
class Resource {
  constructor(public name: string) {}
  [Symbol.dispose](): void {}
}

function useResource() {
  using resource = new Resource("test");
  const _name: string = resource.name;
}

class AsyncResource {
  constructor(public name: string) {}
  async [Symbol.asyncDispose](): Promise<void> {
    await Promise.resolve();
  }
}

async function useAsyncResource() {
  await using resource = new AsyncResource("async-test");
  const _name: string = resource.name;
}

export {};
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "lib": ["esnext"]
          },
          "files": ["resource.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "--lib esnext should load Symbol.dispose and Symbol.asyncDispose without diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_triple_slash_reference_attribute_must_match_exactly() {
    // Regression for #3375: triple-slash reference attributes must be matched
    // as exact attribute names. `notpath="..."` must NOT be treated as
    // `path="..."`. tsc reports TS1084 for the invalid directive and does not
    // pull in the bogus referenced file, so the global declared in extra.d.ts
    // must be unresolved (TS2304).
    //
    // The bogus and valid cases use separate compilations because ambient
    // declarations from extra.d.ts become global across the entire program
    // once any file in the program pulls it in - sharing one project would
    // mask the leak the bug demonstrates.

    // === Bogus attribute: must NOT pull in extra.d.ts; must emit TS1084. ===
    let bogus_temp = TempDir::new().expect("temp dir (bogus)");
    let bogus_base = bogus_temp.path.as_path();
    write_file(
        &bogus_base.join("extra.d.ts"),
        "declare const extraGlobal: number;\n",
    );
    write_file(
        &bogus_base.join("main.ts"),
        r#"/// <reference notpath="./extra.d.ts" />
extraGlobal.toFixed();
"#,
    );
    write_file(
        &bogus_base.join("tsconfig.json"),
        r#"{
          "compilerOptions": { "noEmit": true, "strict": true },
          "files": ["main.ts"]
        }"#,
    );

    let mut bogus_args = default_args();
    bogus_args.project = Some(bogus_base.join("tsconfig.json"));
    let bogus_result = compile(&bogus_args, bogus_base).expect("bogus compile should succeed");
    let bogus_codes: Vec<u32> = bogus_result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        bogus_codes.contains(&1084),
        "Expected TS1084 (invalid reference directive) for `notpath=`; got: {:?}",
        bogus_result.diagnostics
    );
    assert!(
        bogus_codes.contains(&2304),
        "Expected TS2304 (cannot find `extraGlobal`) - bogus reference must not pull in extra.d.ts; got: {:?}",
        bogus_result.diagnostics
    );

    // === Control: a valid path attribute must still resolve and type-check. ===
    let valid_temp = TempDir::new().expect("temp dir (valid)");
    let valid_base = valid_temp.path.as_path();
    write_file(
        &valid_base.join("extra.d.ts"),
        "declare const extraGlobal: number;\n",
    );
    write_file(
        &valid_base.join("main.ts"),
        r#"/// <reference path="./extra.d.ts" />
extraGlobal.toFixed();
"#,
    );
    write_file(
        &valid_base.join("tsconfig.json"),
        r#"{
          "compilerOptions": { "noEmit": true, "strict": true },
          "files": ["main.ts"]
        }"#,
    );

    let mut valid_args = default_args();
    valid_args.project = Some(valid_base.join("tsconfig.json"));
    let valid_result = compile(&valid_args, valid_base).expect("valid compile should succeed");
    let valid_codes: Vec<u32> = valid_result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !valid_codes.contains(&1084),
        "Valid `path=` directive must not be flagged TS1084; got: {:?}",
        valid_result.diagnostics
    );
    assert!(
        !valid_codes.contains(&2304),
        "Valid `path=` directive must pull in extra.d.ts so `extraGlobal` resolves; got: {:?}",
        valid_result.diagnostics
    );
}

#[test]
fn compile_import_elision_ignores_string_and_block_comment_text() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("string-literal.ts"),
        r#"import { Foo } from "./dep";

const label = "Foo";

label;
"#,
    );
    write_file(
        &base.join("block-comment.ts"),
        r#"import { Foo } from "./dep";

/* Foo */
const label = "bar";

label;
"#,
    );
    write_file(
        &base.join("line-comment.ts"),
        r#"import { Foo } from "./dep";

// Foo
const label = "bar";

label;
"#,
    );
    write_file(&base.join("dep.ts"), "export const Foo = 1;\n");
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "esnext",
            "strict": true,
            "outDir": "dist"
          },
          "files": [
            "string-literal.ts",
            "block-comment.ts",
            "line-comment.ts",
            "dep.ts"
          ]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    let string_js =
        fs::read_to_string(base.join("dist/string-literal.js")).expect("read string output");
    let block_js =
        fs::read_to_string(base.join("dist/block-comment.js")).expect("read block output");
    let line_js = fs::read_to_string(base.join("dist/line-comment.js")).expect("read line output");

    for output in [&string_js, &block_js, &line_js] {
        assert!(
            !output.contains("import { Foo }"),
            "unused Foo import should be elided; output:\n{output}"
        );
        assert!(
            output.contains("export {};"),
            "module marker should remain after import elision; output:\n{output}"
        );
    }
    assert!(string_js.contains("const label = \"Foo\";"));
    assert!(block_js.contains("/* Foo */"));
    assert!(line_js.contains("// Foo"));
}

#[test]
fn compile_amd_dependency_comment_name_fixture_keeps_ts2792_under_ts5107() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"///<amd-dependency path='aliasedModule5' name='n1'/>
///<amd-dependency path='unaliasedModule3'/>
///<amd-dependency path='aliasedModule6' name='n2'/>
///<amd-dependency path='unaliasedModule4'/>

import "unaliasedModule1";

import r1 = require("aliasedModule1");
r1;

import {p1, p2, p3} from "aliasedModule2";
p1;

import d from "aliasedModule3";
d;

import * as ns from "aliasedModule4";
ns;

import "unaliasedModule2";
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "amd"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes.iter().filter(|&&code| code == 5107).count(),
        1,
        "Expected one TS5107 deprecation diagnostic, got diagnostics: {:?}",
        result.diagnostics
    );
    // tsc only emits TS5107 here (no TS2307/TS2792). Under `module: amd`
    // without `ignoreDeprecations`, the deprecation diagnostic is the
    // user-visible signal and tsc suppresses the secondary missing-module
    // diagnostic. See `ts2792_emitted_for_missing_import_under_module_amd`
    // for the inverse case where `ignoreDeprecations: 6.0` re-enables TS2792.
    assert!(
        !codes.contains(&2307),
        "Did not expect TS2307 under module: amd, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2792),
        "Did not expect TS2792 under module: amd without ignoreDeprecations (TS5107 is the visible signal), got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn declaration_emit_ts2883_prefers_canonical_named_reference_message() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("src/index.ts"),
        r#"import { SomeType } from "some-dep";
export const foo = (thing: SomeType) => { return thing; };
export const bar = (thing: SomeType) => { return thing.arg; };
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/dist/inner.d.ts"),
        r#"export declare type Other = { other: string };
export declare type SomeType = { arg: Other };
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/dist/index.d.ts"),
        r#"export type OtherType = import('./inner').Other;
export type SomeType = import('./inner').SomeType;
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/package.json"),
        r#"{
  "name": "some-dep",
  "exports": { ".": "./dist/index.js" }
}"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "nodenext"
  },
  "files": ["src/index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|m| m
                .contains("reference to 'SomeType' from '../node_modules/some-dep/dist/inner'")),
        "expected canonical SomeType TS2883, got: {messages:#?}"
    );
    assert!(
        messages.iter().all(|m| !m.contains("reference to '../node_modules/some-dep/dist/inner' from 'Other'")),
        "expected swapped TS2883 to be filtered, got: {messages:#?}"
    );
}

#[test]
fn declaration_emit_default_object_assign_reports_nested_reference_ts2883_for_default_only() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base
            .join("node_modules/styled-components/node_modules/hoist-non-react-statics/index.d.ts"),
        r#"interface Statics {
    "$$whatever": string;
}
declare namespace hoistNonReactStatics {
    type NonReactStatics<T> = {[X in Exclude<keyof T, keyof Statics>]: T[X]}
}
export = hoistNonReactStatics;
"#,
    );
    write_file(
        &base.join("node_modules/styled-components/index.d.ts"),
        r#"import * as hoistNonReactStatics from "hoist-non-react-statics";
export interface DefaultTheme {}
export type StyledComponent<TTag extends string, TTheme = DefaultTheme, TStyle = {}, TWhatever = never> =
    string
    & StyledComponentBase<TTag, TTheme, TStyle, TWhatever>
    & hoistNonReactStatics.NonReactStatics<TTag>;
export interface StyledComponentBase<TTag extends string, TTheme = DefaultTheme, TStyle = {}, TWhatever = never> {
    tag: TTag;
    theme: TTheme;
    style: TStyle;
    whatever: TWhatever;
}
export interface StyledInterface {
    div: (a: TemplateStringsArray) => StyledComponent<"div">;
}
declare const styled: StyledInterface;
export default styled;
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import styled from "styled-components";

const A = styled.div``;
const B = styled.div``;
export const C = styled.div``;

export default Object.assign(A, {
    B,
    C
});
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected only the default Object.assign export to report TS2883, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages
            .iter()
            .any(|message| message.contains("inferred type of 'default'")),
        "expected TS2883 for default export, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages
            .iter()
            .all(|message| !message.contains("inferred type of 'C'")),
        "named export C has a reusable public surface type and should not report TS2883: {ts2883_messages:#?}"
    );
}

#[cfg(unix)]
#[test]
fn declaration_emit_symlinked_import_type_package_ref_stays_portable() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("Folder/monorepo/package-a/index.d.ts"),
        r#"export declare const styles: import("styled-components").InterpolationValue[];
"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/package.json"),
        r#"{
  "name": "styled-components",
  "version": "3.3.3",
  "typings": "typings/styled-components.d.ts"
}"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/typings/styled-components.d.ts"),
        r#"export interface InterpolationValue {}
"#,
    );
    write_file(
        &base.join("Folder/monorepo/core/index.ts"),
        r#"import { styles } from "package-a";

export function getStyles() {
    return styles;
}
"#,
    );
    std::fs::create_dir_all(base.join("Folder/monorepo/package-a/node_modules"))
        .expect("package-a node_modules");
    std::fs::create_dir_all(base.join("Folder/monorepo/core/node_modules"))
        .expect("core node_modules");
    std::os::unix::fs::symlink(
        base.join("Folder/node_modules/styled-components"),
        base.join("Folder/monorepo/package-a/node_modules/styled-components"),
    )
    .expect("styled-components symlink");
    std::os::unix::fs::symlink(
        base.join("Folder/monorepo/package-a"),
        base.join("Folder/monorepo/core/node_modules/package-a"),
    )
    .expect("package-a symlink");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--ignoreConfig",
        "--alwaysStrict",
        "true",
        "--esModuleInterop",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "Folder/monorepo/package-a/index.d.ts",
        "Folder/node_modules/styled-components/typings/styled-components.d.ts",
        "Folder/monorepo/core/index.ts",
    ])
    .expect("CLI args should parse");

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2883),
        "symlinked package import type should be portable, got: {:#?}",
        result.diagnostics
    );
    let dts =
        fs::read_to_string(base.join("Folder/monorepo/core/index.d.ts")).expect("read core d.ts");
    assert!(
        dts.contains(
            r#"export declare function getStyles(): import("styled-components").InterpolationValue[];"#
        ),
        "expected bare package import type in declaration emit, got: {dts}"
    );
}

#[cfg(unix)]
#[test]
fn declaration_emit_symlinked_import_type_package_ref_reports_when_dependency_is_resolution_only() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("Folder/monorepo/package-a/index.d.ts"),
        r#"export declare const styles: import("styled-components").InterpolationValue[];
"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/package.json"),
        r#"{
  "name": "styled-components",
  "version": "3.3.3",
  "typings": "typings/styled-components.d.ts"
}"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/typings/styled-components.d.ts"),
        r#"export interface InterpolationValue {}
"#,
    );
    write_file(
        &base.join("Folder/monorepo/core/index.ts"),
        r#"import { styles } from "package-a";

export function getStyles() {
    return styles;
}
"#,
    );
    std::fs::create_dir_all(base.join("Folder/monorepo/package-a/node_modules"))
        .expect("package-a node_modules");
    std::fs::create_dir_all(base.join("Folder/monorepo/core/node_modules"))
        .expect("core node_modules");
    std::os::unix::fs::symlink(
        base.join("Folder/node_modules/styled-components"),
        base.join("Folder/monorepo/package-a/node_modules/styled-components"),
    )
    .expect("styled-components symlink");
    std::os::unix::fs::symlink(
        base.join("Folder/monorepo/package-a"),
        base.join("Folder/monorepo/core/node_modules/package-a"),
    )
    .expect("package-a symlink");

    let mut args = default_args();
    args.project = Some(base.join("Folder/monorepo/core/tsconfig.json"));
    write_file(
        &base.join("Folder/monorepo/core/tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected one TS2883 diagnostic for resolution-only symlinked import type, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages[0].contains("InterpolationValue"),
        "expected TS2883 to name imported member, got: {}",
        ts2883_messages[0]
    );
    assert!(
        ts2883_messages[0]
            .contains("../package-a/node_modules/styled-components/typings/styled-components"),
        "expected TS2883 to preserve source-package node_modules path, got: {}",
        ts2883_messages[0]
    );
}

#[test]
fn declaration_emit_commonjs_call_tuple_reports_nested_reference_ts2883() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("r/node_modules/foo/node_modules/nested/index.d.ts"),
        "export interface NestedProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/other/index.d.ts"),
        "export interface OtherIndexProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/other.d.ts"),
        "export interface OtherProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/index.d.ts"),
        r#"import { OtherProps } from "./other";
import { OtherIndexProps } from "./other/index";
import { NestedProps } from "nested";
export interface SomeProps {}

export function foo(): [SomeProps, OtherProps, OtherIndexProps, NestedProps];
"#,
    );
    write_file(
        &base.join("node_modules/root/index.d.ts"),
        r#"export interface RootProps {}

export function bar(): RootProps;
"#,
    );
    write_file(
        &base.join("r/entry.ts"),
        r#"import { foo } from "foo";
import { bar } from "root";
export const x = foo();
export const y = bar();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["r/entry.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected one TS2883 diagnostic for nested tuple return reference, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages[0].contains("NestedProps"),
        "expected TS2883 to name NestedProps, got: {}",
        ts2883_messages[0]
    );
    assert!(
        ts2883_messages[0].contains("foo/node_modules/nested"),
        "expected TS2883 to reference foo/node_modules/nested, got: {}",
        ts2883_messages[0]
    );
}

