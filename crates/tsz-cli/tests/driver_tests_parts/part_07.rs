#[test]
fn cli_declaration_dir_places_declarations_outside_out_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/index.ts"), "export const value = 42;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--outDir",
        "dist",
        "--rootDir",
        "src",
        "--declaration",
        "--declarationDir",
        "types",
        "src/index.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be in dist/"
    );
    assert!(
        base.join("types/index.d.ts").is_file(),
        "Declaration file should be in CLI declarationDir"
    );
    assert!(
        !base.join("dist/index.d.ts").is_file(),
        "Declaration file should not fall back to outDir"
    );
}

#[test]
fn compile_outdir_places_output_in_directory() {
    // Test that outDir places compiled files in the specified directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in build/ directory
    assert!(
        base.join("build/src/index.js").is_file(),
        "JS output should be in build/src/"
    );

    // Output should NOT be alongside source
    assert!(
        !base.join("src/index.js").is_file(),
        "JS output should NOT be alongside source when outDir is set"
    );
}

#[test]
fn compile_outdir_absent_outputs_alongside_source() {
    // Test that missing outDir places compiled files alongside source files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {},
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be alongside source file
    assert!(
        base.join("src/index.js").is_file(),
        "JS output should be alongside source when outDir is not set"
    );
}

#[test]
fn compile_outdir_with_rootdir_flattens_paths() {
    // Test that rootDir + outDir flattens the output path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // With rootDir=src, output should NOT include src/ in path
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be at dist/index.js (flattened)"
    );
    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "Nested JS output should be at dist/utils/helpers.js"
    );

    // Should NOT be at dist/src/...
    assert!(
        !base.join("dist/src/index.js").is_file(),
        "Output should NOT include src/ when rootDir is set to src"
    );
}

#[test]
fn compile_outdir_nested_structure() {
    // Test that outDir preserves nested directory structure
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const main = 1;");
    write_file(&base.join("src/models/user.ts"), "export const user = 2;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 3;",
    );
    write_file(
        &base.join("src/services/api/client.ts"),
        "export const client = 4;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All nested directories should be preserved
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/models/user.js").is_file());
    assert!(base.join("dist/src/utils/helpers.js").is_file());
    assert!(base.join("dist/src/services/api/client.js").is_file());
}

#[test]
fn compile_outdir_deep_nested_path() {
    // Test that outDir can be a deeply nested path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build/output/js"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in deeply nested outDir
    assert!(
        base.join("build/output/js/src/index.js").is_file(),
        "JS output should be in build/output/js/src/"
    );
}

#[test]
fn compile_outdir_with_declaration_and_sourcemap() {
    // Test that outDir works correctly with declaration and sourceMap
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All output files should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS should be in outDir"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "Declaration should be in outDir"
    );
    assert!(
        base.join("dist/index.js.map").is_file(),
        "Source map should be in outDir"
    );

    // Verify source map references correct file
    let map_contents = std::fs::read_to_string(base.join("dist/index.js.map")).expect("read map");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map");
    let file_field = map_json.get("file").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(
        file_field, "index.js",
        "Source map file field should be index.js"
    );
}

#[test]
fn compile_outdir_multiple_entry_points() {
    // Test outDir with multiple entry point files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const main = 1;");
    write_file(&base.join("src/worker.ts"), "export const worker = 2;");
    write_file(&base.join("src/cli.ts"), "export const cli = 3;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All entry points should be compiled to outDir
    assert!(base.join("dist/main.js").is_file());
    assert!(base.join("dist/worker.js").is_file());
    assert!(base.join("dist/cli.js").is_file());
}

// =============================================================================
// Error Handling: Missing Input Files
// =============================================================================

#[test]
fn compile_missing_file_in_files_array_returns_error() {
    // Test that referencing a missing file in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/missing.ts"]
        }"#,
    );
    // Intentionally NOT creating src/missing.ts

    let args = default_args();
    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("TS6053") || err.contains("missing"),
        "Error should mention file not found: {err}"
    );
    // No output should be produced
    assert!(!base.join("dist").is_dir());
}

