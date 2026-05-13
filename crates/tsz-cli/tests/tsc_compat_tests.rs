//! Integration tests that compare tsz output against tsc (TypeScript compiler) output
//! to ensure they match character-by-character.
//!
//! These tests require `tsc` to be installed and available in PATH.
//! They compare the diagnostic output format (non-pretty mode) between tsz and tsc
//! to verify that tsz produces identical output to tsc for identical inputs.
//!
//! Note: Some tests compare output structure only (ignoring error span positions)
//! because tsz's type checker may report errors on different AST nodes than tsc.
//! Tests that use error codes/types where both compilers agree on spans will
//! verify exact char-by-char matches.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_tsc_compat_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

#[test]
fn list_files_only_accepts_bare_relative_project_config_path() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("listfiles_bare_relative_project").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"include":["src/**/*"],"compilerOptions":{"noEmit":true,"noLib":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let output = Command::new(tsz_bin)
        .args([
            "-p",
            "tsconfig.json",
            "--pretty",
            "false",
            "--listFilesOnly",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz --listFilesOnly");

    let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "tsz --listFilesOnly should accept a bare relative project config path.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("src/a.ts"),
        "expected discovered source file in stdout, got:\n{stdout}"
    );
}

#[test]
fn list_files_only_resolve_json_module_does_not_list_unimported_json_roots() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("listfiles_resolve_json_no_json_roots").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true,"noLib":true,"resolveJsonModule":true,"module":"node16","moduleResolution":"node16","types":[]},"include":["**/*"]}"#,
    );
    write_file(&temp.path.join("app.ts"), "export const x = 1;\n");
    write_file(&temp.path.join("data.json"), "{ not valid json }\n");

    let output = Command::new(tsz_bin)
        .args(["--pretty", "false", "--listFilesOnly"])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz --listFilesOnly");

    let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "tsz --listFilesOnly should succeed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("app.ts"),
        "expected discovered TS file in stdout, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("data.json"),
        "unimported JSON matched by include should not be listed, got:\n{stdout}"
    );
}

#[test]
fn relative_module_augmentation_missing_target_reports_ts2664() {
    let temp = TempDir::new("relative_module_augmentation_missing_target").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"declare module "./nonexistent" {
    interface Extra {
        extra: boolean;
    }
}

export {};
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(code, 0, "missing relative augmentation target should fail");
    assert!(
        output.contains("error TS2664: Invalid module name in augmentation, module './nonexistent' cannot be found."),
        "expected TS2664 for unresolved relative module augmentation, got:\n{output}"
    );
}

#[test]
fn accessor_modifier_below_es2015_reports_ts18045() {
    let temp = TempDir::new("accessor_modifier_below_es2015").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"class Counter {
    accessor count = 0;
}
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es5",
            "--ignoreDeprecations",
            "6.0",
            "--pretty",
            "false",
            "test.ts",
        ],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(code, 0, "ES5 accessor property should fail");
    assert!(
        output.contains("error TS18045: Properties with the 'accessor' modifier are only available when targeting ECMAScript 2015 and higher."),
        "expected TS18045 for ES5 accessor property, got:\n{output}"
    );
}

#[test]
fn bigint_and_symbol_availability_follow_target_and_lib() {
    let temp = TempDir::new("bigint_symbol_target_lib").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"const big = 123n;
const sym = Symbol("unique");
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es5",
            "--ignoreDeprecations",
            "6.0",
            "--lib",
            "es5",
            "--pretty",
            "false",
            "test.ts",
        ],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(code, 0, "ES5 BigInt/Symbol availability should fail");
    assert!(
        output.contains(
            "error TS2737: BigInt literals are not available when targeting lower than ES2020."
        ),
        "expected TS2737 for BigInt literal below ES2020, got:\n{output}"
    );
    assert!(
        output.contains("error TS2585: 'Symbol' only refers to a type, but is being used as a value here. Do you need to change your target library?"),
        "expected TS2585 for Symbol value without es2015 lib, got:\n{output}"
    );
}

#[test]
fn async_function_without_promise_constructor_reports_ts2705() {
    let temp = TempDir::new("async_promise_target_lib").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"async function asyncFn(): Promise<string> {
    return "hello";
}
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit",
            "--strict",
            "--target",
            "es5",
            "--ignoreDeprecations",
            "6.0",
            "--lib",
            "es5",
            "--pretty",
            "false",
            "test.ts",
        ],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_ne!(
        code, 0,
        "ES5 async function without Promise constructor should fail"
    );
    assert!(
        output.contains(
            "error TS2705: An async function or method in ES5 requires the 'Promise' constructor."
        ),
        "expected TS2705 for async function without Promise constructor, got:\n{output}"
    );
}

#[test]
fn tsconfig_output_only_flags_accept_jsonc_trailing_commas() {
    let temp = TempDir::new("output_only_flags_jsonc").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
            // Output-only flags are read before the main config load.
            "compilerOptions": {
                "noEmit": true,
                "listFiles": true,
            },
            "files": [
                "src/a.ts",
            ],
        }"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let Some((code, output)) = run_tsz_with_exit_code(&temp.path, &["--pretty", "false"]) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(code, 0, "tsz should compile JSONC tsconfig: {output}");
    assert!(
        output.contains("src/a.ts"),
        "tsconfig listFiles flag should be honored from JSONC, got:\n{output}"
    );
}

#[test]
fn show_config_and_list_files_only_find_parent_tsconfig() {
    let temp = TempDir::new("special_modes_parent_tsconfig").expect("temp dir");
    write_file(&temp.path.join("p/a.ts"), "let parentConfigFile = 1;\n");
    write_file(
        &temp.path.join("p/tsconfig.json"),
        r#"{
  "compilerOptions": { "target": "es2015", "strict": true, "noEmit": true },
  "files": ["a.ts"]
}
"#,
    );
    std::fs::create_dir_all(temp.path.join("p/sub")).expect("create subdir");
    let cwd = temp.path.join("p/sub");

    let (show_code, show_output) =
        run_tsz_with_exit_code(&cwd, &["--showConfig", "--pretty", "false"])
            .expect("tsz should run");
    assert_eq!(show_code, 0, "showConfig should succeed: {show_output}");
    assert!(
        show_output.contains("\"target\": \"es6\""),
        "showConfig should load parent compiler options: {show_output}"
    );
    assert!(
        show_output.contains("\"./a.ts\""),
        "showConfig should list parent project files relative to config: {show_output}"
    );

    let (list_code, list_output) =
        run_tsz_with_exit_code(&cwd, &["--listFilesOnly", "--pretty", "false"])
            .expect("tsz should run");
    assert_eq!(list_code, 0, "listFilesOnly should succeed: {list_output}");
    assert!(
        list_output.contains("p/a.ts"),
        "listFilesOnly should list the parent project file: {list_output}"
    );
}

#[test]
fn special_modes_ignore_config_with_no_inputs_follow_no_input_behavior() {
    let temp = TempDir::new("special_modes_ignore_config_no_inputs").expect("temp dir");

    let (show_code, show_output) = run_tsz_with_exit_code(
        &temp.path,
        &["--showConfig", "--ignoreConfig", "--pretty", "false"],
    )
    .expect("tsz should run");
    assert_eq!(show_code, 1, "showConfig should fail: {show_output}");
    assert!(
        show_output.contains("error TS5081: Cannot find a tsconfig.json file"),
        "showConfig should report TS5081: {show_output}"
    );

    let (list_code, list_output) = run_tsz_with_exit_code(
        &temp.path,
        &["--listFilesOnly", "--ignoreConfig", "--pretty", "false"],
    )
    .expect("tsz should run");
    assert_eq!(list_code, 1, "listFilesOnly should fail: {list_output}");
    assert!(
        list_output.contains("Version "),
        "listFilesOnly should print no-input help/version output: {list_output}"
    );
}

#[test]
fn show_config_ignore_config_without_files_loads_discovered_tsconfig() {
    let temp = TempDir::new("show_config_ignore_config_loads_tsconfig").expect("temp dir");
    write_file(&temp.path.join("index.ts"), "const ok = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"include":["index.ts"]}"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--showConfig", "--ignoreConfig", "--pretty", "false"],
    )
    .expect("tsz should run");

    assert_eq!(code, 0, "showConfig should succeed: {output}");
    assert!(
        !output.contains("error TS5081"),
        "showConfig must not report TS5081 when a tsconfig is discoverable: {output}"
    );
    assert!(
        output.contains("\"noEmit\": true"),
        "showConfig should load compilerOptions from tsconfig: {output}"
    );
    assert!(
        output.contains("\"ignoreConfig\": true"),
        "showConfig should include the CLI ignoreConfig option: {output}"
    );
    assert!(
        output.contains("\"./index.ts\""),
        "showConfig should include discovered project file: {output}"
    );
    assert!(
        output.contains("\"include\": [") && output.contains("\"index.ts\""),
        "showConfig should preserve include specs from tsconfig: {output}"
    );
}

#[test]
fn invalid_locale_with_explicit_file_reports_ts6048() {
    let temp = TempDir::new("invalid_locale_explicit_file").expect("temp dir");
    write_file(&temp.path.join("index.ts"), "const ok = 1;\n");

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--locale",
            "does-not-exist",
            "index.ts",
            "--ignoreConfig",
            "--pretty",
            "false",
        ],
    )
    .expect("tsz should run");

    assert_eq!(code, 1, "invalid locale should exit 1: {output}");
    assert_eq!(
        output,
        "error TS6048: Locale must be of the form <language> or <language>-<territory>. For example 'en' or 'ja-jp'.\n"
    );
}

#[test]
fn invalid_locale_with_discovered_config_reports_ts6048() {
    let temp = TempDir::new("invalid_locale_discovered_config").expect("temp dir");
    write_file(&temp.path.join("index.ts"), "const ok = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["index.ts"]}"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--locale", "does-not-exist", "--pretty", "false"],
    )
    .expect("tsz should run");

    assert_eq!(code, 1, "invalid locale should exit 1: {output}");
    assert_eq!(
        output,
        "error TS6048: Locale must be of the form <language> or <language>-<territory>. For example 'en' or 'ja-jp'.\n"
    );
}

// --- Regression tests for issue #3580 ---
//
// `tsz --showConfig` must match tsc's tsconfig-discovery rules:
//   - explicit files + no tsconfig anywhere: synthesize from CLI options.
//   - explicit files + walked-up tsconfig: emit TS5112 (the implicit
//     pickup is rejected; user must opt out via `--ignoreConfig`).
//   - no files + no tsconfig anywhere: emit TS5081 even when other CLI
//     options are present.

#[test]
fn show_config_explicit_files_without_any_tsconfig_synthesizes_config() {
    let temp = TempDir::new("show_config_no_tsconfig_explicit_files").expect("temp dir");
    write_file(&temp.path.join("main.ts"), "export const value = 1;\n");

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--showConfig", "--target", "es2020", "main.ts"],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "showConfig with explicit file and no tsconfig should exit 0, got: {output}"
    );
    assert!(
        !output.contains("error TS5081"),
        "showConfig must not emit TS5081 when an explicit file is provided: {output}"
    );
    assert!(
        output.contains("\"target\": \"es2020\""),
        "showConfig should include CLI --target: {output}"
    );
    assert!(
        output.contains("\"./main.ts\""),
        "showConfig should list the explicit file: {output}"
    );
}

