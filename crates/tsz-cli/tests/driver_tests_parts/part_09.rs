#[test]
fn compile_for_of_loop() {
    // Test for...of loop compilation
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

    write_file(
        &base.join("src/forof.ts"),
        r#"
export function sumArray(arr: number[]): number {
    let sum = 0;
    for (const num of arr) {
        sum += num;
    }
    return sum;
}

export function joinStrings(arr: string[]): string {
    let result = "";
    for (const str of arr) {
        result += str;
    }
    return result;
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

    let js = std::fs::read_to_string(base.join("dist/src/forof.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_shorthand_methods() {
    // Test shorthand method syntax compilation
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

    write_file(
        &base.join("src/methods.ts"),
        r#"
export const calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    }
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/methods.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_incremental_creates_tsbuildinfo() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Setup tsconfig with incremental enabled
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "incremental": true,
            "tsBuildInfoFile": "dist/project.tsbuildinfo"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();

    // First compilation should create BuildInfo
    let result = compile(&args, base).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    // Verify JS output exists
    let js_path = base.join("dist/src/index.js");
    assert!(js_path.is_file(), "JS output should exist");

    // Verify BuildInfo file is created
    let build_info_path = base.join("dist/project.tsbuildinfo");
    assert!(
        build_info_path.is_file(),
        "tsbuildinfo file should be created"
    );

    // Verify BuildInfo can be parsed
    let build_info_content = std::fs::read_to_string(&build_info_path).expect("read buildinfo");
    let build_info: serde_json::Value =
        serde_json::from_str(&build_info_content).expect("parse buildinfo");

    // Verify structure
    assert_eq!(
        build_info["version"],
        crate::incremental::BUILD_INFO_VERSION
    );
    assert!(build_info["rootFiles"].is_array());

    // Second build with no changes should succeed
    let result2 = compile(&args, base).expect("second compile should succeed");
    assert!(result2.diagnostics.is_empty());

    // Verify BuildInfo still exists and has been updated
    let build_info_content2 =
        std::fs::read_to_string(&build_info_path).expect("read buildinfo again");
    let build_info2: serde_json::Value =
        serde_json::from_str(&build_info_content2).expect("parse buildinfo again");
    assert_eq!(
        build_info2["version"],
        crate::incremental::BUILD_INFO_VERSION
    );

    // Third build with a source change
    write_file(
        &base.join("src/index.ts"),
        "export const value = 2; export const foo = 'bar';",
    );
    let result3 = compile(&args, base).expect("third compile should succeed");
    assert!(result3.diagnostics.is_empty());

    // Verify BuildInfo was updated with new content
    let build_info_content3 =
        std::fs::read_to_string(&build_info_path).expect("read buildinfo third time");
    let build_info3: serde_json::Value =
        serde_json::from_str(&build_info_content3).expect("parse buildinfo third time");
    assert_eq!(
        build_info3["version"],
        crate::incremental::BUILD_INFO_VERSION
    );
}

#[cfg(unix)]
#[test]
fn compile_incremental_reports_ts5033_when_tsbuildinfo_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let readonly_dir = base.join("readonly");
    std::fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o555))
        .expect("mark readonly dir");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "incremental": true,
            "tsBuildInfoFile": "readonly/project.tsbuildinfo"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed with diagnostic");

    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    let ts5033_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::COULD_NOT_WRITE_FILE)
        .collect();
    assert!(
        ts5033_diags.iter().any(|diag| {
            diag.message_text.contains("readonly/project.tsbuildinfo")
                && (diag.message_text.contains("permission denied")
                    || diag.message_text.contains("read-only file system"))
        }),
        "Expected TS5033 for non-writable tsbuildinfo path, got: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn compile_tsbuildinfo_without_incremental_does_not_report_ts5033() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let readonly_dir = base.join("readonly");
    std::fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o555))
        .expect("mark readonly dir");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "tsBuildInfoFile": "readonly/project.tsbuildinfo"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::COULD_NOT_WRITE_FILE),
        "Expected no TS5033 when incremental build info is disabled, got: {result:?}"
    );
}