#[test]
fn compile_missing_file_in_include_pattern_returns_error() {
    // Test that an include pattern matching no files returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    // Intentionally NOT creating any .ts files in src/

    let args = default_args();
    let result = compile(&args, base);

    // Should return Ok with TS18003 diagnostic (not a fatal error)
    let compilation = result.expect("Should return Ok with diagnostics, not a fatal error");
    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 18003),
        "Should contain TS18003 diagnostic when no input files found, got: {:?}",
        compilation
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_ignore_config_without_files_reports_ts18003_from_discovered_config() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noLib": true
          }
        }"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.no_emit = true;
    let compilation = compile(&args, base).expect("Should return Ok with diagnostics");

    let ts18003 = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == 18003)
        .expect("expected TS18003 diagnostic");
    let expected_path = base
        .join("tsconfig.json")
        .canonicalize()
        .expect("canonical config path");
    let expected_path = expected_path.to_string_lossy();
    assert!(
        ts18003.message_text.contains(expected_path.as_ref()),
        "TS18003 should include discovered config path: {}",
        ts18003.message_text
    );
    assert!(
        ts18003
            .message_text
            .contains("Specified 'include' paths were '[\"**/*\"]'"),
        "TS18003 should use default include paths when --ignoreConfig skips config loading: {}",
        ts18003.message_text
    );
}

#[test]
fn compile_terminal_include_star_matches_direct_js_file() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "strict": true
          },
          "include": ["src/*"]
        }"#,
    );
    write_file(
        &base.join("src/a.js"),
        r#"// @ts-check
function takesString(value) {
  return value.toUpperCase();
}

takesString(123);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&18003),
        "terminal include star should discover src/a.js instead of reporting TS18003, got: {codes:?}"
    );
    assert!(
        codes.contains(&7006),
        "discovered checked JS file should be type checked, got: {codes:?}"
    );
}

#[test]
fn compile_inherited_include_resolves_from_base_config_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("base/tsconfig.base.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("base/src/a.ts"),
        "export const baseOnly: string = 1;",
    );
    write_file(
        &base.join("app/tsconfig.json"),
        r#"{
          "extends": "../base/tsconfig.base.json"
        }"#,
    );

    let mut args = default_args();
    args.project = Some(PathBuf::from("app"));
    let compilation = compile(&args, base).expect("compile should return diagnostics");

    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 2322),
        "expected inherited include to check base/src/a.ts and report TS2322, got {:?}",
        compilation
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_inherited_files_resolves_from_base_config_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("base/tsconfig.base.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true
          },
          "files": ["src/a.ts"]
        }"#,
    );
    write_file(
        &base.join("base/src/a.ts"),
        "export const baseFileOnly: string = 1;",
    );
    write_file(
        &base.join("app/tsconfig.json"),
        r#"{
          "extends": "../base/tsconfig.base.json"
        }"#,
    );

    let mut args = default_args();
    args.project = Some(PathBuf::from("app"));
    let compilation = compile(&args, base).expect("compile should return diagnostics");

    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 2322),
        "expected inherited files entry to check base/src/a.ts and report TS2322, got {:?}",
        compilation
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_missing_file_in_include_pattern_reports_custom_config_path() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let config_rel = PathBuf::from("configs/custom-name.json");
    let config_path = base.join(&config_rel);

    write_file(
        &config_path,
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(config_rel);
    let compilation = compile(&args, base).expect("Should return Ok with diagnostics");

    let ts18003 = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == 18003)
        .expect("expected TS18003 diagnostic");
    let expected_path = config_path.canonicalize().expect("canonical config path");
    let expected_path = expected_path.to_string_lossy();
    assert!(
        ts18003.message_text.contains(expected_path.as_ref()),
        "TS18003 should include resolved config path: {}",
        ts18003.message_text
    );
}

#[test]
fn compile_missing_file_in_include_pattern_prefers_ts18003_over_type_root_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "include": ["src/**/*.ts"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(
        &base.join("node_modules/@types/lib-extender/index.d.ts"),
        r#"declare var lib: () => void;
declare namespace lib {}
export = lib;
declare module "lib" {
    export function fn(): void;
}"#,
    );

    let args = default_args();
    let compilation = compile(&args, base).expect("Should return Ok with diagnostics");

    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 18003),
        "Expected TS18003 when include pattern has no matching source files"
    );
    assert!(
        !compilation.diagnostics.iter().any(|d| d.code == 2649),
        "TS2649 from @types files should not be reported when there are no root inputs"
    );
}

#[test]
fn compile_missing_single_file_via_cli_args_returns_error() {
    // Test that passing a non-existent file via CLI args returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.files = vec![PathBuf::from("nonexistent.ts")];

    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing CLI file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("No such file"),
        "Error should mention file not found: {err}"
    );
}