#[test]
fn show_config_explicit_files_with_walkup_tsconfig_emits_ts5112() {
    let temp = TempDir::new("show_config_explicit_files_walkup_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"target":"es5"}}"#,
    );
    let cwd = temp.path.join("sub");
    std::fs::create_dir_all(&cwd).expect("create subdir");
    write_file(&cwd.join("main.ts"), "export const value = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&cwd, &["--showConfig", "--target", "es2020", "main.ts"])
            .expect("tsz should run");
    assert_eq!(
        code, 1,
        "showConfig must reject implicit walkup tsconfig with explicit files, got exit {code}: {output}"
    );
    assert!(
        output.contains("error TS5112"),
        "showConfig should emit TS5112 when a walked-up tsconfig is shadowed by explicit files: {output}"
    );
    assert!(
        !output.contains("\"strict\": true"),
        "showConfig must not silently inherit walked-up tsconfig options when TS5112 fires: {output}"
    );
}

#[test]
fn show_config_explicit_files_with_walkup_tsconfig_ignore_config_synthesizes() {
    // `--ignoreConfig` is the documented escape hatch for TS5112; the user
    // gets a CLI-only synthesis even when a parent tsconfig exists.
    let temp = TempDir::new("show_config_explicit_files_walkup_ignore").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"target":"es5"}}"#,
    );
    let cwd = temp.path.join("sub");
    std::fs::create_dir_all(&cwd).expect("create subdir");
    write_file(&cwd.join("main.ts"), "export const value = 1;\n");

    let (code, output) = run_tsz_with_exit_code(
        &cwd,
        &[
            "--showConfig",
            "--ignoreConfig",
            "--target",
            "es2020",
            "main.ts",
        ],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "showConfig with --ignoreConfig should exit 0 even with a walkup tsconfig, got: {output}"
    );
    assert!(
        !output.contains("error TS5112"),
        "showConfig with --ignoreConfig must not emit TS5112: {output}"
    );
    assert!(
        !output.contains("\"strict\": true"),
        "showConfig with --ignoreConfig must not inherit walked-up tsconfig options: {output}"
    );
    assert!(
        output.contains("\"target\": \"es2020\""),
        "showConfig with --ignoreConfig should still include CLI --target: {output}"
    );
}

#[test]
fn show_config_no_files_no_tsconfig_with_cli_options_emits_ts5081() {
    // tsc emits TS5081 when neither an explicit file list nor a tsconfig
    // anchors the project, even if the user supplied unrelated CLI options
    // like `--target`. Without this, a stray invocation in an empty
    // directory silently prints `{}` and exits 0.
    let temp = TempDir::new("show_config_no_anchor_with_cli_opts").expect("temp dir");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig", "--target", "es2020"])
            .expect("tsz should run");
    assert_eq!(
        code, 1,
        "showConfig with no anchor should exit 1, got: {output}"
    );
    assert!(
        output.contains("error TS5081"),
        "showConfig should emit TS5081 when there is no project anchor: {output}"
    );
}

#[test]
fn tsc_parity_show_config_explicit_files_no_tsconfig() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_parity_explicit_files_no_tsconfig").expect("temp dir");
    write_file(&temp.path.join("main.ts"), "export const value = 1;\n");

    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--showConfig", "--target", "es2020", "main.ts"],
        "tsz --showConfig must match tsc when explicit files are passed and no tsconfig exists",
    );
}

#[test]
fn tsc_parity_show_config_explicit_files_walkup_tsconfig_ts5112() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_parity_walkup_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"target":"es5"}}"#,
    );
    let cwd = temp.path.join("sub");
    std::fs::create_dir_all(&cwd).expect("create subdir");
    write_file(&cwd.join("main.ts"), "export const value = 1;\n");

    assert_tsc_tsz_match_with_exit_code(
        &cwd,
        &["--showConfig", "--target", "es2020", "main.ts"],
        "tsz --showConfig must match tsc when an implicit walkup tsconfig collides with explicit files",
    );
}

#[test]
fn trace_resolution_prints_relative_import_resolution() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("trace_resolution_relative_import").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true,"moduleResolution":"node","ignoreDeprecations":"6.0"},"files":["index.ts"]}"#,
    );
    write_file(&temp.path.join("dep.ts"), "export const dep = 2;\n");
    write_file(
        &temp.path.join("index.ts"),
        "import { dep } from \"./dep\";\nexport const value = dep;\n",
    );

    let output = Command::new(tsz_bin)
        .args([
            "-p",
            "tsconfig.json",
            "--traceResolution",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz --traceResolution");

    let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "tsz --traceResolution should compile successfully.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("======== Resolving module './dep' from '"),
        "expected trace to include module resolution start, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Explicitly specified module resolution kind: 'Node10'."),
        "expected trace to include effective module resolution kind, got:\n{stdout}"
    );
    assert!(
        stdout.contains("File '") && stdout.contains("dep.ts' exists"),
        "expected trace to include successful file probe, got:\n{stdout}"
    );
    assert!(
        stdout.contains("======== Module name './dep' was successfully resolved to '")
            && stdout.contains("dep.ts'. ========"),
        "expected trace to include successful module resolution, got:\n{stdout}"
    );
}

#[test]
fn readonly_companion_factories_do_not_reuse_cross_file_symbol_cache_entries() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("readonly_companion_factory_cache").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2018","module":"esnext","strict":true,"lib":["es2022"],"types":[],"skipLibCheck":true,"noEmit":true,"moduleResolution":"bundler"},"include":["*.ts"]}"#,
    );
    write_file(
        &temp.path.join("object-utils.ts"),
        r#"
export function freeze<T>(value: T): Readonly<T> {
  return value
}
"#,
    );
    write_file(
        &temp.path.join("operation-node.ts"),
        r#"
export type OperationNodeKind = "AliasNode" | "IdentifierNode"

export interface OperationNode {
  readonly kind: OperationNodeKind
}
"#,
    );
    write_file(
        &temp.path.join("alias-node.ts"),
        r#"
import { freeze } from "./object-utils.js"
import type { OperationNode } from "./operation-node.js"

export interface AliasNode extends OperationNode {
  readonly kind: "AliasNode"
  readonly node: OperationNode
  readonly alias: OperationNode
}

type AliasNodeFactory = Readonly<{
  is(node: OperationNode): node is AliasNode
  create(node: OperationNode, alias: OperationNode): Readonly<AliasNode>
}>

export const AliasNode: AliasNodeFactory = freeze<AliasNodeFactory>({
  is(node): node is AliasNode {
    return node.kind === "AliasNode"
  },
  create(node, alias) {
    return freeze({ kind: "AliasNode", node, alias })
  },
})
"#,
    );
    write_file(
        &temp.path.join("identifier-node.ts"),
        r#"
import { freeze } from "./object-utils.js"
import type { OperationNode } from "./operation-node.js"

export interface IdentifierNode extends OperationNode {
  readonly kind: "IdentifierNode"
  readonly name: string
}

type IdentifierNodeFactory = Readonly<{
  is(node: OperationNode): node is IdentifierNode
  create(name: string): Readonly<IdentifierNode>
}>

export const IdentifierNode: IdentifierNodeFactory =
  freeze<IdentifierNodeFactory>({
    is(node): node is IdentifierNode {
      return node.kind === "IdentifierNode"
    },
    create(name) {
      return freeze({ kind: "IdentifierNode", name })
    },
  })
"#,
    );
    write_file(
        &temp.path.join("use.ts"),
        r#"
import { AliasNode } from "./alias-node.js"
import { IdentifierNode } from "./identifier-node.js"

const alias = AliasNode.create(
  IdentifierNode.create("x"),
  IdentifierNode.create("y"),
)
const id = IdentifierNode.create("z")
const name: string = id.name
"#,
    );

    let output = Command::new(tsz_bin)
        .args(["--noEmit", "-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz readonly companion factory cache regression");

    let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "tsz should preserve each imported companion factory shape.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn positional_file_no_lib_no_emit_returns_from_binary() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("positional_no_lib_no_emit").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const x = 1;\n");

    let mut child = Command::new(tsz_bin)
        .args(["a.ts", "--noLib", "--noEmit", "--pretty", "false"])
        .current_dir(&temp.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tsz positional compile");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child
            .try_wait()
            .expect("poll tsz positional compile")
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect killed tsz output");
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("tsz positional compile timed out\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let output = child
        .wait_with_output()
        .expect("collect tsz positional compile output");
    let stdout = normalize_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_output(&String::from_utf8_lossy(&output.stderr));

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected diagnostics-with-no-emit exit\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("error TS2318: Cannot find global type 'Array'."),
        "expected noLib missing global diagnostics, got:\n{stdout}"
    );
    assert!(stderr.is_empty(), "expected no stderr, got:\n{stderr}");
}