#[test]
fn test_no_types_and_symbols_directive_does_not_disable_default_libs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"// @noTypesAndSymbols: true
const value = 1;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2318_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE)
        .collect();
    assert!(
        ts2318_errors.is_empty(),
        "Expected @noTypesAndSymbols not to disable libs, got TS2318 diagnostics: {:?}",
        ts2318_errors
            .iter()
            .map(|d| d.message_text.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_no_types_and_symbols_tsconfig_disables_automatic_node_types() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "declaration": true,
            "emitDeclarationOnly": true,
            "noTypesAndSymbols": true
          },
          "files": ["usage1.ts", "usage2.ts", "usage3.ts"]
        }"#,
    );
    write_file(
        &base.join("usage1.ts"),
        r#"export { parse } from "url";
"#,
    );
    write_file(
        &base.join("usage2.ts"),
        r#"import { parse } from "url";
export const thing: import("url").Url = parse();
"#,
    );
    write_file(
        &base.join("usage3.ts"),
        r#"import { parse } from "url";
export const thing = parse();
"#,
    );
    write_file(
        &base.join("node_modules/@types/node/index.d.ts"),
        r#"declare module "url" {
  export class Url {}
  export function parse(): Url;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2591_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2
        })
        .collect();
    assert!(
        ts2591_errors.len() == 4,
        "Expected noTypesAndSymbols tsconfig to suppress automatic @types/node loading and emit four TS2591 diagnostics, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_binary_file_reports_errors() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"G@\xFFG@\xFFG@";
    std::fs::write(&binary_path, content).expect("failed to write binary file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let has_ts1490 = result.diagnostics.iter().any(|d| d.code == 1490);
    assert!(
        has_ts1490,
        "Expected TS1490 (File appears to be binary). Diagnostics: {:?}",
        result.diagnostics
    );

    // Binary file detection should suppress parser diagnostics - only TS1490 is emitted
    let non_binary_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code != 1490)
        .collect();
    assert!(
        non_binary_errors.is_empty(),
        "Expected only TS1490 for binary files, but got additional errors: {non_binary_errors:?}"
    );
}

#[test]
fn compile_control_byte_binary_file_preserves_parser_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"G@\x04\x04\x04\x04\x04";
    std::fs::write(&binary_path, content).expect("failed to write control-byte file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1490),
        "Expected TS1490 for control-byte binary. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&1127),
        "Expected TS1127 to be preserved for control-byte binary. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_short_garbage_payload_binary_suppresses_parser_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"// @target: es2015\n\xEF\xBF\xBD\x1F\xEF\xBF\xBD\x03\xEF\xBF\xBD\x03\x19\x1F";
    std::fs::write(&binary_path, content).expect("failed to write corrupted file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1490],
        "Expected only TS1490 for short garbage binary payloads. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_import_alias_assignment_does_not_leak_non_exported_module_symbols() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true
          },
          "files": [
            "aliasUsageInVarAssignment_backbone.ts",
            "aliasUsageInVarAssignment_moduleA.ts",
            "aliasUsageInVarAssignment_main.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_backbone.ts"),
        r#"export class Model {
    public someData: string;
}
"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_moduleA.ts"),
        r#"import Backbone = require("./aliasUsageInVarAssignment_backbone");
export class VisualizationModel extends Backbone.Model {
    // interesting stuff here
}
"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_main.ts"),
        r#"import Backbone = require("./aliasUsageInVarAssignment_backbone");
