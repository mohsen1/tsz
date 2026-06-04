#[test]
fn declaration_emit_expands_foreign_import_mapped_keys_from_nested_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "dist",
            "rootDir": "r",
            "target": "es2017",
            "module": "commonjs",
            "moduleResolution": "node",
            "ignoreDeprecations": "6.0",
            "skipLibCheck": true,
            "strict": true,
            "typeRoots": ["./empty-types"]
          },
          "files": ["r/entry.ts"]
        }"#,
    );
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("r/entry.ts"),
        r#"import { foo } from "foo";

export const x = foo();
"#,
    );
    write_file(
        &base.join("r/node_modules/foo/index.d.ts"),
        r#"export function foo(): { [K in import("keys").Key]?: string };
"#,
    );
    write_file(
        &base.join("r/node_modules/foo/node_modules/keys/index.d.ts"),
        r#"export type Key = "a" | "b";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "did not expect diagnostics: {:?}",
        result.diagnostics
    );

    let dts = std::fs::read_to_string(base.join("dist/entry.d.ts"))
        .expect("Declaration output should be emitted");
    assert!(
        dts.contains("a?: string | undefined;"),
        "expected expanded mapped key 'a': {dts}",
    );
    assert!(
        dts.contains("b?: string | undefined;"),
        "expected expanded mapped key 'b': {dts}",
    );
    assert!(
        !dts.contains("[K in"),
        "foreign mapped type should not leak into declaration output: {dts}",
    );
}

#[test]
fn declaration_emit_skips_file_with_ts4023_but_writes_unaffected_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("type.ts"),
        r#"export namespace Foo {
    export const sym = Symbol();
}

export type Type = { x?: { [Foo.sym]: 0 } };
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import { type Type } from "./type";

export const foo = { ...({} as Type) };
"#,
    );

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--target",
        "es2015",
        "--strict",
        "--lib",
        "esnext",
        "--declaration",
        "--emitDeclarationOnly",
        "--listEmittedFiles",
        "--outDir",
        "dist",
        "--pretty",
        "false",
        "index.ts",
        "type.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED),
        "expected TS4023 diagnostic, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !base.join("dist/index.d.ts").exists(),
        "Declaration output for file with TS4023 should not be written"
    );
    assert!(
        base.join("dist/type.d.ts").is_file(),
        "Unaffected declaration output should still be written"
    );
    assert!(
        !result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/index.d.ts")),
        "emitted files should not include blocked declaration: {:?}",
        result.emitted_files
    );
    assert!(
        result
            .emitted_files
            .iter()
            .any(|path| path.ends_with("dist/type.d.ts")),
        "emitted files should include unaffected declaration: {:?}",
        result.emitted_files
    );
}

#[test]
fn compile_emit_declaration_only_from_cli_suppresses_js_output() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(
        &base.join("main.ts"),
        "export const value: string = \"ok\";\n",
    );

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--emitDeclarationOnly",
        "--pretty",
        "false",
        "--typeRoots",
        "./empty-types",
        "--skipLibCheck",
        "--target",
        "es2017",
        "--lib",
        "es2017",
        "--outDir",
        "dist",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(
        base.join("dist/main.d.ts").is_file(),
        "Declaration output should be emitted"
    );
    assert!(
        !base.join("dist/main.js").exists(),
        "JavaScript output should be suppressed by CLI emitDeclarationOnly"
    );
}

#[test]
fn compile_config_allow_importing_ts_extensions_requires_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowImportingTsExtensions": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "allowImportingTsExtensions without an emit guard should report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_config_allow_importing_ts_extensions_accepts_no_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowImportingTsExtensions": true,
            "noEmit": true
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "allowImportingTsExtensions with noEmit should not report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_cli_allow_importing_ts_extensions_requires_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowImportingTsExtensions",
        "--ignoreConfig",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "CLI allowImportingTsExtensions without an emit guard should report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_cli_allow_importing_ts_extensions_accepts_no_emit_guard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowImportingTsExtensions",
        "--noEmit",
        "--ignoreConfig",
        "main.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(
            &diagnostic_codes::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLAR
        ),
        "CLI allowImportingTsExtensions with noEmit should not report TS5096, got: {codes:?}"
    );
}

#[test]
fn compile_bundler_dts_value_import_reports_ts2846_not_ts2307() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["a.ts", "types.d.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "export {};\n");
    write_file(&base.join("types.d.ts"), "import {} from \"./a.d.ts\";\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(
            &diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT
        ),
        "expected TS2846 for value import of ./a.d.ts, got: {:#?}",
        result.diagnostics
    );
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "TS2846 should suppress TS2307 for ./a.d.ts when ./a.ts exists, got: {:#?}",
        result.diagnostics
    );
}