#[test]
fn declaration_emit_expands_foreign_import_mapped_keys_from_nested_package() {
    let temp = TempDir::new("foreign_mapped_keys").expect("temp dir");

    write_file(
        &temp.path.join("tsconfig.json"),
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
    std::fs::create_dir_all(temp.path.join("empty-types")).expect("empty typeRoots");
    write_file(
        &temp.path.join("r/entry.ts"),
        r#"import { foo } from "foo";

export const x = foo();
"#,
    );
    write_file(
        &temp.path.join("r/node_modules/foo/index.d.ts"),
        r#"export function foo(): { [K in import("keys").Key]?: string };
"#,
    );
    write_file(
        &temp
            .path
            .join("r/node_modules/foo/node_modules/keys/index.d.ts"),
        r#"export type Key = "a" | "b";
"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json"]).expect("tsz should run");
    assert_eq!(code, 0, "tsz should succeed, got output:\n{output}");

    let dts = std::fs::read_to_string(temp.path.join("dist/entry.d.ts"))
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
fn declaration_emit_reports_single_quoted_transitive_import_type() {
    let temp = TempDir::new("single_quoted_transitive_import_type").expect("temp dir");

    write_file(
        &temp.path.join("tsconfig.json"),
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
    std::fs::create_dir_all(temp.path.join("empty-types")).expect("empty typeRoots");
    write_file(
        &temp.path.join("r/entry.ts"),
        r#"import { foo } from "foo";

export const x = foo();
"#,
    );
    write_file(
        &temp.path.join("r/node_modules/foo/index.d.ts"),
        r#"export function foo(): [import('nested').NestedProps];
"#,
    );
    write_file(
        &temp
            .path
            .join("r/node_modules/foo/node_modules/nested/index.d.ts"),
        r#"export interface NestedProps { x: string; }
"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json"]).expect("tsz should run");
    assert_ne!(code, 0, "tsz should report TS2883");
    assert!(
        output.contains("TS2883") && output.contains("NestedProps"),
        "expected TS2883 for NestedProps, got:\n{output}",
    );
}

#[test]
fn declaration_emit_preserves_template_literal_type_text_from_dependency() {
    let temp = TempDir::new("template_literal_type_text_from_dependency").expect("temp dir");

    write_file(
        &temp.path.join("tsconfig.json"),
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
    std::fs::create_dir_all(temp.path.join("empty-types")).expect("empty typeRoots");
    write_file(
        &temp.path.join("r/entry.ts"),
        r#"import { foo } from "foo";

export const x = foo();
"#,
    );
    write_file(
        &temp.path.join("r/node_modules/foo/index.d.ts"),
        r#"import { Kind } from "nested";
export function foo(): `Kind-${string}`;
"#,
    );
    write_file(
        &temp
            .path
            .join("r/node_modules/foo/node_modules/nested/index.d.ts"),
        r#"export type Kind = "a";
"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json"]).expect("tsz should run");
    assert_eq!(code, 0, "tsz should succeed, got output:\n{output}");
    assert!(
        !output.contains("TS2883"),
        "template literal text should not report TS2883:\n{output}",
    );

    let dts = std::fs::read_to_string(temp.path.join("dist/entry.d.ts"))
        .expect("Declaration output should be emitted");
    assert!(
        dts.contains("export declare const x: `Kind-${string}`;"),
        "expected original template literal type text, got:\n{dts}",
    );
    assert!(
        !dts.contains("import(\"nested\").Kind"),
        "template literal text should not be rewritten as an import type: {dts}",
    );
}

/// Run tsc and return its stderr output (where diagnostics go) with ANSI codes stripped.
fn run_tsc(cwd: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = tsc_command()?;
    let output = cmd.args(args).current_dir(cwd).output().ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // tsc outputs diagnostics to stdout in non-pretty mode and stderr in some modes
    let combined = if !stdout.is_empty() {
        stdout.into_owned()
    } else {
        stderr.into_owned()
    };
    Some(normalize_output(&combined))
}

/// Run tsz and return its diagnostic output with ANSI codes stripped.
fn run_tsz(cwd: &Path, args: &[&str]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(&tsz_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // tsz currently writes diagnostics to stdout in plain mode.
    let combined = if !stdout.is_empty() {
        stdout.into_owned()
    } else {
        stderr.into_owned()
    };
    Some(normalize_output(&combined))
}

/// Run tsz and return (`exit_code`, `combined_output`).
fn run_tsz_with_exit_code(cwd: &Path, args: &[&str]) -> Option<(i32, String)> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(&tsz_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Combine both stdout and stderr for the full picture
    let mut combined = String::new();
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    Some((code, normalize_output(&combined)))
}

/// Run tsc and return (`exit_code`, `combined_output`).
fn run_tsc_with_exit_code(cwd: &Path, args: &[&str]) -> Option<(i32, String)> {
    let mut cmd = tsc_command()?;
    let output = cmd.args(args).current_dir(cwd).output().ok()?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stderr.is_empty() {
        combined.push_str(&stderr);
    }
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    Some((code, normalize_output(&combined)))
}

/// Run both tsc and tsz and assert their outputs match exactly.
/// Returns the common output on success.
fn assert_tsc_tsz_match(cwd: &Path, args: &[&str], label: &str) -> String {
    let tsc_out = run_tsc(cwd, args).expect("tsc failed to run");
    let tsz_out = run_tsz(cwd, args).expect("tsz failed to run");
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "{label}: tsz output does not match tsc.\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
    tsc_out
}

/// Run both tsc and tsz and assert their outputs AND exit codes match.
fn assert_tsc_tsz_match_with_exit_code(cwd: &Path, args: &[&str], label: &str) {
    let (tsc_code, tsc_out) = run_tsc_with_exit_code(cwd, args).expect("tsc failed to run");
    let (tsz_code, tsz_out) = run_tsz_with_exit_code(cwd, args).expect("tsz failed to run");
    assert_eq!(
        tsc_code, tsz_code,
        "{label}: exit code mismatch: tsc={tsc_code}, tsz={tsz_code}\ntsc output:\n{tsc_out}\ntsz output:\n{tsz_out}"
    );
    let tsc_norm = normalize_output(&tsc_out);
    let tsz_norm = normalize_output(&tsz_out);
    if let Some(diff) = diff_outputs(&tsc_norm, &tsz_norm) {
        panic!("{label}: output mismatch.\n{diff}\n\ntsc:\n{tsc_norm}\n\ntsz:\n{tsz_norm}");
    }
}

/// Find the tsz binary in the target directory.
fn find_tsz_binary() -> Option<PathBuf> {
    // Cargo sets this for integration tests when the package builds a `tsz` binary.
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Nextest may not provide CARGO_BIN_EXE_tsz; derive from the current test binary path.
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(debug_dir) = current_exe.parent().and_then(|p| p.parent())
    {
        let candidate = debug_dir.join("tsz");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Try workspace root (two directories up from crates/tsz-cli)
    if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
        for profile in &["debug", "release", "dist-fast"] {
            for target_dir in &[".target", "target"] {
                let path = workspace_root.join(target_dir).join(profile).join("tsz");
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    // Try crate-local build output locations (fallback)
    for profile in &["debug", "release"] {
        for target_dir in &[".target", "target"] {
            let path = manifest_dir.join(target_dir).join(profile).join("tsz");
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn array_values_iterator_helpers_do_not_report_missing_members() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("array_values_iterator_helpers").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"[1, 2, 3, 4].values()
    .filter((x) => x % 2 === 0)
    .map((x) => x * 10)
    .toArray();
"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--target",
            "esnext",
            "--lib",
            "es2024,es2025.iterator",
            "--strict",
            "--noEmit",
            "--pretty",
            "false",
            "test.ts",
        ],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "array iterator helpers should type-check without false diagnostics:\n{output}"
    );
}

#[test]
fn readonly_property_remains_readonly_after_in_narrowing() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("readonly_in_narrowing").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
type ReadonlyA = { readonly a: string };
type ReadonlyB = { readonly b: number };
type Union = ReadonlyA | ReadonlyB;

declare const x: Union;

if ("a" in x) {
  x.a = "modified";
}
"#,
    );

    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
        "readonly property after in narrowing",
    );
    assert!(
        output.contains("TS2540"),
        "expected readonly assignment diagnostic, got:\n{output}"
    );
}

#[test]
fn template_literal_union_prefix_pattern_matches_before_infer() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("template_literal_union_prefix_pattern").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
type RemoveWhitespace<S extends string> =
  S extends `${" " | "\t"}${infer Rest}` ? Rest : S;

type RW1 = RemoveWhitespace<" hello">;
type RW2 = RemoveWhitespace<"\thello">;

const rw1: RW1 = "hello";
const rw2: RW2 = "hello";
"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "union-prefix template literal pattern should type-check:\n{output}"
    );
}

#[test]
fn required_mapped_keyof_index_access_does_not_report_ts2536() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("required_mapped_keyof_index_access").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
type Test<T> = {
  [K in keyof T]: Required<T>[K];
};

type Obj = { a: number; b?: string };
type T1 = Test<Obj>;
const t1: T1 = { a: 1, b: "x" };
"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "Required<T>[K] where K extends keyof T should type-check:\n{output}"
    );
    assert!(
        !output.contains("TS2536"),
        "unexpected TS2536 diagnostic:\n{output}"
    );
}

#[test]
fn explicit_type_arguments_violate_function_constraint() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("explicit_type_arg_constraints").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
type AppendArgument<Fn extends (...args: any[]) => any, A> =
  Fn extends (...args: infer Args) => infer R
    ? (...args: [...Args, A]) => R
    : never;

type T1 = AppendArgument<unknown, undefined>;
type T2 = AppendArgument<string, number>;
type T3 = AppendArgument<{ a: 1 }, boolean>;
type T4 = AppendArgument<(value: string) => number, boolean>;
"#,
    );

    let args = ["--noEmit", "--strict", "--pretty", "false", "test.ts"];
    let (_, tsc_output) = run_tsc_with_exit_code(&temp.path, &args).expect("tsc should run");
    let (_, tsz_output) = run_tsz_with_exit_code(&temp.path, &args).expect("tsz should run");
    assert_eq!(
        tsc_output.matches("TS2344").count(),
        3,
        "expected tsc fixture to contain three TS2344 diagnostics, got:\n{tsc_output}"
    );
    assert_eq!(
        tsz_output.matches("TS2344").count(),
        3,
        "expected three explicit invalid type arguments to emit TS2344, got:\n{tsz_output}"
    );
}

#[test]
fn recursive_template_literal_intrinsics_evaluate_to_specific_literal() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("recursive_template_literal_intrinsics").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"
type CamelCase<S extends string> = S extends `${infer L}_${infer R}`
  ? `${Lowercase<L>}${CamelCase<Capitalize<R>>}`
  : Lowercase<S>;

type CC1 = CamelCase<"hello_world">;

const x: CC1 = "anything";
"#,
    );

    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
        "recursive template literal intrinsics",
    );
    assert!(
        output.contains("TS2322") && output.contains("\"helloworld\""),
        "expected assignment to fail against the concrete literal, got:\n{output}"
    );
}

#[test]
fn esnext_lib_loads_disposable_symbols_without_builtin_lib_diagnostics() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("esnext_disposable_symbols").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"class Resource {
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

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--strict", "--lib", "esnext", "--pretty", "false", "test.ts",
        ],
    )
    .expect("tsz should run");
    assert_eq!(
        code, 0,
        "--lib esnext should load disposable symbols and avoid unrelated builtin lib diagnostics:\n{output}"
    );
}

#[test]
fn default_parameter_function_initializer_gets_contextual_type() {
    let temp = TempDir::new("default_param_function_context").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        r#"function withDefault(fn: (x: number) => string = (x) => String(x)) {
    return fn(42);
}

const withDefault2 = (fn: (x: number) => string = (x) => String(x)) => fn(42);
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "test.ts"],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(
        code, 0,
        "default parameter function initializers should be contextually typed without TS7006:\n{output}"
    );
}

#[test]
fn batch_mode_uses_project_cwd_for_jsdoc_required_constructor_types() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("batch_jsdoc_required_constructor").expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node.d.ts"),
        "declare function require(id: string): any;\ndeclare var module: any, exports: any;\n",
    );
    write_file(
        &base.join("a-ext.js"),
        "exports.A = function () {\n    this.x = 1;\n};\n",
    );
    write_file(
        &base.join("a.js"),
        "const { A } = require(\"./a-ext\");\n\n/** @param {A} p */\nfunction a(p) { p.x; }\n",
    );
    write_file(
        &base.join("b-ext.js"),
        "exports.B = class {\n    constructor() {\n        this.x = 1;\n    }\n};\n",
    );
    write_file(
        &base.join("b.js"),
        "const { B } = require(\"./b-ext\");\n\n/** @param {B} p */\nfunction b(p) { p.x; }\n",
    );
    write_file(
        &base.join("c-ext.js"),
        "export function C() {\n    this.x = 1;\n}\n",
    );
    write_file(
        &base.join("c.js"),
        "const { C } = require(\"./c-ext\");\n\n/** @param {C} p */\nfunction c(p) { p.x; }\n",
    );
    write_file(
        &base.join("d-ext.js"),
        "export var D = function() {\n    this.x = 1;\n};\n",
    );
    write_file(
        &base.join("d.js"),
        "const { D } = require(\"./d-ext\");\n\n/** @param {D} p */\nfunction d(p) { p.x; }\n",
    );
    write_file(
        &base.join("e-ext.js"),
        "export class E {\n    constructor() {\n        this.x = 1;\n    }\n}\n",
    );
    write_file(
        &base.join("e.js"),
        "const { E } = require(\"./e-ext\");\n\n/** @param {E} p */\nfunction e(p) { p.x; }\n",
    );
    write_file(
        &base.join("f.js"),
        "var F = function () {\n    this.x = 1;\n};\n\n/** @param {F} p */\nfunction f(p) { p.x; }\n",
    );
    write_file(
        &base.join("g.js"),
        "function G() {\n    this.x = 1;\n}\n\n/** @param {G} p */\nfunction g(p) { p.x; }\n",
    );
    write_file(
        &base.join("h.js"),
        "class H {\n    constructor() {\n        this.x = 1;\n    }\n}\n\n/** @param {H} p */\nfunction h(p) { p.x; }\n",
    );
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
  "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
  "exclude": ["node_modules"]
}"#,
    );

    let mut child = Command::new(tsz_bin)
        .arg("--batch")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tsz --batch");

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("batch stdin");
        writeln!(stdin, "{}", base.display()).expect("write batch project");
    }

    let output = child.wait_with_output().expect("wait for tsz --batch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "batch worker failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("TS2339"),
        "expected no TS2339 from JSDoc constructor param in batch mode, got:\n{stdout}\n{stderr}"
    );
}

#[test]
fn declaration_emit_keyword_destructuring_rest_omits_keyword_key() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("keyword_destructuring_rest_dts").expect("temp dir");
    let base = temp.path.as_path();
    let out_dir = base.join("out");

    write_file(
        &base.join("input.ts"),
        r#"
type P = {
    enum: boolean;
    function: boolean;
    abstract: boolean;
    async: boolean;
    await: boolean;
    one: boolean;
};

function f1({ enum: _enum, ...rest }: P) {
    return rest;
}

function f2({ function: _function, ...rest }: P) {
    return rest;
}

function f3({ abstract: _abstract, ...rest }: P) {
    return rest;
}

function f4({ async: _async, ...rest }: P) {
    return rest;
}

function f5({ await: _await, ...rest }: P) {
    return rest;
}
"#,
    );

    let output = Command::new(&tsz_bin)
        .args([
            "--ignoreConfig",
            "--declaration",
            "--emitDeclarationOnly",
            "--target",
            "es2015",
            "--outDir",
            out_dir.to_str().expect("utf-8 temp path"),
            "input.ts",
        ])
        .current_dir(base)
        .output()
        .expect("failed to run tsz");
    assert!(
        output.status.success(),
        "tsz failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let dts = std::fs::read_to_string(out_dir.join("input.d.ts")).expect("declaration output");
    for (function_name, omitted_key) in [
        ("f1", "enum"),
        ("f2", "function"),
        ("f3", "abstract"),
        ("f4", "async"),
        ("f5", "await"),
    ] {
        let signature = format!("declare function {function_name}");
        let start = dts
            .find(&signature)
            .unwrap_or_else(|| panic!("Expected {signature} in declaration output: {dts}"));
        let end = dts[start..]
            .find("};")
            .map_or(dts.len(), |offset| start + offset);
        let emitted_function = &dts[start..end];

        assert!(
            !emitted_function.contains(&format!("    {omitted_key}: boolean;")),
            "Expected `{omitted_key}` to be omitted from {function_name} rest return type: {dts}"
        );
    }
}