import moduleA = require("./aliasUsageInVarAssignment_moduleA");
interface IHasVisualizationModel {
    VisualizationModel: typeof Backbone.Model;
}
var i: IHasVisualizationModel;
var m: typeof moduleA = i;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let mut codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();

    assert_eq!(
        codes,
        vec![2454, 2564],
        "Expected only TS2454 and TS2564 for alias usage assignment. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|diag| diag.code != 2740),
        "Expected no TS2740 namespace-shape diagnostic leakage. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn ts2688_unresolved_types_in_tsconfig() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    // Create a type root directory so default_type_roots finds it,
    // but don't create the requested package inside it
    std::fs::create_dir_all(base.join("node_modules/@types")).unwrap();
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "types": ["nonexistent-package"] }, "files": ["index.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for unresolved 'nonexistent-package' in types array, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    assert!(
        ts2688_diags[0].message_text.contains("nonexistent-package"),
        "TS2688 message should mention the package name, got: {}",
        ts2688_diags[0].message_text
    );
}

#[test]
fn cli_types_reports_unresolved_type_package() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    std::fs::create_dir_all(base.join("node_modules/@types")).unwrap();
    write_file(&base.join("index.ts"), "const x: number = 1;\n");

    let args = CliArgs::try_parse_from(["tsz", "--types", "nonexistent-package", "index.ts"])
        .expect("CLI args should parse");
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for unresolved CLI --types package, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn cli_type_roots_prevents_parent_at_types_discovery() {
    let tmp = TempDir::new().unwrap();
    let parent = &tmp.path;
    let base = parent.join("app");

    write_file(
        &parent.join("node_modules/@types/leaky/index.d.ts"),
        "declare const leakedGlobal: number;\n",
    );
    std::fs::create_dir_all(base.join("empty-types")).expect("empty typeRoots");
    write_file(&base.join("index.ts"), "leakedGlobal;\n");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--typeRoots",
        "./empty-types",
        "index.ts",
    ])
    .expect("CLI args should parse");
    let result = compile(&args, &base).expect("compile should succeed");

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        !ts2304_diags.is_empty(),
        "Expected CLI --typeRoots to hide parent @types globals, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn ts2688_resolved_types_no_error() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    // Create a valid @types package structure
    write_file(
        &base.join("node_modules/@types/mylib/index.d.ts"),
        "declare const myLibValue: string;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "types": ["mylib"] }, "files": ["index.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        ts2688_diags.is_empty(),
        "Should NOT emit TS2688 when types package is found, got: {ts2688_diags:?}"
    );
}

#[test]
fn tsconfig_types_resolves_node_modules_package_subpath_declaration() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "allowJs": false,
            "module": "esnext",
            "moduleResolution": "bundler",
            "target": "es2022",
            "types": ["vite/client"],
            "noEmit": true
          },
          "files": ["src/main.ts"]
        }"#,
    );
    write_file(
        &base.join("node_modules/vite/package.json"),
        r#"{ "name": "vite", "version": "1.0.0" }"#,
    );
    write_file(
        &base.join("node_modules/vite/client.d.ts"),
        r#"declare module "*.css" {}
declare module "*.svg" {
  const src: string;
  export default src;
}
declare module "*.png" {
  const src: string;
  export default src;
}
"#,
    );
    write_file(
        &base.join("src/main.ts"),
        r#"import "./style.css";
import viteLogo from "./assets/vite.svg";
import heroImg from "./assets/hero.png";

viteLogo;
heroImg;
"#,
    );
    write_file(&base.join("src/style.css"), "");
    write_file(&base.join("src/assets/vite.svg"), "<svg></svg>");
    write_file(&base.join("src/assets/hero.png"), "");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected vite/client type subpath and asset modules to resolve, got: {result:?}"
    );
}

#[test]
fn ts2688_types_entry_still_loads_node_modules_package_globals() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("typings/dummy.d.ts"),
        "declare const dummy: number;\n",
    );
    write_file(
        &base.join("node_modules/phaser/types/phaser.d.ts"),
        "declare const phaserValue: number;\n",
    );
    write_file(
        &base.join("node_modules/phaser/package.json"),
        r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["typings"],
            "types": ["phaser"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "phaserValue;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 when typeRoots does not contain the requested package, got: {:?}",
        result.diagnostics
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Node-modules fallback should still make package globals visible, got: {ts2304_diags:?}"
    );
}