#[test]
fn compile_directory_as_cli_root_file_returns_ts6231_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    std::fs::create_dir_all(base.join("src")).unwrap();
    write_file(&base.join("src/index.ts"), "export const ok = 1;\n");

    let mut args = default_args();
    args.files = vec![PathBuf::from("src")];

    let result = compile(&args, base);

    assert!(
        result.is_err(),
        "Should return error when root file is a directory"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.starts_with("TS6231: "),
        "Error should be a TS6231 marker for directory root file, got: {err}"
    );
}

#[test]
fn compile_dot_as_cli_root_file_returns_ts6231_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("index.ts"), "export const ok = 1;\n");

    let mut args = default_args();
    args.files = vec![PathBuf::from(".")];

    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error when root file is '.'");
    let err = result.unwrap_err().to_string();
    assert!(
        err.starts_with("TS6231: "),
        "Error should be a TS6231 marker for '.' root file, got: {err}"
    );
}

#[test]
fn compile_missing_multiple_files_in_files_array_returns_error() {
    // Test that multiple missing files in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/a.ts", "src/b.ts", "src/c.ts"]
        }"#,
    );
    // Only create one of the three files
    write_file(&base.join("src/b.ts"), "export const b = 2;");

    let args = default_args();
    let result = compile(&args, base);

    // Should return error for missing files
    assert!(
        result.is_err(),
        "Should return error when some files in files array are missing"
    );
}

#[test]
fn compile_missing_project_directory_returns_error() {
    // Test that specifying a non-existent project directory returns TS5058
    // ("The specified path does not exist"), matching tsc behavior. TS5057 is
    // reserved for the case where the directory exists but lacks tsconfig.json.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.project = Some(PathBuf::from("nonexistent_project"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic for missing project directory"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::THE_SPECIFIED_PATH_DOES_NOT_EXIST,
        "Should have TS5058 for non-existent --project path"
    );
    assert_eq!(
        result.diagnostics[0].message_text,
        "The specified path does not exist: 'nonexistent_project'."
    );
}

#[test]
fn compile_missing_project_file_returns_ts5058() {
    // --project pointing at a non-existent .json file should also be TS5058.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.project = Some(PathBuf::from("missing/tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    assert!(!result.diagnostics.is_empty());
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::THE_SPECIFIED_PATH_DOES_NOT_EXIST,
    );
    assert_eq!(
        result.diagnostics[0].message_text,
        "The specified path does not exist: 'missing/tsconfig.json'."
    );
}

#[test]
fn compile_missing_tsconfig_in_project_dir_returns_error() {
    // Test that a project directory without tsconfig.json returns an error diagnostic
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create project directory but no tsconfig.json
    std::fs::create_dir_all(base.join("myproject")).expect("create dir");
    write_file(&base.join("myproject/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("myproject"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    // Should have error diagnostic since there's no tsconfig.json
    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic when tsconfig.json is missing in project dir"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
        "Should have correct error code"
    );
    assert_eq!(
        result.diagnostics[0].message_text,
        "Cannot find a tsconfig.json file at the specified directory: 'myproject'."
    );
}

#[test]
fn compile_missing_tsconfig_uses_defaults() {
    // Test that compilation works without tsconfig.json using defaults
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("src/index.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    // Output should be next to source when no outDir specified
    assert!(base.join("src/index.js").is_file());
}

#[test]
fn compile_ambient_external_module_without_internal_import_declaration_fixture() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "ambientExternalModuleWithoutInternalImportDeclaration_0.ts",
            "ambientExternalModuleWithoutInternalImportDeclaration_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_0.ts"),
        r#"declare module 'M' {
    namespace C {
        export var f: number;
    }
    class C {
        foo(): void;
    }
    export = C;
}"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_1.ts"),
        r#"/// <reference path='ambientExternalModuleWithoutInternalImportDeclaration_0.ts'/>
import A = require('M');
var c = new A();"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_alias_on_merged_module_interface_fixture_reports_ts2708() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "aliasOnMergedModuleInterface_0.ts",
            "aliasOnMergedModuleInterface_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_0.ts"),
        r#"declare module "foo" {
    namespace B {
        export interface A {}
    }
    interface B {
        bar(name: string): B.A;
    }
    export = B;
}"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_1.ts"),
        r#"/// <reference path='aliasOnMergedModuleInterface_0.ts' />