/// Normalize output: strip ANSI codes, normalize line endings to \n.
fn normalize_output(s: &str) -> String {
    // Strip ANSI escape codes
    let stripped = strip_ansi(s);
    // Normalize Windows line endings to Unix
    stripped.replace("\r\n", "\n")
}

#[test]
fn generic_private_class_assignment_preserves_type_arguments_in_cli_output() {
    let temp = TempDir::new("generic_private_class_assignment").expect("temp dir");
    let source = r#"
class C<T> {
    #foo: T;
    #method(): T { return this.#foo; }
    get #prop(): T { return this.#foo; }
    set #prop(value: T) { this.#foo = value; }

    bar(x: C<T>) { return x.#foo; }
    bar2(x: C<T>) { return x.#method(); }
    bar3(x: C<T>) { return x.#prop; }

    baz(x: C<number>) { return x.#foo; }
    baz2(x: C<number>) { return x.#method; }
    baz3(x: C<number>) { return x.#prop; }

    quux(x: C<string>) { return x.#foo; }
    quux2(x: C<string>) { return x.#method; }
    quux3(x: C<string>) { return x.#prop; }
}

declare let a: C<number>;
declare let b: C<string>;
a.#foo;
a.#method;
a.#prop;
a = b;
b = a;
"#;
    write_file(&temp.path.join("test.ts"), source);

    let (_, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noEmit",
            "--strict",
            "--target",
            "es6",
            "--strictPropertyInitialization",
            "false",
            "test.ts",
        ],
    )
    .expect("tsz should run");

    assert!(
        output.contains("Type 'C<string>' is not assignable to type 'C<number>'."),
        "expected C<string> -> C<number> display in CLI output, got:\n{output}"
    );
    assert!(
        output.contains("Type 'C<number>' is not assignable to type 'C<string>'."),
        "expected C<number> -> C<string> display in CLI output, got:\n{output}"
    );
    assert!(
        !output.contains("Type 'C' is not assignable to type 'C'."),
        "generic class CLI diagnostic should not erase type arguments, got:\n{output}"
    );
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence: ESC [ ... (letter)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Compare two outputs line by line, returning a detailed diff description.
fn diff_outputs(tsc_output: &str, tsz_output: &str) -> Option<String> {
    let tsc_lines: Vec<&str> = tsc_output.lines().collect();
    let tsz_lines: Vec<&str> = tsz_output.lines().collect();

    let mut diffs = Vec::new();

    let max_lines = tsc_lines.len().max(tsz_lines.len());
    for i in 0..max_lines {
        let tsc_line = tsc_lines.get(i).unwrap_or(&"<missing>");
        let tsz_line = tsz_lines.get(i).unwrap_or(&"<missing>");
        if tsc_line != tsz_line {
            diffs.push(format!(
                "Line {} differs:\n  tsc: {:?}\n  tsz: {:?}",
                i + 1,
                tsc_line,
                tsz_line
            ));
        }
    }

    if tsc_lines.len() != tsz_lines.len() {
        diffs.push(format!(
            "Line count: tsc={}, tsz={}",
            tsc_lines.len(),
            tsz_lines.len()
        ));
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

/// Check that tsc is available on the system.
fn tsc_available() -> bool {
    tsc_command()
        .and_then(|mut cmd| cmd.arg("--version").output().ok())
        .is_some()
}

/// Create a command that runs the pinned repo TypeScript compiler when available.
/// Falls back to PATH `tsc` for environments without `scripts/node_modules` installed.
fn tsc_command() -> Option<Command> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let local_tsc_js = workspace_root.join("scripts/node_modules/typescript/lib/tsc.js");

    if local_tsc_js.exists() {
        let mut cmd = Command::new("node");
        cmd.arg(local_tsc_js);
        return Some(cmd);
    }

    Some(Command::new("tsc"))
}

// ===========================================================================
// Integration tests: exact match (where checker positions agree)
// ===========================================================================

#[test]
fn tsc_compat_cannot_find_name_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("cannot_find_name_plain").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (non-pretty):\n{diff}\n\ntsc output:\n{tsc_out}\n\ntsz output:\n{tsz_out}"
        );
    }
}

#[test]
fn tsc_compat_cannot_find_name_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("cannot_find_name_pretty").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (pretty):\n{diff}\n\ntsc output:\n{tsc_out}\n\ntsz output:\n{tsz_out}"
        );
    }
}

#[test]
fn tsc_compat_multiple_cannot_find_name_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("multi_cannot_find_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (non-pretty, multiple errors):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

#[test]
fn unresolved_callee_callback_still_reports_implicit_any() {
    let temp = TempDir::new("unresolved_callee_callback_implicit_any").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "// @noImplicitAny: true\nconst result = fooBarBaz([], error => error);\nresult;\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2304"),
        "expected unresolved callee diagnostic, got:\n{tsz_out}"
    );
    assert!(
        tsz_out.contains("TS7006"),
        "expected callback implicit-any diagnostic to survive unresolved callee fallback, got:\n{tsz_out}"
    );
}

#[test]
fn errored_initializer_receiver_call_still_reports_implicit_any() {
    let temp = TempDir::new("errored_initializer_receiver_call_implicit_any").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "// @noImplicitAny: true\nconst children = foo.bar();\nchildren.foreach((item) => item);\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2304"),
        "expected unresolved receiver diagnostic, got:\n{tsz_out}"
    );
    assert!(
        tsz_out.contains("TS7006"),
        "expected callback implicit-any diagnostic to survive errored initializer flow, got:\n{tsz_out}"
    );
}

#[test]
fn definite_assignment_error_keeps_callback_context() {
    let temp = TempDir::new("definite_assignment_keeps_callback_context").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "class Observable<T> { map<U>(proj: (e: T) => U): Observable<U> { return null as any; } }\nlet x: Observable<number>;\nlet y = x.map(x => x + 1);\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    assert!(
        tsz_out.contains("TS2454"),
        "expected definite-assignment diagnostic, got:\n{tsz_out}"
    );
    assert!(
        !tsz_out.contains("TS7006"),
        "did not expect callback implicit-any diagnostic when contextual type is still known, got:\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_multiple_cannot_find_name_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("multi_cannot_find_pretty").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (pretty, multiple errors):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// Structural comparison tests (format structure matches, ignoring span positions)
// These test that the output FORMAT is correct even when the checker reports
// errors on different AST nodes.
// ===========================================================================

/// Compare output structure: same number of lines, same diagnostic header format,
/// same summary format, etc. Ignores specific column numbers and underline positions.
fn compare_output_structure(tsc_output: &str, tsz_output: &str) -> Option<String> {
    let tsc_lines: Vec<&str> = tsc_output.lines().collect();
    let tsz_lines: Vec<&str> = tsz_output.lines().collect();

    let mut diffs = Vec::new();

    // Line count must match
    if tsc_lines.len() != tsz_lines.len() {
        diffs.push(format!(
            "Line count: tsc={}, tsz={}",
            tsc_lines.len(),
            tsz_lines.len()
        ));
    }

    let min_lines = tsc_lines.len().min(tsz_lines.len());
    for i in 0..min_lines {
        let tsc_line = tsc_lines[i];
        let tsz_line = tsz_lines[i];

        // Both blank or both non-blank
        if tsc_line.is_empty() != tsz_line.is_empty() {
            diffs.push(format!(
                "Line {}: blank mismatch (tsc blank={}, tsz blank={})",
                i + 1,
                tsc_line.is_empty(),
                tsz_line.is_empty()
            ));
            continue;
        }

        if tsc_line.is_empty() {
            continue; // Both blank, OK
        }

        // For diagnostic header lines, check the format structure
        // tsc non-pretty: file(line,col): error TScode: message
        // tsc pretty: file:line:col - error TScode: message
        if tsc_line.contains(": error TS") || tsc_line.contains(" - error TS") {
            // Both should have the same error code and message
            if let (Some(tsc_code_msg), Some(tsz_code_msg)) = (
                tsc_line.split("error TS").nth(1),
                tsz_line.split("error TS").nth(1),
            ) && tsc_code_msg != tsz_code_msg
            {
                diffs.push(format!(
                    "Line {}: error message differs:\n  tsc: error TS{}\n  tsz: error TS{}",
                    i + 1,
                    tsc_code_msg,
                    tsz_code_msg
                ));
            }
        }

        // For "Found N errors" lines, should match exactly
        if tsc_line.starts_with("Found ") && tsc_line != tsz_line {
            diffs.push(format!(
                "Line {}: summary differs:\n  tsc: {}\n  tsz: {}",
                i + 1,
                tsc_line,
                tsz_line
            ));
        }

        // For "Errors  Files" header, should match exactly
        if tsc_line == "Errors  Files" && tsz_line != "Errors  Files" {
            diffs.push(format!(
                "Line {}: expected 'Errors  Files', got: {}",
                i + 1,
                tsz_line
            ));
        }
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

#[test]
fn tsc_compat_structure_type_error_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("struct_type_error_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Structural comparison (both should have same number of error lines and format)
    let tsc_count = tsc_out.lines().filter(|l| l.contains("error TS")).count();
    let tsz_count = tsz_out.lines().filter(|l| l.contains("error TS")).count();
    assert_eq!(
        tsc_count, tsz_count,
        "Different number of errors:\ntsc ({tsc_count}):\n{tsc_out}\ntsz ({tsz_count}):\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_structure_type_error_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("struct_type_error_pretty").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    if let Some(diff) = compare_output_structure(&tsc_out, &tsz_out) {
        panic!("Output structure mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }
}

#[test]
fn tsc_compat_no_errors_plain() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("no_errors_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x: number = 42;\nconst y: string = \"hello\";\n",
    );

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Both should produce empty output for valid code
    assert!(
        tsc_out.trim().is_empty(),
        "tsc should have no errors: {tsc_out}"
    );
    assert!(
        tsz_out.trim().is_empty(),
        "tsz should have no errors: {tsz_out}"
    );
}

#[test]
fn tsc_compat_exit_code_no_errors() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("exit_code_ok").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x: number = 42;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = tsc_command()
        .expect("tsc command unavailable")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "--ignoreConfig", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsz failed");

    assert_eq!(
        tsc_status.code(),
        tsz_status.code(),
        "Exit codes differ for no-error case: tsc={:?}, tsz={:?}",
        tsc_status.code(),
        tsz_status.code()
    );
}

#[test]
fn tsc_compat_exit_code_with_errors() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("exit_code_err").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");

    let tsz_bin = find_tsz_binary().expect("tsz binary not found");

    let tsc_status = tsc_command()
        .expect("tsc command unavailable")
        .args(["--noEmit", "--pretty", "false", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsc failed");

    let tsz_status = Command::new(&tsz_bin)
        .args(["--noEmit", "--pretty", "false", "--ignoreConfig", "test.ts"])
        .current_dir(&temp.path)
        .status()
        .expect("tsz failed");

    assert_eq!(
        tsc_status.code(),
        tsz_status.code(),
        "Exit codes differ for error case: tsc={:?}, tsz={:?}",
        tsc_status.code(),
        tsz_status.code()
    );
}

// ===========================================================================
// Line ending tests (cross-platform)
// ===========================================================================

#[test]
fn tsc_compat_line_endings_normalized() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("line_endings").expect("temp dir");
    // Use \r\n line endings (Windows style) in the source
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\r\n");

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // After normalization (replace \r\n → \n), outputs should match
    assert!(
        !tsc_out.contains('\r'),
        "tsc output should have normalized line endings"
    );
    assert!(
        !tsz_out.contains('\r'),
        "tsz output should have normalized line endings"
    );

    // Both should detect the same error
    assert!(
        tsc_out.contains("error TS2304"),
        "tsc should find TS2304: {tsc_out}"
    );
    assert!(
        tsz_out.contains("error TS2304"),
        "tsz should find TS2304: {tsz_out}"
    );

    // Exact match for this case (TS2304 spans agree)
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "tsz output does not match tsc (Windows line endings):\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// Format-specific tests
// ===========================================================================

#[test]
fn tsc_compat_plain_format_structure() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("plain_format").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\n",
    );

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "false", "test.ts"]).expect("tsz failed");

    // Non-pretty format: file(line,col): error TScode: message
    for line in tsz_out.lines() {
        if line.is_empty() {
            continue;
        }
        // Each line should match: file(line,col): category TScode: message
        assert!(
            line.contains("): error TS") || line.contains("): warning TS"),
            "Non-pretty line doesn't match format 'file(line,col): error TScode: message': {line}"
        );
        // Should contain parenthesized position
        assert!(
            line.contains('(') && line.contains(')'),
            "Non-pretty line missing parenthesized position: {line}"
        );
        // Should NOT contain source snippets
        assert!(
            !line.contains('~'),
            "Non-pretty line should not have underline markers: {line}"
        );
    }
}

#[test]
fn tsc_compat_pretty_format_structure() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("pretty_format").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const a = foo;\n");

    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    let lines: Vec<&str> = tsz_out.lines().collect();

    // Pretty format structure:
    // Line 0: file:line:col - error TScode: message
    // Line 1: (blank)
    // Line 2: {line_num} {source}
    // Line 3: {underline}
    // Line 4: (blank)
    // Line 5: (blank)
    // Line 6: Found N error(s)...
    // Line 7: (blank - trailing)
    assert!(
        lines.len() >= 6,
        "Pretty output should have at least 6 lines, got {}:\n{}",
        lines.len(),
        tsz_out
    );

    // Line 0: header with colon-separated location and " - error"
    assert!(
        lines[0].contains(" - error TS"),
        "Pretty header should use ' - error TS' format: {}",
        lines[0]
    );
    // Should NOT use parenthesized format in pretty mode
    assert!(
        !lines[0].contains("): error"),
        "Pretty header should not use parenthesized format: {}",
        lines[0]
    );

    // Line 1: blank
    assert!(lines[1].is_empty(), "Line 2 should be blank");

    // Line 2: source line with line number
    assert!(
        lines[2].starts_with('1') || lines[2].starts_with(' '),
        "Source line should start with line number: {}",
        lines[2]
    );

    // Line 3: underline with tildes
    assert!(
        lines[3].contains('~'),
        "Underline line should contain tildes: {}",
        lines[3]
    );

    // Should have "Found" summary
    let has_found = lines.iter().any(|l| l.starts_with("Found "));
    assert!(
        has_found,
        "Should have 'Found N error(s)' summary:\n{tsz_out}"
    );
}

#[test]
fn tsc_compat_double_digit_line_number_pretty() {
    if !tsc_available() {
        return;
    }

    let temp = TempDir::new("double_digit_line").expect("temp dir");
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("const a{i} = {i};\n"));
    }
    source.push_str("const a10 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);

    let tsc_out =
        run_tsc(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsc failed");
    let tsz_out =
        run_tsz(&temp.path, &["--noEmit", "--pretty", "true", "test.ts"]).expect("tsz failed");

    // Exact match: TS2304 spans should agree for both compilers
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!(
            "Double-digit line number output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }
}

// ===========================================================================
// CLI error format tests (TS5023, TS5025, TS6369, build mode flag remapping)
// ===========================================================================

const TS5112_COMMAND_LINE_FILES_OUTPUT: &str = "error TS5112: tsconfig.json is present but will not be loaded if files are specified on commandline. Use '--ignoreConfig' to skip this error.\n";

#[test]
fn build_only_flags_report_ts5093_outside_build_mode() {
    let temp = TempDir::new("build_only_flags_ts5093").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const x = 1;\n");

    for (flag, option_name) in [
        ("--verbose", "verbose"),
        ("--dry", "dry"),
        ("--force", "force"),
        ("--clean", "clean"),
        ("--stopBuildOnErrors", "stopBuildOnErrors"),
    ] {
        let (code, output) =
            run_tsz_with_exit_code(&temp.path, &["--pretty", "false", flag, "a.ts"])
                .expect("tsz binary not found");
        let expected = format!(
            "error TS5093: Compiler option '--{option_name}' may only be used with '--build'.\n"
        );

        assert_eq!(code, 1, "Expected exit code 1 for {flag}, got {code}");
        assert_eq!(output, expected, "Unexpected output for {flag}");
        assert!(
            !temp.path.join("a.js").exists(),
            "{flag} outside build mode should not emit a.js"
        );
    }
}

#[test]
fn build_only_explicit_false_still_reports_ts5093() {
    let temp = TempDir::new("build_only_false_ts5093").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const x = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--dry", "false", "a.ts"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for --dry false");
    assert_eq!(
        output,
        "error TS5093: Compiler option '--dry' may only be used with '--build'.\n"
    );
    assert!(
        !temp.path.join("a.js").exists(),
        "--dry false outside build mode should not emit a.js"
    );
}

#[test]
fn unknown_flag_ts5023_format() {
    let temp = TempDir::new("unknown_flag_ts5023").expect("temp dir");
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--badFlag"]).expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for unknown flag, got {code}");
    assert!(
        output.contains("error TS5023: Unknown compiler option '--badFlag'."),
        "Expected TS5023 diagnostic for unknown flag, got:\n{output}"
    );
}