#[test]
fn scoped_types_entry_resolves_plain_mangled_package_name_from_custom_roots() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/mangled__nodemodulescache/index.d.ts"),
        "declare const mangledNodeModules: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["types", "node_modules", "node_modules/@types"],
            "types": ["@mangled/nodemodulescache"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "mangledNodeModules;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for the unresolved scoped types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected scoped mangled package name to resolve from custom roots, got: {result:?}"
    );
}

#[test]
fn scoped_types_entry_loads_at_types_scoped_package_globals_while_preserving_ts2688() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@types/@scoped/attypescache/index.d.ts"),
        "declare const atTypesCache: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["types", "node_modules", "node_modules/@types"],
            "types": ["@scoped/attypescache"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "atTypesCache;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for the unresolved scoped @types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected scoped @types package globals to load despite TS2688, got: {result:?}"
    );
}

#[test]
fn type_query_on_import_type_value_binding_does_not_emit_ts2552() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@types/foo/package.json"),
        r#"{
          "name": "@types/foo",
          "version": "1.0.0",
          "exports": {
            ".": {
              "import": "./index.d.mts",
              "require": "./index.d.cts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/@types/foo/index.d.mts"),
        "export declare const x: \"module\";\n",
    );
    write_file(
        &base.join("node_modules/@types/foo/index.d.cts"),
        "export declare const x: \"script\";\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "moduleResolution": "bundler",
            "declaration": true,
            "emitDeclarationOnly": true
          },
          "files": ["app.ts", "other.ts"]
        }"#,
    );
    write_file(
        &base.join("app.ts"),
        r#"import type { x as Default } from "foo";
import type { x as ImportRelative } from "./other" with { "resolution-mode": "import" };

type _Default = typeof Default;
type _ImportRelative = typeof ImportRelative;

export { _Default, _ImportRelative };
"#,
    );
    write_file(&base.join("other.ts"), r#"export const x = "other";"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        ts2552_diags.is_empty(),
        "Expected typeof on import type bindings to avoid TS2552, got: {result:?}"
    );
}

#[test]
fn import_type_resolution_mode_declaration_emit_uses_exact_package_condition() {
    for module_kind in ["node16", "node18", "node20", "nodenext"] {
        let tmp = TempDir::new().unwrap();
        let base = &tmp.path;

        write_file(
            &base.join("node_modules/pkg/package.json"),
            r#"{
          "name": "pkg",
          "version": "0.0.1",
          "exports": {
            "import": "./import.js",
            "require": "./require.js"
          }
        }"#,
        );
        write_file(
            &base.join("node_modules/pkg/import.d.ts"),
            "export interface ImportInterface {}\n",
        );
        write_file(
            &base.join("node_modules/pkg/require.d.ts"),
            "export interface RequireInterface {}\n",
        );
        write_file(
            &base.join("tsconfig.json"),
            &r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "__MODULE_KIND__",
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "out"
          },
          "files": ["index.ts"]
        }"#
            .replace("__MODULE_KIND__", module_kind),
        );
        write_file(
            &base.join("index.ts"),
            r#"export type LocalInterface =
    & import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface
    & import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface;

export const a = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
export const b = (null as any as import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface);
"#,
        );

        let args = default_args();
        let result = compile(&args, base).expect("compile should succeed");

        let ts2694_diags: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER)
            .collect();
        assert!(
            ts2694_diags.is_empty(),
            "Did not expect TS2694 under module {module_kind} when import types use distinct resolution-mode conditions, got: {result:?}"
        );
    }
}