import foo = require("foo");
declare var z: foo;
z.bar("hello");
var x: foo.A = foo.bar("hello");"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2708),
        "Expected TS2708, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// E2E: Generic Utility Library Compilation
// =============================================================================

#[test]
fn compile_generic_utility_library_array_utils() {
    // Test compilation of generic array utility functions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true,
            "strict": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Generic array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function map<T, U>(arr: T[], fn: (item: T, index: number) => U): U[] {
    const result: U[] = [];
    for (let i = 0; i < arr.length; i++) {
        result.push(fn(arr[i], i));
    }
    return result;
}

export function filter<T>(arr: T[], predicate: (item: T) => boolean): T[] {
    const result: T[] = [];
    for (const item of arr) {
        if (predicate(item)) {
            result.push(item);
        }
    }
    return result;
}

export function find<T>(arr: T[], predicate: (item: T) => boolean): T | undefined {
    for (const item of arr) {
        if (predicate(item)) {
            return item;
        }
    }
    return undefined;
}

export function reduce<T, U>(arr: T[], fn: (acc: U, item: T) => U, initial: U): U {
    let acc = initial;
    for (const item of arr) {
        acc = fn(acc, item);
    }
    return acc;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/array.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/array.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output has type annotations stripped
    let js = std::fs::read_to_string(base.join("dist/src/array.js")).expect("read js");
    assert!(!js.contains(": T[]"), "Type annotations should be stripped");
    assert!(!js.contains(": U[]"), "Type annotations should be stripped");
    assert!(js.contains("function map"), "Function should be present");
    assert!(js.contains("function filter"), "Function should be present");
    assert!(js.contains("function find"), "Function should be present");
    assert!(js.contains("function reduce"), "Function should be present");

    // Verify declarations preserve types
    let dts = std::fs::read_to_string(base.join("dist/src/array.d.ts")).expect("read dts");
    assert!(
        dts.contains("map<T, U>") || dts.contains("map<T,U>"),
        "Generic should be in declaration"
    );
    assert!(
        dts.contains("filter<T>"),
        "Generic should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_type_utilities() {
    // Test compilation with type-level utilities (conditional types, mapped types)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Type utilities with runtime helpers
    write_file(
        &base.join("src/types.ts"),
        r#"
// Note: Object, Readonly, Partial are provided by lib.d.ts

// Type-level utilities (erased at runtime)
export type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? Readonly<T[P]> : T[P];
};

export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? Partial<T[P]> : T[P];
};

export type Nullable<T> = T | null;

// Mapped type that uses index access (T[P])
export type ValueTypes<T> = {
    [P in keyof T]: T[P];
};

// Runtime function using these types
export function deepFreeze<T extends object>(obj: T): DeepReadonly<T> {
    Object.freeze(obj);
    for (const key of Object.keys(obj)) {
        const value = (obj as Record<string, unknown>)[key];
        if (typeof value === "object" && value !== null) {
            deepFreeze(value as object);
        }
    }
    return obj as DeepReadonly<T>;
}

export function isNonNull<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/types.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/types.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output - type aliases should be completely erased
    let js = std::fs::read_to_string(base.join("dist/src/types.js")).expect("read js");
    assert!(!js.contains("DeepReadonly"), "Type alias should be erased");
    assert!(!js.contains("DeepPartial"), "Type alias should be erased");
    assert!(
        js.contains("function deepFreeze"),
        "Runtime function should be present"
    );
    assert!(
        js.contains("function isNonNull"),
        "Runtime function should be present"
    );

    // Verify declarations preserve type utilities
    let dts = std::fs::read_to_string(base.join("dist/src/types.d.ts")).expect("read dts");
    assert!(
        dts.contains("DeepReadonly"),
        "Type alias should be in declaration"
    );
    assert!(
        dts.contains("DeepPartial"),
        "Type alias should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_multi_file() {
    // Test multi-file generic utility library with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function first<T>(arr: T[]): T | undefined {
    return arr[0];
}

export function last<T>(arr: T[]): T | undefined {
    return arr[arr.length - 1];
}
"#,
    );

    // String utilities
    write_file(
        &base.join("src/string.ts"),
        r#"
export function capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function repeat(str: string, count: number): string {
    let result = "";
    for (let i = 0; i < count; i++) {
        result += str;
    }
    return result;
}
"#,
    );

    // Function utilities
    write_file(
        &base.join("src/function.ts"),
        r#"
export function identity<T>(value: T): T {
    return value;
}

export function constant<T>(value: T): () => T {
    return () => value;
}

export function noop(): void {}
"#,
    );

    // Main index re-exporting everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export { first, last } from "./array";
export { capitalize, repeat } from "./string";
export { identity, constant, noop } from "./function";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    // All JS files should exist
    assert!(base.join("dist/src/array.js").is_file());
    assert!(base.join("dist/src/string.js").is_file());
    assert!(base.join("dist/src/function.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    // All declaration files should exist
    assert!(base.join("dist/src/array.d.ts").is_file());
    assert!(base.join("dist/src/string.d.ts").is_file());
    assert!(base.join("dist/src/function.d.ts").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // All source maps should exist
    assert!(base.join("dist/src/array.js.map").is_file());
    assert!(base.join("dist/src/index.js.map").is_file());

    // Verify index re-exports
    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read index");
    assert!(
        index_js.contains("require") || index_js.contains("export"),
        "Index should have exports"
    );

    // Verify index declaration
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("first") && index_dts.contains("last"),
        "Index declaration should re-export array utils"
    );
}

#[test]
fn compile_generic_utility_library_with_constraints() {
    // Test generic functions with complex constraints
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/constrained.ts"),
        r#"
// Generic with extends constraint
export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

// Generic with multiple constraints
export function setProperty<T extends object, K extends keyof T>(
    obj: T,
    key: K,
    value: T[K]
): T {
    obj[key] = value;
    return obj;
}

// Generic with default type parameter
export function createArray<T = string>(length: number, fill: T): T[] {
    const result: T[] = [];
    for (let i = 0; i < length; i++) {
        result.push(fill);
    }
    return result;
}

// Function overloads with generics
export function wrap<T>(value: T): T[];
export function wrap<T>(value: T, count: number): T[];
export function wrap<T>(value: T, count: number = 1): T[] {
    const result: T[] = [];
    for (let i = 0; i < count; i++) {
        result.push(value);
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/constrained.js")).expect("read js");
    assert!(
        !js.contains("extends keyof"),
        "Constraints should be stripped"
    );
    assert!(
        !js.contains("extends object"),
        "Constraints should be stripped"
    );
    assert!(
        js.contains("function getProperty"),
        "Function should be present"
    );
    assert!(js.contains("function wrap"), "Function should be present");

    let dts = std::fs::read_to_string(base.join("dist/src/constrained.d.ts")).expect("read dts");
    // Check that generic functions are present in declaration
    assert!(
        dts.contains("getProperty"),
        "getProperty should be in declaration"
    );
    assert!(
        dts.contains("setProperty"),
        "setProperty should be in declaration"
    );
    assert!(
        dts.contains("createArray"),
        "createArray should be in declaration"
    );
    assert!(dts.contains("wrap"), "wrap should be in declaration");
}

#[test]
fn compile_generic_utility_library_classes() {
    // Test generic utility classes
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/collections.ts"),
        r#"
export class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    get size(): number {
        return this.items.length;
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

export class Queue<T> {
    private items: T[] = [];

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | undefined {
        return this.items.shift();
    }

    front(): T | undefined {
        return this.items[0];
    }

    get size(): number {
        return this.items.length;
    }
}

export class Result<T, E> {
    private constructor(
        private readonly value: T | undefined,
        private readonly error: E | undefined,
        private readonly isOk: boolean
    ) {}

    static ok<T, E>(value: T): Result<T, E> {
        return new Result<T, E>(value, undefined, true);
    }

    static err<T, E>(error: E): Result<T, E> {
        return new Result<T, E>(undefined, error, false);
    }

    isSuccess(): boolean {
        return this.isOk;
    }

    getValue(): T | undefined {
        return this.value;
    }

    getError(): E | undefined {
        return this.error;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/collections.js")).expect("read js");
    assert!(js.contains("class Stack"), "Class should be present");
    assert!(js.contains("class Queue"), "Class should be present");
    assert!(js.contains("class Result"), "Class should be present");
    assert!(!js.contains("<T>"), "Generic parameters should be stripped");
    assert!(!js.contains("T[]"), "Type annotations should be stripped");
    assert!(
        !js.contains(": void"),
        "Return type annotations should be stripped"
    );

    let dts = std::fs::read_to_string(base.join("dist/src/collections.d.ts")).expect("read dts");
    assert!(
        dts.contains("Stack<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Queue<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Result<T, E>") || dts.contains("Result<T,E>"),
        "Generic class should be in declaration"
    );
}

// =============================================================================
// E2E: Module Re-exports
// =============================================================================

#[test]
fn compile_module_named_reexports() {
    // Test named re-exports: export { foo, bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { add, multiply, PI } from "./utils";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );
    assert!(base.join("dist/src/utils.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify index re-exports
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("add"), "add should be re-exported");
    assert!(
        index_dts.contains("multiply"),
        "multiply should be re-exported"
    );
    assert!(index_dts.contains("PI"), "PI should be re-exported");
}

#[test]
fn compile_module_renamed_reexports() {
    // Test renamed re-exports: export { foo as bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/internal.ts"),
        r#"
export function internalHelper(): string {
    return "helper";
}

export const internalValue = 42;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { internalHelper as helper, internalValue as value } from "./internal";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("helper"), "helper should be re-exported");
    assert!(index_dts.contains("value"), "value should be re-exported");
}

#[test]
fn compile_module_star_reexports() {
    // Test star re-exports: export * from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/math.ts"),
        r#"
export function sum(arr: number[]): number {
    let total = 0;
    for (const n of arr) {
        total += n;
    }
    return total;
}

export function average(arr: number[]): number {
    return sum(arr) / arr.length;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export * from "./math";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("sum") || index_dts.contains("*"),
        "sum should be re-exported or star export present"
    );
}

#[test]
fn wildcard_reexport_collision_emits_ts2308() {
    // When two modules both export the same name and a third does `export * from` both,
    // TS2308 must be reported. Verify the rule is structural and not name-sensitive
    // by testing with two different exported names.
    for exported_name in ["value", "result"] {
        let temp = TempDir::new().expect("temp dir");
        let base = &temp.path;

        write_file(
            &base.join("tsconfig.json"),
            r#"{"compilerOptions":{"module":"commonjs","noEmit":true},"include":["*.ts"]}"#,
        );
        write_file(
            &base.join("a.ts"),
            &format!("export const {exported_name} = 1;\n"),
        );
        write_file(
            &base.join("b.ts"),
            &format!("export const {exported_name} = 2;\n"),
        );
        write_file(
            &base.join("index.ts"),
            "export * from './a';\nexport * from './b';\n",
        );

        let args = default_args();
        let result = compile(&args, base).expect("compile should succeed");

        assert!(
            result.diagnostics.iter().any(|d| d.code == 2308),
            "Expected TS2308 for collision on '{exported_name}', got: {:?}",
            result.diagnostics
        );
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| !d.message_text.contains("escape")),
            "Global lib symbol 'escape' must not appear in TS2308 diagnostics"
        );
    }
}

#[test]
fn wildcard_reexport_no_collision_no_ts2308() {
    // Three modules with disjoint exports, all star-re-exported from an index.
    // Global lib symbols like `escape` must not cause spurious TS2308 diagnostics
    // even though they are visible in every file's scope.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{"compilerOptions":{"module":"commonjs","noEmit":true},"include":["*.ts"]}"#,
    );
    write_file(&base.join("a.ts"), "export const alpha = 1;\n");
    write_file(&base.join("b.ts"), "export const beta = 2;\n");
    write_file(&base.join("c.ts"), "export const gamma = 3;\n");
    write_file(
        &base.join("index.ts"),
        "export * from './a';\nexport * from './b';\nexport * from './c';\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics for non-colliding star re-exports, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_module_chained_reexports() {
    // Test chained re-exports: A re-exports from B which re-exports from C
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Level 3: core module
    write_file(
        &base.join("src/core.ts"),
        r#"
export function coreFunction(): string {
    return "core";
}

export const CORE_VERSION = "1.0.0";
"#,
    );

    // Level 2: intermediate module
    write_file(
        &base.join("src/intermediate.ts"),
        r#"
export { coreFunction, CORE_VERSION } from "./core";

export function intermediateFunction(): string {
    return "intermediate";
}
"#,
    );

    // Level 1: public module
    write_file(
        &base.join("src/index.ts"),
        r#"
export { coreFunction, CORE_VERSION, intermediateFunction } from "./intermediate";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/core.js").is_file());
    assert!(base.join("dist/src/intermediate.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("coreFunction"),
        "coreFunction should be re-exported"
    );
    assert!(
        index_dts.contains("intermediateFunction"),
        "intermediateFunction should be re-exported"
    );
}