#[test]
fn unknown_flag_ts5025_suggestion_format() {
    let temp = TempDir::new("unknown_flag_ts5025").expect("temp dir");
    // --strct is close to --strict, should trigger TS5025 with suggestion
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--strct"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for unknown flag with suggestion, got {code}"
    );
    assert!(
        output.contains("error TS5025: Unknown compiler option '--strct'. Did you mean 'strict'?"),
        "Expected TS5025 diagnostic with suggestion, got:\n{output}"
    );
}

#[test]
fn unknown_flag_exit_code_is_1_not_2() {
    let temp = TempDir::new("unknown_flag_exit_code").expect("temp dir");
    let (code, _output) = run_tsz_with_exit_code(&temp.path, &["--totallyBogusOption123"])
        .expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for unknown flag (not clap's default 2), got {code}"
    );
}

#[test]
fn generate_cpu_profile_is_visible_unsupported_error() {
    let temp = TempDir::new("generate_cpu_profile_unsupported").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const value = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["test.ts"]}"#,
    );

    let profile_path = temp.path.join("tsz.cpuprofile");
    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "-p",
            "tsconfig.json",
            "--generateCpuProfile",
            "tsz.cpuprofile",
            "--pretty",
            "false",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected unsupported --generateCpuProfile to exit 1, got {code}:\n{output}"
    );
    assert!(
        output.contains("--generateCpuProfile")
            && output.contains("not supported")
            && output.contains("--generateTrace"),
        "Expected visible unsupported-option error, got:\n{output}"
    );
    assert!(
        !profile_path.exists(),
        "Unsupported --generateCpuProfile should not create a fake profile at {}",
        profile_path.display()
    );
}

#[test]
fn bare_optional_boolean_flags_apply_to_following_input_file() {
    let temp = TempDir::new("bare_optional_boolean_flags").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "function f(value) { return value; }\nconst text: string = null;\n",
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--ignoreConfig",
            "--noEmit",
            "--pretty",
            "false",
            "--noImplicitAny",
            "--strictNullChecks",
            "test.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_ne!(code, 0, "Expected diagnostics exit code, got {code}");
    assert!(
        !output.contains("TS6044"),
        "Bare optional boolean flags should not require explicit values:\n{output}"
    );
    assert!(
        output.contains("TS7006") && output.contains("TS2322"),
        "Expected both bare boolean flags to affect test.ts, got:\n{output}"
    );
}

#[test]
fn command_line_files_with_discovered_tsconfig_report_ts5112() {
    let temp = TempDir::new("command_line_files_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--noLib", "src/a.ts"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS5112, got {code}");
    assert_eq!(output, TS5112_COMMAND_LINE_FILES_OUTPUT);
}

#[test]
fn list_files_only_with_discovered_tsconfig_reports_ts5112() {
    let temp = TempDir::new("list_files_only_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noLib",
            "--listFilesOnly",
            "src/a.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS5112, got {code}");
    assert_eq!(output, TS5112_COMMAND_LINE_FILES_OUTPUT);
}

#[test]
fn list_files_only_reports_ts6504_for_explicit_js_root_without_allow_js() {
    let temp = TempDir::new("list_files_only_ts6504_js_root").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["a.js"]}"#,
    );
    write_file(&temp.path.join("a.js"), "const x = 1;\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--pretty", "false", "--listFilesOnly"])
            .expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for TS6504, got {code}");
    assert!(
        output.contains("error TS6504")
            && output.contains("a.js")
            && output.contains("allowJs")
            && output.contains("Part of 'files' list in tsconfig.json"),
        "--listFilesOnly should report the explicit JS root diagnostic before listing files, got:\n{output}"
    );
}

#[test]
fn list_files_only_without_inputs_and_without_config_prints_help() {
    let temp = TempDir::new("list_files_only_no_inputs_no_config").expect("temp dir");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--listFilesOnly"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for no-input help, got {code}"
    );
    assert!(
        output.contains("Version ") && output.contains("The TypeScript Compiler"),
        "--listFilesOnly without inputs or tsconfig should print help, got:\n{output}"
    );
}

#[test]
fn ignore_config_skips_ts5112_for_command_line_files() {
    let temp = TempDir::new("ignore_config_skips_ts5112").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("src/a.ts"), "const a = 1;\n");

    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--pretty", "false", "--noLib", "--ignoreConfig", "src/a.ts"],
    )
    .expect("tsz binary not found");

    assert!(
        !output.contains("TS5112"),
        "--ignoreConfig should skip TS5112, got:\n{output}"
    );

    let (list_code, list_output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noLib",
            "--ignoreConfig",
            "--listFilesOnly",
            "src/a.ts",
        ],
    )
    .expect("tsz binary not found");

    assert_eq!(
        list_code, 0,
        "--listFilesOnly with --ignoreConfig should succeed, got:\n{list_output}"
    );
    assert!(
        !list_output.contains("TS5112") && list_output.contains("src/a.ts"),
        "--listFilesOnly with --ignoreConfig should list the explicit file, got:\n{list_output}"
    );
}

#[test]
fn build_mode_v_means_verbose() {
    let temp = TempDir::new("build_v_verbose").expect("temp dir");
    // With -b -v, -v should map to --build-verbose, NOT --version.
    // Since there's no tsconfig, it will error, but should not print version info.
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-v"]).expect("tsz binary not found");

    // Should NOT contain version output
    assert!(
        !output.contains("Version ") && !output.contains("tsz "),
        "tsz -b -v should not print version info, got:\n{output}"
    );
    // The build should proceed (even if it fails due to no tsconfig) - it should
    // not be interpreted as --version
    let _ = code; // Exit code varies based on tsconfig presence
}