#[test]
fn export_type_resolution_mode_declaration_emit_does_not_emit_alias_ts2305() {
    for root_package_json in [
        None,
        Some(
            r#"{
          "private": true,
          "type": "module"
        }"#,
        ),
    ] {
        let tmp = TempDir::new().unwrap();
        let base = &tmp.path;

        write_file(
            &base.join("node_modules/pkg/package.json"),
            r#"{
          "name": "pkg",
          "version": "0.0.1",
          "exports": {
            "import": "./import.js",
            "require": "./require.js"
          }
        }"#,
        );
        write_file(
            &base.join("node_modules/pkg/import.d.ts"),
            "export interface ImportInterface {}\n",
        );
        write_file(
            &base.join("node_modules/pkg/require.d.ts"),
            "export interface RequireInterface {}\n",
        );
        if let Some(package_json) = root_package_json {
            write_file(&base.join("package.json"), package_json);
        }
        write_file(
            &base.join("tsconfig.json"),
            r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "out"
          },
          "files": ["index.ts"]
        }"#,
        );
        write_file(
            &base.join("index.ts"),
            r#"export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
export type { ImportInterface } from "pkg" with { "resolution-mode": "import" };
"#,
        );

        let args = default_args();
        let result = compile(&args, base).expect("compile should succeed");

        let ts2305_diags: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
            .collect();
        assert!(
            ts2305_diags.is_empty(),
            "Did not expect generic alias TS2305 for export type resolution-mode declaration emit, got: {ts2305_diags:?}\nall diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn import_non_exported_member_alias_reports_ts2460() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"declare function foo(): any
declare function bar(): any;
export { foo, bar as baz };
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import { foo, bar } from "./a";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2460_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS)
        .collect();
    assert!(
        ts2460_diags.iter().any(|diag| {
            diag.message_text.contains("\"./a\"")
                && diag.message_text.contains("'bar'")
                && diag.message_text.contains("'baz'")
        }),
        "Expected TS2460 for renamed export import, got: {result:?}"
    );
}

#[test]
fn direct_export_with_separate_type_alias_does_not_report_ts2460() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"export class A<T> { a!: T }
export type { A as B };
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import type { A } from "./a";
import { B } from "./a";

let a: A<string> = { a: "" };
let b: B<number> = { a: 3 };
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS),
        "Did not expect TS2460 for direct export plus type-only alias, got: {result:?}"
    );
}

#[test]
fn bare_import_type_reports_ts1340() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["test.ts", "main.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"export interface T {
    value: string
}
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"export const a: import("./test") = null;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1340_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1340)
        .collect();
    assert!(
        ts1340_diags.iter().any(|diag| {
            diag.message_text
                .contains("Module './test' does not refer to a type")
                && diag.message_text.contains("typeof import('./test')")
        }),
        "Expected TS1340 for bare import type, got: {result:?}"
    );
}

#[test]
fn declaration_emit_raw_typeof_import_text_still_reports_ts9006() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;
    let raw_specifier = base.join("some-mod").display().to_string();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "declaration": true,
    "emitDeclarationOnly": true,
    "module": "commonjs",
    "target": "es2020",
    "types": []
  },
  "files": ["some-mod.d.ts", "index.js", "index-comment.js"]
}"#,
    );
    write_file(
        &base.join("some-mod.d.ts"),
        r#"
interface Item {
  x: string;
}

declare function getItems(): Item[];
export = getItems;
"#,
    );
    write_file(
        &base.join("index.js"),
        &format!(
            r#"// @ts-check
const items = require("./some-mod")();
const note = 'typeof import("{raw_specifier}")';

module.exports = items;
"#
        ),
    );
    write_file(
        &base.join("index-comment.js"),
        &format!(
            r#"// @ts-check
// typeof import("{raw_specifier}")
const items = require("./some-mod")();

module.exports = items;
"#
        ),
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts9006: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 9006)
        .collect();
    assert_eq!(
        ts9006.len(),
        2,
        "raw string/comment import text must not suppress TS9006, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_declaration_emit_self_referential_prototype_method_type_does_not_recurse() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "declaration": true,
    "outDir": "out",
    "module": "commonjs",
    "target": "es2015",
    "strict": false
  },
  "files": ["source.js", "referencer.js"]
}"#,
    );
    write_file(
        &base.join("source.js"),
        r#"/** @param {number} len */
export function Vec(len) {
  /** @type {number[]} */
  this.storage = new Array(len);
}

Vec.prototype = {
  /** @param {Vec} other */
  dot(other) {
    return other.storage.length;
  }
};
"#,
    );
    write_file(
        &base.join("referencer.js"),
        r#"import { Vec } from "./source";
export const vec = new Vec(1);
"#,
    );

    let args = default_args();
    compile(&args, base).expect("compile should succeed");

    let dts = std::fs::read_to_string(base.join("out/source.d.ts"))
        .expect("source declaration should be emitted");
    assert!(
        dts.contains("dot(other: Vec): number;"),
        "expected self-referential prototype method parameter to print by name: {dts}"
    );
}