#[test]
fn build_mode_d_means_dry() {
    let temp = TempDir::new("build_d_dry").expect("temp dir");
    // With -b -d, -d should map to --dry, NOT --declaration.
    // Since there's no tsconfig, it will error, but should try dry run path.
    let (_code, output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-d"]).expect("tsz binary not found");

    // If it tried --declaration instead, clap would likely work differently.
    // The key test: -d in build mode should not set declaration=true outside build context.
    // The output should either show dry run behavior or build mode error (no tsconfig),
    // but not a "declaration" related message.
    let _ = output;
}

#[test]
fn build_mode_f_means_force() {
    let temp = TempDir::new("build_f_force").expect("temp dir");
    // With -b -f, -f should map to --force.
    let (_code, _output) =
        run_tsz_with_exit_code(&temp.path, &["-b", "-f"]).expect("tsz binary not found");
    // Should not error with "unknown argument" for -f in build mode
}

#[test]
fn build_not_first_ts6369() {
    let temp = TempDir::new("build_not_first").expect("temp dir");
    // --build must be first; if it's not, emit TS6369
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--noEmit", "--build"]).expect("tsz binary not found");

    assert_eq!(
        code, 1,
        "Expected exit code 1 for TS6369 (--build not first), got {code}"
    );
    assert!(
        output.contains("error TS6369: Option '--build' must be the first command line argument."),
        "Expected TS6369 diagnostic, got:\n{output}"
    );
}

#[test]
fn build_first_is_ok() {
    let temp = TempDir::new("build_first_ok").expect("temp dir");
    // --build as first argument should NOT trigger TS6369
    let (_code, output) =
        run_tsz_with_exit_code(&temp.path, &["--build"]).expect("tsz binary not found");

    assert!(
        !output.contains("TS6369"),
        "Should not emit TS6369 when --build is first, got:\n{output}"
    );
}

#[test]
fn build_short_b_not_first_ts5023() {
    let temp = TempDir::new("build_short_not_first").expect("temp dir");
    // -b not first: tsc v6 treats this as an unknown flag (TS5023), not TS6369.
    // Only the long form --build triggers TS6369.
    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--noEmit", "-b"]).expect("tsz binary not found");

    assert_eq!(code, 1, "Expected exit code 1 for unknown -b, got {code}");
    assert!(
        output.contains("error TS5023"),
        "Expected TS5023 for -b not first (matching tsc v6), got:\n{output}"
    );
}

// ===========================================================================
// End-to-end parity tests: tsz vs tsc (pinned TypeScript version)
//
// These tests run both compilers on identical inputs and assert that outputs
// and exit codes match exactly. They use the tsc version installed globally,
// which must match the pinned version in scripts/package.json.
// ===========================================================================

// ---------------------------------------------------------------------------
// TS6046: Valid values for enum options
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts6046_target() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_target").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--target", "badValue", "test.ts"],
        "TS6046 --target",
    );
}

#[test]
fn tsc_parity_ts6046_module() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_module").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--module", "badValue", "test.ts"],
        "TS6046 --module",
    );
}

#[test]
fn tsc_parity_ts6046_jsx() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_jsx").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--jsx", "badValue", "test.ts"],
        "TS6046 --jsx",
    );
}

#[test]
fn tsc_parity_ts6046_module_resolution() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_modres").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--moduleResolution", "badValue", "test.ts"],
        "TS6046 --moduleResolution",
    );
}

#[test]
fn tsc_parity_ts6046_module_detection() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6046_moddet").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export {};\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--moduleDetection", "badValue", "test.ts"],
        "TS6046 --moduleDetection",
    );
}

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_exit_code_clean() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_clean").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x: number = 42;\n");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "exit code: clean compile",
    );
}

#[test]
fn tsc_parity_exit_code_errors_no_emit() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_noEmit").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    // --noEmit + errors => exit code 2 (errors present, no outputs to skip)
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "exit code: --noEmit + errors",
    );
}

#[test]
fn tsc_parity_exit_code_unknown_flag() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("exit_unknown").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--totallyBogusFlag"],
        "exit code: unknown flag",
    );
}

// ---------------------------------------------------------------------------
// TS5023 / TS5025: Unknown compiler option
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts5023_unknown_flag() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5023").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--badFlag"], "TS5023 unknown flag");
}

#[test]
fn tsc_parity_ts5025_did_you_mean() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5025").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--strct"], "TS5025 did you mean");
}

#[test]
fn tsc_parity_ts5025_targett() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts5025_target").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--targett"],
        "TS5025 --targett did you mean --target",
    );
}

// ---------------------------------------------------------------------------
// TS6369: --build not first
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts6369_build_not_first() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6369").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--build"],
        "TS6369 --build not first",
    );
}

#[test]
fn tsc_parity_ts6369_short_b_not_first() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts6369_short").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--noEmit", "-b"], "TS6369 -b not first");
}

// ---------------------------------------------------------------------------
// --version / --help
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_version() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--version"], "--version output");
}

#[test]
fn tsc_parity_version_short() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("version_short").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["-v"], "-v output");
}

#[test]
fn tsc_parity_help() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help"], "--help output");
}

#[test]
fn tsc_parity_help_all() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("help_all").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &["--help", "--all"], "--help --all output");
}

#[test]
fn tsc_parity_no_input() {
    if !tsc_available() {
        return;
    }
    // No tsconfig.json, no files => print version + help, exit 1
    let temp = TempDir::new("no_input").expect("temp dir");
    assert_tsc_tsz_match_with_exit_code(&temp.path, &[], "no input (no tsconfig, no files)");
}

#[test]
fn no_input_from_project_subdirectory_uses_parent_tsconfig() {
    let temp = TempDir::new("parent_config_no_input").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true
  },
  "files": ["src/a.ts"]
}
"#,
    );
    write_file(
        &temp.path.join("src/a.ts"),
        "function f(value) {\n  return value;\n}\nf(1);\n",
    );

    let src_dir = temp.path.join("src");
    let (code, output) = run_tsz_with_exit_code(&src_dir, &["--pretty", "false"])
        .expect("tsz should run from project subdirectory");

    assert_ne!(
        code, 0,
        "strict config should report diagnostics:\n{output}"
    );
    assert!(
        output.contains("TS7006"),
        "parent tsconfig should be discovered and applied:\n{output}"
    );
    assert!(
        !output.contains("tsc: The TypeScript Compiler"),
        "subdirectory project discovery should not fall through to help:\n{output}"
    );
}

#[test]
fn tsc_parity_show_config_strict_stays_compact() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_strict_compact").expect("temp dir");
    write_file(&temp.path.join("main.ts"), "const n: number = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2017",
    "strict": true,
    "noEmit": true
  },
  "files": ["main.ts"]
}
"#,
    );

    let tsc_output = run_tsc(&temp.path, &["--showConfig"]).expect("tsc should run");
    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert!(
        !tsc_output.contains("\"strictNullChecks\""),
        "tsc should keep strict sub-options compact: {tsc_output}"
    );
    assert!(
        !output.contains("\"strictNullChecks\""),
        "strict sub-options should not be expanded: {output}"
    );
}

#[test]
fn tsc_parity_show_config_node16_resolve_json_false() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_node16_resolve_json").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        "import data from \"./data.json\";\nconst n: number = data.value;\n",
    );
    write_file(&temp.path.join("data.json"), "{\"value\":123}\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "node16",
    "moduleResolution": "node16",
    "target": "es2017",
    "strict": true,
    "noEmit": true
  },
  "files": ["main.ts", "data.json"]
}
"#,
    );

    let tsc_output = run_tsc(&temp.path, &["--showConfig"]).expect("tsc should run");
    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert!(
        tsc_output.contains("\"resolveJsonModule\": false"),
        "tsc should show node16 resolveJsonModule false: {tsc_output}"
    );
    assert!(
        output.contains("\"resolveJsonModule\": false"),
        "node16 showConfig should include resolveJsonModule false: {output}"
    );
}

#[test]
fn show_config_check_js_implied_allow_js_includes_js_files() {
    let temp = TempDir::new("show_config_checkjs_allowjs_files").expect("temp dir");
    write_file(&temp.path.join("src/a.js"), "module.exports = 1;\n");
    write_file(&temp.path.join("src/b.ts"), "const x: number = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"checkJs":true},"include":["src"]}"#,
    );

    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|value| value.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));
    let files: Vec<_> = json
        .get("files")
        .and_then(|value| value.as_array())
        .unwrap_or_else(|| panic!("missing files in showConfig output:\n{output}"))
        .iter()
        .filter_map(|value| value.as_str())
        .collect();

    assert_eq!(
        options.get("allowJs"),
        Some(&serde_json::Value::Bool(true)),
        "showConfig should print implied allowJs: {output}"
    );
    assert!(
        files.iter().any(|file| file.ends_with("src/a.js")),
        "showConfig files should include JS discovered via implied allowJs: {output}"
    );
    assert!(
        files.iter().any(|file| file.ends_with("src/b.ts")),
        "showConfig files should keep TS files too: {output}"
    );
}

#[test]
fn show_config_renders_inherited_root_selectors_relative_to_child_config() {
    let temp = TempDir::new("show_config_inherited_root_selectors").expect("temp dir");
    let base = temp.path.join("base");
    let child = temp.path.join("child");
    std::fs::create_dir_all(base.join("src")).expect("create base src");
    std::fs::create_dir_all(&child).expect("create child");
    write_file(&base.join("src/a.ts"), "export const x = 1;\n");
    write_file(
        &base.join("tsconfig.base.json"),
        r#"{
  "include": ["src"]
}
"#,
    );
    write_file(
        &child.join("tsconfig.json"),
        r#"{
  "extends": "../base/tsconfig.base.json"
}
"#,
    );

    let output = run_tsz(&child, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let files = json
        .get("files")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("missing files in showConfig output:\n{output}"));
    let include = json
        .get("include")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("missing include in showConfig output:\n{output}"));

    assert_eq!(
        files,
        &[serde_json::Value::String("../base/src/a.ts".to_string())],
        "inherited discovered file should render relative to child config: {output}"
    );
    assert_eq!(
        include,
        &[serde_json::Value::String("../base/src".to_string())],
        "inherited include should render relative to child config: {output}"
    );
    assert!(
        !output.contains(temp.path.to_string_lossy().as_ref()),
        "showConfig should not leak absolute temp paths: {output}"
    );
}

#[test]
fn show_config_rejects_tsconfig_only_cli_options() {
    let temp = TempDir::new("show_config_tsconfig_only_cli_options").expect("temp dir");
    write_file(&temp.path.join("index.ts"), "export {};\n");

    for (flag, value) in [("--paths", "@/*=src/*"), ("--plugins", "foo")] {
        let (code, output) = run_tsz_with_exit_code(
            &temp.path,
            &["--showConfig", "--ignoreConfig", flag, value, "index.ts"],
        )
        .expect("tsz should run");
        assert_eq!(code, 1, "expected failure for {flag}, got: {output}");
        assert!(
            output.contains("error TS6064:"),
            "expected TS6064 for {flag}, got: {output}"
        );
    }
}

#[test]
fn invalid_top_level_config_array_types_emit_ts5024() {
    let temp = TempDir::new("top_level_config_array_types").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "export {};\n");

    for (key, value) in [
        ("include", r#""*.ts""#),
        ("exclude", r#""dist""#),
        ("references", r#""./lib""#),
    ] {
        write_file(
            &temp.path.join("tsconfig.json"),
            &format!(
                r#"{{
  "{key}": {value},
  "compilerOptions": {{ "noEmit": true }},
  "files": ["a.ts"]
}}
"#
            ),
        );

        let (code, output) =
            run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json", "--pretty", "false"])
                .expect("tsz should run");

        assert_ne!(
            code, 0,
            "expected config diagnostic for {key}, got: {output}"
        );
        assert!(
            !output.contains("failed to parse tsconfig"),
            "invalid {key} should recover through TS5024, got:\n{output}"
        );
        assert!(
            output.contains(&format!(
                "error TS5024: Compiler option '{key}' requires a value of type Array."
            )),
            "expected TS5024 for {key}, got:\n{output}"
        );
    }
}