#[test]
fn jsdoc_property_typedef_declaration_emit_quotes_non_identifier_names() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "dist"
  },
  "include": ["index.js"]
}"#,
    );
    write_file(
        &base.join("index.js"),
        r#"/**
 * @typedef {Object} Options
 * @property {string} data-id
 */
exports.value = {};
"#,
    );

    let args = default_args();
    compile(&args, base).expect("compile should succeed");

    let dts = std::fs::read_to_string(base.join("dist/index.d.ts"))
        .expect("index declaration should be emitted");
    assert!(
        dts.contains("\"data-id\": string;"),
        "expected JSDoc property name requiring quotes to emit a valid string-literal property: {dts}"
    );
    assert!(
        !dts.contains("data-id: string;"),
        "expected invalid unquoted hyphenated property name to be absent: {dts}"
    );
}

#[test]
fn js_commonjs_keyword_named_exports_emit_valid_declarations() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "dist",
    "module": "commonjs",
    "target": "es2022",
    "strict": true,
    "skipLibCheck": true
  },
  "files": ["index.js"]
}"#,
    );
    write_file(
        &base.join("index.js"),
        r#"exports.class = 123;
exports.for = "loop";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    let dts = std::fs::read_to_string(base.join("dist/index.d.ts"))
        .expect("index declaration should be emitted");
    assert!(
        dts.contains("declare const _class: 123;"),
        "expected keyword export to use a local alias: {dts}"
    );
    assert!(
        dts.contains("declare const _for: \"loop\";"),
        "expected keyword export to use a local alias: {dts}"
    );
    assert!(
        dts.contains("export { _class as class, _for as for };"),
        "expected keyword exports to be re-exported by alias: {dts}"
    );
    assert!(
        !dts.contains("export const class"),
        "expected invalid keyword binding declaration to be absent: {dts}"
    );
    assert!(
        !dts.contains("export const for"),
        "expected invalid keyword binding declaration to be absent: {dts}"
    );
}

#[test]
fn bare_import_type_export_equals_class_does_not_report_ts1340() {
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
  "files": ["foo.ts", "usage.ts"]
}"#,
    );
    write_file(
        &base.join("foo.ts"),
        r#"class Conn {
    item = 3;
}

export = Conn;
"#,
    );
    write_file(
        &base.join("usage.ts"),
        r#"type Conn = import("./foo");
declare const x: Conn;
export const y = x.item;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|d| d.code == 1340),
        "Did not expect TS1340 for bare import type of export= class module, got: {result:?}"
    );
}