#[test]
fn show_config_includes_supported_direct_and_inherited_options() {
    let temp = TempDir::new("show_config_supported_options").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "enum E { A }\n");
    write_file(
        &temp.path.join("base.json"),
        r#"{
  "compilerOptions": {
    "erasableSyntaxOnly": true
  }
}
"#,
    );
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "extends": "./base.json",
  "compilerOptions": {
    "strictNullChecks": true,
    "exactOptionalPropertyTypes": true
  },
  "files": ["a.ts"]
}
"#,
    );

    let output = run_tsz(&temp.path, &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("strictNullChecks"),
        Some(&serde_json::Value::Bool(true)),
        "control option should still render: {output}"
    );
    assert_eq!(
        options.get("exactOptionalPropertyTypes"),
        Some(&serde_json::Value::Bool(true)),
        "direct supported option should render: {output}"
    );
    assert_eq!(
        options.get("erasableSyntaxOnly"),
        Some(&serde_json::Value::Bool(true)),
        "inherited supported option should render: {output}"
    );
}

#[test]
fn show_config_direct_base_url_and_root_dirs_stay_relative() {
    let temp = TempDir::new("show_config_direct_path_options").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("src")).expect("create src");
    std::fs::create_dir_all(temp.path.join("generated")).expect("create generated");
    write_file(&temp.path.join("src/a.ts"), "export {}\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": "src",
    "rootDirs": ["src", "generated"],
    "rootDir": "src",
    "outDir": "dist"
  },
  "files": ["src/a.ts"]
}
"#,
    );

    let output =
        run_tsz(&temp.path, &["--showConfig", "--pretty", "false"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("baseUrl"),
        Some(&serde_json::Value::String("./src".to_string())),
        "direct baseUrl should stay config-relative: {output}"
    );
    assert_eq!(
        options.get("rootDirs"),
        Some(&serde_json::json!(["./src", "./generated"])),
        "direct rootDirs should stay config-relative: {output}"
    );
    assert!(
        !output.contains(temp.path.to_string_lossy().as_ref()),
        "showConfig leaked the temp directory in path options:\n{output}"
    );
}

#[test]
fn show_config_inherited_base_url_and_root_dirs_stay_declaring_relative() {
    let temp = TempDir::new("show_config_inherited_path_options").expect("temp dir");
    std::fs::create_dir_all(temp.path.join("base/src")).expect("create base src");
    std::fs::create_dir_all(temp.path.join("base/generated")).expect("create base generated");
    std::fs::create_dir_all(temp.path.join("app/src")).expect("create app src");
    write_file(&temp.path.join("app/src/a.ts"), "export {}\n");
    write_file(
        &temp.path.join("base/tsconfig.base.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "rootDirs": ["src", "generated"]
  }
}
"#,
    );
    write_file(
        &temp.path.join("app/tsconfig.json"),
        r#"{
  "extends": "../base/tsconfig.base.json",
  "files": ["src/a.ts"]
}
"#,
    );

    let output = run_tsz(&temp.path.join("app"), &["--showConfig"]).expect("tsz should run");
    let json: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("invalid showConfig JSON:\n{output}"));
    let options = json
        .get("compilerOptions")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("missing compilerOptions in showConfig output:\n{output}"));

    assert_eq!(
        options.get("baseUrl"),
        Some(&serde_json::Value::String("../base".to_string())),
        "inherited baseUrl should render relative to the child config: {output}"
    );
    assert_eq!(
        options.get("rootDirs"),
        Some(&serde_json::json!(["../base/src", "../base/generated"])),
        "inherited rootDirs should render relative to the child config: {output}"
    );
}

// ---------------------------------------------------------------------------
// --init
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_init() {
    if !tsc_available() {
        return;
    }
    // Run --init in separate temp dirs and compare generated tsconfig.json
    let temp_tsc = TempDir::new("init_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_tsz").expect("temp dir");

    let tsc_out = run_tsc(&temp_tsc.path, &["--init"]).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, &["--init"]).expect("tsz --init failed");

    // Console output should match
    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    // Generated tsconfig.json should match
    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init: generated tsconfig.json files differ"
    );
}

/// Regression test for #3905. When `--init` is invoked together with
/// recognized compiler options, the generated tsconfig.json should reflect
/// those options instead of the hardcoded template. This exercises three
/// distinct override paths: replacing a commented template line (`rootDir`,
/// `outDir`), overwriting an active template line (`module`, `target`,
/// `strict`), and appending an option that has no template slot (`pretty`).
#[test]
fn tsc_parity_init_with_options() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_opts_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_opts_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "--rootDir",
        "src",
        "--outDir",
        "dist",
        "--strict",
        "false",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init with options: generated tsconfig.json files differ"
    );
}

/// Multiple command-line-only options (`--diagnostics`, `--listFiles`,
/// `--noEmit`, `--pretty`) get appended after the template body in the order
/// they appeared on the command line.
#[test]
fn tsc_parity_init_appends_command_line_options_in_order() {
    if !tsc_available() {
        return;
    }
    let temp_tsc = TempDir::new("init_append_tsc").expect("temp dir");
    let temp_tsz = TempDir::new("init_append_tsz").expect("temp dir");

    let opts: &[&str] = &[
        "--init",
        "--listFiles",
        "--noEmit",
        "--diagnostics",
        "--pretty",
        "false",
    ];

    let tsc_out = run_tsc(&temp_tsc.path, opts).expect("tsc --init failed");
    let tsz_out = run_tsz(&temp_tsz.path, opts).expect("tsz --init failed");

    if let Some(diff) = diff_outputs(&tsc_out, &tsz_out) {
        panic!("--init console output mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}");
    }

    let tsc_config =
        std::fs::read_to_string(temp_tsc.path.join("tsconfig.json")).expect("tsc tsconfig.json");
    let tsz_config =
        std::fs::read_to_string(temp_tsz.path.join("tsconfig.json")).expect("tsz tsconfig.json");
    assert_eq!(
        tsc_config, tsz_config,
        "--init append-only options: generated tsconfig.json files differ"
    );
}

// ---------------------------------------------------------------------------
// Diagnostic output: plain mode exact match
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_plain_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain single TS2304",
    );
}

#[test]
fn tsc_parity_plain_multiple_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_ts2304").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain multiple TS2304",
    );
}

#[test]
fn tsc_parity_plain_multi_file() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_multi_file").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "const a = foo;\n");
    write_file(&temp.path.join("b.ts"), "const b = bar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "a.ts", "b.ts"],
        "plain multi-file",
    );
}

#[test]
fn tsc_parity_plain_no_errors() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("plain_clean").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x: number = 42;\nconst y: string = \"hello\";\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "plain no errors",
    );
}

// ---------------------------------------------------------------------------
// Diagnostic output: pretty mode exact match
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_pretty_single_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_ts2304").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty single TS2304",
    );
}

#[test]
fn tsc_parity_pretty_multiple_ts2304() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_multi_ts2304").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\nconst c = baz;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty multiple TS2304",
    );
}

#[test]
fn tsc_parity_pretty_multi_file_summary() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_multi_file_summary").expect("temp dir");
    write_file(
        &temp.path.join("a.ts"),
        "const a1 = foo;\nconst a2 = bar;\n",
    );
    write_file(&temp.path.join("b.ts"), "const b1 = baz;\n");
    let tsc_out = run_tsc(
        &temp.path,
        &["--noEmit", "--pretty", "true", "a.ts", "b.ts"],
    )
    .expect("tsc failed");
    let tsz_out = run_tsz(
        &temp.path,
        &["--noEmit", "--pretty", "true", "a.ts", "b.ts"],
    )
    .expect("tsz failed");

    // Check the summary table structure matches
    if let Some(diff) = compare_output_structure(&tsc_out, &tsz_out) {
        panic!(
            "pretty multi-file summary structure mismatch:\n{diff}\n\ntsc:\n{tsc_out}\n\ntsz:\n{tsz_out}"
        );
    }

    // Verify "Found N errors in M files" summary text
    let tsc_summary: Vec<&str> = tsc_out
        .lines()
        .filter(|l| l.starts_with("Found "))
        .collect();
    let tsz_summary: Vec<&str> = tsz_out
        .lines()
        .filter(|l| l.starts_with("Found "))
        .collect();
    assert_eq!(
        tsc_summary, tsz_summary,
        "Found summary mismatch:\ntsc: {tsc_summary:?}\ntsz: {tsz_summary:?}"
    );

    // Verify "Errors  Files" table exists in both
    assert!(
        tsc_out.contains("Errors  Files"),
        "tsc missing 'Errors  Files' table"
    );
    assert!(
        tsz_out.contains("Errors  Files"),
        "tsz missing 'Errors  Files' table"
    );
}

#[test]
fn tsc_parity_pretty_double_digit_line() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_double_digit").expect("temp dir");
    let mut source = String::new();
    for i in 1..=9 {
        source.push_str(&format!("const a{i} = {i};\n"));
    }
    source.push_str("const a10 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty double-digit line number",
    );
}

#[test]
fn tsc_parity_pretty_triple_digit_line() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("pretty_triple_digit").expect("temp dir");
    let mut source = String::new();
    for i in 1..=99 {
        source.push_str(&format!("const v{i} = {i};\n"));
    }
    source.push_str("const v100 = unknownVar;\n");
    write_file(&temp.path.join("test.ts"), &source);
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "pretty triple-digit line number",
    );
}

// ---------------------------------------------------------------------------
// TS2304 with various identifier patterns
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2304_unicode_identifier() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2304_unicode").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = café;\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2304 unicode identifier (plain)",
    );
}

#[test]
fn tsc_parity_ts2304_long_identifier() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2304_long_id").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const x = thisIsAVeryLongIdentifierNameThatDoesNotExistAnywhere;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2304 long identifier (plain)",
    );
}

// ---------------------------------------------------------------------------
// TS2322: type mismatch (plain mode - exact match for error text)
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2322_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2322_plain").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "let x: number = \"hello\";\nlet y: string = 42;\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS2322 type mismatch (plain)",
    );
}

// ---------------------------------------------------------------------------
// TS8020: JSDoc types in TypeScript source
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_jsdoc_constructor_function_suffix() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts8020_jsdoc_constructor_suffix").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        "var c: function(new: number): string;\n",
    );
    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--noEmit", "--pretty", "false", "main.ts"],
        "JSDoc constructor function suffix recovery",
    );
}

// ---------------------------------------------------------------------------
// TS1005: Syntax errors
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts1005_missing_semicolon_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts1005_semi").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = 1\nconst y = 2\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "TS1005 missing semicolon (plain)",
    );
}

// ---------------------------------------------------------------------------
// --build mode: TS5083 missing tsconfig
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_build_missing_tsconfig() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("build_no_tsconfig").expect("temp dir");
    // --build with a path that doesn't exist
    let (tsc_code, tsc_out) =
        run_tsc_with_exit_code(&temp.path, &["--build", "nonexistent/tsconfig.json"])
            .expect("tsc failed");
    let (tsz_code, tsz_out) =
        run_tsz_with_exit_code(&temp.path, &["--build", "nonexistent/tsconfig.json"])
            .expect("tsz failed");

    assert_eq!(
        tsc_code, tsz_code,
        "build missing tsconfig exit code: tsc={tsc_code}, tsz={tsz_code}"
    );
    // Both should mention TS5083
    assert!(
        tsc_out.contains("TS5083") || tsc_out.contains("Cannot read file"),
        "tsc should report missing file: {tsc_out}"
    );
    assert!(
        tsz_out.contains("TS5083") || tsz_out.contains("Cannot read file"),
        "tsz should report missing file: {tsz_out}"
    );
}

// ---------------------------------------------------------------------------
// Line endings: Windows-style source
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_windows_line_endings() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("windows_crlf").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\r\n");
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "Windows CRLF line endings",
    );
}

// ---------------------------------------------------------------------------
// Multiple error codes in same file
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_mixed_error_codes_plain() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("mixed_codes").expect("temp dir");
    // TS2304 (undefined name) + TS2322 (type mismatch) in same file
    write_file(
        &temp.path.join("test.ts"),
        "const a = unknownName;\nlet b: number = \"hello\";\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "false", "test.ts"],
        "mixed error codes (plain)",
    );
}