#[test]
fn checked_js_async_jsdoc_promise_prefixed_alias_reports_ts1064() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

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
  "files": ["main.js"]
}"#,
    );
    write_file(
        &base.join("main.js"),
        r#"// @ts-check

/**
 * @template T
 * @typedef {{ value: T }} PromiseButNot
 */

/** @type {function(): Promise<string>} */
const ok = async () => "ok";

/** @type {function(): PromiseButNot<string>} */
const f = async () => "ok";

ok;
f;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1064: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1064)
        .collect();
    assert_eq!(
        ts1064.len(),
        1,
        "expected TS1064 only for PromiseButNot, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts1064[0].message_text.contains("PromiseButNot<string>"),
        "expected TS1064 to suggest wrapping PromiseButNot<string>, got: {:?}",
        ts1064[0]
    );
}

#[test]
fn checked_js_async_jsdoc_shadowed_promise_typedef_reports_ts1064() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

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
  "files": ["main.js"]
}"#,
    );
    write_file(
        &base.join("main.js"),
        r#"// @ts-check
export {};

/**
 * @template T
 * @typedef {{ value: T }} Promise
 */

/** @type {function(): Promise<string>} */
const f = async () => "ok";

f;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == 1064 && d.message_text.contains("Promise<Promise<string>>")),
        "expected TS1064 for shadowed Promise typedef, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2322),
        "expected assignment mismatch alongside TS1064, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_external_module_typedef_does_not_suppress_generic_arg_ts2304() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "strict": true,
    "noEmit": true,
    "module": "esnext",
    "typeRoots": ["./empty-types"]
  },
  "files": ["a.js", "b.js"]
}"#,
    );
    write_file(&base.join("empty-types/.keep"), "");
    write_file(
        &base.join("a.js"),
        r#"// @ts-check
/** @typedef {Array<Missing>} A */
export {};
"#,
    );
    write_file(
        &base.join("b.js"),
        r#"// @ts-check
/** @typedef {number} Missing */
export {};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let missing_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == diagnostic_codes::CANNOT_FIND_NAME
                && diagnostic.message_text.contains("'Missing'")
        })
        .collect();

    assert_eq!(
        missing_diags.len(),
        1,
        "Expected TS2304 for unimported JSDoc typedef in another external module, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_typeof_import_type_query_non_literal_reports_ts1141() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "noEmit": true,
    "types": []
  },
  "files": ["index.ts"]
}"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
type ImportByKey<K extends string> = typeof import(K);
type MappedImport<T extends string[]> = {
    [K in T[number]]: typeof import(K);
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let ts1141_count = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::STRING_LITERAL_EXPECTED)
        .count();

    assert_eq!(
        ts1141_count, 2,
        "Expected TS1141 for both typeof import(K) type queries, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_jsdoc_import_backtick_reports_ts1141_in_project_mode() {
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
  "files": ["index.js", "dep.d.ts"]
}"#,
    );
    write_file(
        &base.join("dep.d.ts"),
        r#"export interface Foo {
  x: string;
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check

/** @type {import(`./dep`).Foo} */
const value = { x: "ok" };

value.x.toUpperCase();
value.y;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::STRING_LITERAL_EXPECTED),
        "Expected TS1141 for backtick JSDoc import type, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Invalid JSDoc import syntax should not resolve Foo and report downstream TS2339, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn checked_js_jsdoc_import_string_literal_export_names_resolve() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "types": []
  },
  "files": ["index.js", "dep.d.ts"]
}"#,
    );
    write_file(
        &base.join("dep.d.ts"),
        r#"export declare const value: number;
export { value as "a,b" };
export { value as "as" };
export { value as "from" };
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check
/** @import { "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep" */
/** @type {CommaName} */
const a = "x";
/** @type {AsName} */
const b = "x";
/** @type {FromName} */
const c = "x";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    let assignability_count = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        assignability_count, 3,
        "Expected three TS2322 diagnostics from resolved JSDoc imports, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "String-literal JSDoc import aliases should resolve, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
            && !codes.contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
            && !codes.contains(&diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN),
        "String-literal export names should not produce unresolved-name or bogus member diagnostics: {:?}",
        result.diagnostics
    );
}