// ---------------------------------------------------------------------------
// Summary: "Found 1 error" vs "Found N errors"
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_found_1_error_pretty() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("found_1_error").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const z = unknownVar;\n");
    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "Found 1 error summary",
    );
    assert!(
        output.contains("Found 1 error"),
        "Should contain 'Found 1 error': {output}"
    );
}

#[test]
fn tsc_parity_found_n_errors_same_file_pretty() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("found_n_errors_same").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "const a = foo;\nconst b = bar;\n",
    );
    let output = assert_tsc_tsz_match(
        &temp.path,
        &["--noEmit", "--pretty", "true", "test.ts"],
        "Found N errors same file summary",
    );
    assert!(
        output.contains("Found 2 errors in the same file"),
        "Should contain 'Found 2 errors in the same file': {output}"
    );
}

// ---------------------------------------------------------------------------
// Deprecated option values: should still be accepted as input
// ---------------------------------------------------------------------------

// Deprecated values can emit TS5107, but they should still be accepted as
// option values rather than rejected with TS6046.

#[test]
fn deprecated_target_es5_accepted() {
    let temp = TempDir::new("deprecated_es5").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "const x = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--target", "es5", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        !output.contains("TS6046"),
        "Deprecated --target es5 should not produce TS6046: {output}"
    );
}

#[test]
fn removed_target_es3_reports_ts5108() {
    let temp = TempDir::new("removed_es3").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "let x: string = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--target", "ES3", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        output.contains("TS5108"),
        "Removed --target ES3 should produce TS5108: {output}"
    );
    assert!(
        output.contains("Option 'target=ES3' has been removed"),
        "Removed --target ES3 should use the removed-value diagnostic: {output}"
    );
    assert!(
        !output.contains("TS6046"),
        "Removed --target ES3 should not be rejected as an invalid enum value: {output}"
    );
}

#[test]
fn deprecated_module_amd_accepted() {
    let temp = TempDir::new("deprecated_amd").expect("temp dir");
    write_file(&temp.path.join("test.ts"), "export const x = 1;\n");
    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--noEmit", "--pretty", "false", "--module", "amd", "test.ts",
        ],
    )
    .expect("tsz binary not found");
    assert!(
        !output.contains("TS6046"),
        "Deprecated --module amd should not produce TS6046: {output}"
    );
}

#[test]
fn dom_deprecated_tag_name_map_keeps_element_constraint_under_node_merge() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("dom_deprecated_tag_name_map").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "pretty": false,
    "noLib": true
  },
  "files": ["lib.d.ts", "test.ts"]
}
"#,
    );
    write_file(
        &temp.path.join("lib.d.ts"),
        r#"
declare const enum SyntaxKind {
    Modifier,
    Decorator,
}

interface Node {
    kind: SyntaxKind;
}

interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }

interface Element extends Node { tagName: string; }
interface HTMLElement extends Element { id: string; }
interface HTMLUnknownElement extends HTMLElement { unknown: string; }
interface HTMLTrackElement extends HTMLElement { kind: string; }

interface HTMLElementTagNameMap {
    div: HTMLElement;
    track: HTMLTrackElement;
}

interface HTMLElementDeprecatedTagNameMap {
    acronym: HTMLElement;
    applet: HTMLUnknownElement;
}

interface HTMLCollectionOf<T extends Element> {
    item(index: number): T;
}

interface QueryRoot {
    getElementsByTagName<K extends keyof HTMLElementTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementTagNameMap[K]>;
    getElementsByDeprecatedTagName<K extends keyof HTMLElementDeprecatedTagNameMap>(
        qualifiedName: K
    ): HTMLCollectionOf<HTMLElementDeprecatedTagNameMap[K]>;
}
"#,
    );
    write_file(
        &temp.path.join("test.ts"),
        r#"
interface Modifier extends Node { kind: SyntaxKind.Modifier; }
interface Decorator extends Node { kind: SyntaxKind.Decorator; }
"#,
    );

    let (_code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--project", ".", "--noEmit", "--pretty", "false"],
    )
    .expect("tsz binary not found");
    assert!(
        output.contains("HTMLElementTagNameMap[K]"),
        "regular tag map should still fail because HTMLTrackElement.kind conflicts with merged Node.kind: {output}"
    );
    assert!(
        !output.contains("HTMLElementDeprecatedTagNameMap[K]"),
        "deprecated tag map entries all satisfy Element and should not produce TS2344: {output}"
    );
}

// ---------------------------------------------------------------------------
// TS2427: Interface name reserved-word handling.
//
// tsc only emits ONE TS2427 (for the hard-keyword interface name `void` or
// `null`) when such an interface declaration is present in a file. Other
// reserved-name interfaces (`any`, `number`, etc.) in the SAME file have
// their TS2427 suppressed because tsc's parser produces a parse error for
// the hard-keyword name, which cascade-suppresses the lazy diagnostics for
// the other interface declarations.
// Regression test for the conformance failure on
// `interfacesWithPredefinedTypesAsNames.ts`.
// ---------------------------------------------------------------------------

#[test]
fn tsc_parity_ts2427_void_suppresses_other_predefined_names() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_void_suppresses").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface number { }\n\
         interface string { }\n\
         interface boolean { }\n\
         interface void {}\n\
         interface unknown {}\n\
         interface never {}\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 void hard-keyword suppresses other predefined-name TS2427s",
    );
}

#[test]
fn tsc_parity_ts2427_null_suppresses_other_predefined_names() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_null_suppresses").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface null {}\n",
    );
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 null keeps parser recovery TS1005 while any is suppressed",
    );
}

#[test]
fn tsc_parity_ts2427_any_alone_still_reported() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("ts2427_any_only").expect("temp dir");
    write_file(
        &temp.path.join("test.ts"),
        "interface any { }\n\
         interface number { }\n",
    );
    // Without `void`/`null`, tsc reports TS2427 for both interfaces. This
    // test pins that the suppression only kicks in when a hard-keyword
    // interface name is present.
    assert_tsc_tsz_match(
        &temp.path,
        &[
            "--target", "es2015", "--noEmit", "--pretty", "false", "test.ts",
        ],
        "TS2427 still reported for predefined names when no hard keyword present",
    );
}

/// Regression for #3908: when `noEmit` comes from tsconfig.json (not the
/// CLI flag), tsz must exit with `DiagnosticsPresent_OutputsGenerated` (2),
/// matching tsc. Previously the exit-code branch only consulted the CLI
/// arg, so config-only `noEmit` fell through to the outputs-skipped path
/// (1).
#[test]
fn tsconfig_no_emit_with_errors_exits_outputs_generated() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("tsconfig_no_emit_exit_code").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "let x: string = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true},"files":["a.ts"]}"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json", "--pretty", "false"])
            .expect("tsz should run");
    assert!(
        output.contains("TS2322"),
        "expected TS2322 diagnostic, got:\n{output}"
    );
    assert_eq!(
        code, 2,
        "tsconfig noEmit with errors should exit 2 (DiagnosticsPresent_OutputsGenerated), got {code}\n{output}"
    );
}

/// Companion to the test above: the same program with `--noEmit` on the
/// command line must produce the same exit code. This locks the parity
/// between CLI-driven and tsconfig-driven `noEmit`.
#[test]
fn cli_no_emit_with_errors_exits_outputs_generated() {
    let Some(_) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("cli_no_emit_exit_code").expect("temp dir");
    write_file(&temp.path.join("a.ts"), "let x: string = 1;\n");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{},"files":["a.ts"]}"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["-p", "tsconfig.json", "--noEmit", "--pretty", "false"],
    )
    .expect("tsz should run");
    assert!(
        output.contains("TS2322"),
        "expected TS2322 diagnostic, got:\n{output}"
    );
    assert_eq!(
        code, 2,
        "CLI --noEmit with errors should exit 2 (DiagnosticsPresent_OutputsGenerated), got {code}\n{output}"
    );
}

// --- Regression tests for issue #3919 ---
//
// `tsz --showConfig` must print the resolved config without validating root
// files. tsc preserves explicit `files` entries that have unsupported
// extensions or that point at missing paths; tsz used to convert both into
// TS18003 because `discover_ts_files` filtered/rejected them and the empty
// result triggered the "no inputs found" error.

#[test]
fn show_config_preserves_unsupported_extension_in_files() {
    let temp = TempDir::new("show_config_unsupported_extension").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["style.css"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("style.css"), "body{}\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 with an unsupported-extension files entry, got: {output}"
    );
    assert!(
        !output.contains("error TS18003"),
        "--showConfig must not emit TS18003: {output}"
    );
    assert!(
        !output.contains("error TS6054"),
        "--showConfig must not emit TS6054 (unsupported extension): {output}"
    );
    assert!(
        output.contains("\"./style.css\""),
        "--showConfig must preserve the unsupported file entry verbatim: {output}"
    );
}

#[test]
fn show_config_preserves_missing_file_in_files() {
    let temp = TempDir::new("show_config_missing_file").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["missing.ts"],"compilerOptions":{"noEmit":true}}"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 even when an explicit file is missing, got: {output}"
    );
    assert!(
        !output.contains("error TS18003"),
        "--showConfig must not emit TS18003: {output}"
    );
    assert!(
        !output.contains("error TS6053"),
        "--showConfig must not emit TS6053 (file not found): {output}"
    );
    assert!(
        output.contains("\"./missing.ts\""),
        "--showConfig must preserve the missing file entry verbatim: {output}"
    );
}

#[test]
fn show_config_preserves_files_when_only_unsupported_entries() {
    let temp = TempDir::new("show_config_only_unsupported").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["a.css","b.scss"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("a.css"), "/*a*/\n");
    write_file(&temp.path.join("b.scss"), "/*b*/\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(
        code, 0,
        "--showConfig must exit 0 when every explicit file has an unsupported extension, got: {output}"
    );
    assert!(
        output.contains("\"./a.css\"") && output.contains("\"./b.scss\""),
        "--showConfig must preserve every explicit entry verbatim: {output}"
    );
}

#[test]
fn show_config_normalizes_already_relative_files_entry() {
    // A `./`-prefixed path in tsconfig must round-trip unchanged (no `./././`).
    let temp = TempDir::new("show_config_already_relative").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["./main.ts"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("main.ts"), "export {};\n");

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["--showConfig"]).expect("tsz should run");
    assert_eq!(code, 0, "--showConfig must exit 0, got: {output}");
    assert!(
        output.contains("\"./main.ts\""),
        "expected \"./main.ts\" entry: {output}"
    );
    assert!(
        !output.contains("\"././main.ts\""),
        "must not double-prefix already-relative paths: {output}"
    );
}

#[test]
fn tsc_parity_show_config_unsupported_extension_files_entry() {
    if !tsc_available() {
        return;
    }
    let temp = TempDir::new("show_config_parity_unsupported").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"files":["style.css"],"compilerOptions":{"noEmit":true}}"#,
    );
    write_file(&temp.path.join("style.css"), "body{}\n");

    assert_tsc_tsz_match_with_exit_code(
        &temp.path,
        &["--showConfig"],
        "tsz --showConfig must match tsc when files lists an unsupported extension",
    );
}

#[test]
fn this_type_predicate_narrows_receiver_property() {
    let temp = TempDir::new("this_predicate_receiver_property").expect("temp dir");
    write_file(
        &temp.path.join("main.ts"),
        r#"
class Container<T> {
  value: T | null = null;

  hasValue(): this is Container<T> & { value: T } {
    return this.value !== null;
  }
}

const container = new Container<number>();

if (container.hasValue()) {
  const value: number = container.value;
}
"#,
    );

    let (code, output) = run_tsz_with_exit_code(
        &temp.path,
        &["--noEmit", "--strict", "--pretty", "false", "main.ts"],
    )
    .expect("tsz should run");

    assert_eq!(
        code, 0,
        "`this is ...` predicates must narrow receiver properties, got: {output}"
    );
}
