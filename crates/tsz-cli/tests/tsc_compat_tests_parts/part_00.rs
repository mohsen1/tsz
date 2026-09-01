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
fn es2022_readonly_collection_assignments_do_not_cycle_display_aliases() {
    let temp = TempDir::new("readonly_collection_display_alias_cycle").expect("temp dir");
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"ES2022","lib":["ES2022"],"noEmit":true},"files":["collections.ts"]}"#,
    );
    write_file(
        &temp.path.join("collections.ts"),
        r#"
const mapped: ReadonlyMap<string, string> = new Map<string, string>();
const settled: ReadonlySet<number> = new Set<number>();
"#,
    );

    let Some((code, output)) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--extendedDiagnostics",
            "--noEmit",
            "-p",
            "tsconfig.json",
            "--pretty",
            "false",
        ],
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(
        code, 0,
        "readonly collection assignment should terminate without diagnostics:\n{output}"
    );
    assert!(
        output.contains("Total diagnostics:             0"),
        "expected zero diagnostics in extended output:\n{output}"
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

    // Oracle-adjudicated (tsc 7.0.2): 'target=ES5' was REMOVED — the native
    // compiler reports TS5108 (exit 1) before any checking runs, so TS18045
    // (which required a sub-ES2015 target) can no longer fire. tsz matches
    // byte-for-byte (verified side by side on this exact fixture).
    assert_ne!(code, 0, "a removed-option report is an error exit");
    assert!(
        output.contains(
            "error TS5108: Option 'target=ES5' has been removed. Please remove it from your configuration."
        ),
        "expected the 7.0.2 removed-option report, got:\n{output}"
    );
}

#[test]
fn bigint_and_symbol_availability_follow_target_and_lib() {
    // Oracle-adjudicated (tsc 7.0.2): 'target=es5' was removed, so the
    // below-ES2020 premise now uses es2017 — the oracle still reports both
    // TS2737 (BigInt below ES2020) and TS2585 (Symbol value without an
    // es2015+ lib) on these flags, byte-matched by tsz.
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
            "es2017",
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

    assert_ne!(code, 0, "BigInt/Symbol availability below ES2020/es2015-lib should fail");
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

    // Oracle-adjudicated (tsc 7.0.2): 'target=ES5' was REMOVED — the native
    // compiler reports TS5108 (exit 1) before checking, so the ES5-specific
    // TS2705 can no longer fire (an es2017 target with lib=es5 is silent in
    // both compilers). tsz matches byte-for-byte.
    assert_ne!(code, 0, "a removed-option report is an error exit");
    assert!(
        output.contains(
            "error TS5108: Option 'target=ES5' has been removed. Please remove it from your configuration."
        ),
        "expected the 7.0.2 removed-option report, got:\n{output}"
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
        "error TS6048: Locale must be an IETF BCP 47 language tag. Examples: 'en', 'ja-jp'.\n"
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
        "error TS6048: Locale must be an IETF BCP 47 language tag. Examples: 'en', 'ja-jp'.\n"
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
        // tsc 7.0.2 removed 'moduleResolution=node10' (TS5108); 'bundler' is
        // the supported kind whose trace shape both compilers share.
        r#"{"compilerOptions":{"noEmit":true,"moduleResolution":"bundler","module":"esnext"},"files":["index.ts"]}"#,
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
        stdout.contains("Explicitly specified module resolution kind: 'Bundler'."),
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
        Some(1),
        "expected diagnostics-with-outputs-skipped exit (tsc 7.0.2 exits 1 for noEmit runs with errors)\nstdout:\n{stdout}\nstderr:\n{stderr}"
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

/// Adjacent negative case to the mapped-key expansion above: a foreign type
/// in the mapped VALUE position does get printed, so tsc reports TS2883 for
/// it even though the key alias from the same nested package stays silent.
/// Binder and package names differ from the positive case (anti-hardcoding).
#[test]
fn declaration_emit_reports_foreign_mapped_value_from_nested_package() {
    let temp = TempDir::new("foreign_mapped_value").expect("temp dir");

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
        r#"import { makeTable } from "tablepkg";

export const table = makeTable();
"#,
    );
    write_file(
        &temp.path.join("r/node_modules/tablepkg/index.d.ts"),
        r#"export function makeTable(): { [C in import("cols").Col]?: import("cells").CellInfo };
"#,
    );
    write_file(
        &temp
            .path
            .join("r/node_modules/tablepkg/node_modules/cols/index.d.ts"),
        r#"export type Col = "left" | "right";
"#,
    );
    write_file(
        &temp
            .path
            .join("r/node_modules/tablepkg/node_modules/cells/index.d.ts"),
        r#"export interface CellInfo { width: number; }
"#,
    );

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json"]).expect("tsz should run");
    assert_ne!(code, 0, "tsz should report TS2883, got output:\n{output}");
    assert!(
        output.contains("TS2883") && output.contains("CellInfo"),
        "expected TS2883 for the mapped VALUE position type, got:\n{output}",
    );
    assert!(
        !output.contains("'Col'"),
        "the key alias must not be reported — keys serialize as property names:\n{output}",
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
fn declaration_emit_default_object_assign_reports_namespace_alias_for_default_only() {
    let temp = TempDir::new("default_object_assign_namespace_alias").expect("temp dir");

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
            "skipLibCheck": true,
            "strict": true,
            "typeRoots": ["./empty-types"]
          },
          "files": ["r/entry.ts"]
        }"#,
    );
    std::fs::create_dir_all(temp.path.join("empty-types")).expect("empty typeRoots");
    write_file(
        &temp
            .path
            .join("r/node_modules/styled-components/node_modules/hoist-non-react-statics/index.d.ts"),
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
        &temp.path.join("r/node_modules/styled-components/index.d.ts"),
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
        &temp.path.join("r/entry.ts"),
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

    let (code, output) =
        run_tsz_with_exit_code(&temp.path, &["-p", "tsconfig.json"]).expect("tsz should run");
    assert_ne!(code, 0, "tsz should report TS2883");
    assert_eq!(
        output.matches("TS2883").count(),
        1,
        "expected exactly one TS2883 diagnostic, got:\n{output}"
    );
    assert!(
        output.contains("inferred type of 'default'")
            && output.contains("NonReactStatics")
            && output.contains("styled-components/node_modules/hoist-non-react-statics"),
        "expected default TS2883 to name NonReactStatics, got:\n{output}"
    );
    assert!(
        !output.contains("inferred type of 'C'") && !output.contains("reference to 'Statics'"),
        "named tagged-template export should not report helper Statics, got:\n{output}"
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

#[test]
fn create_type_options_required_constraint_cli_no_ts2344() {
    let temp = TempDir::new("create_type_options_required_constraint").expect("temp dir");
    write_file(
        &temp.path.join("repro.ts"),
        r#"type CreateTypeOptions<
  Options extends Required<Options>,
  OverrideOptions extends Partial<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: OverrideOptions[Key] extends Options[Key] ? OverrideOptions[Key] : DefaultOptions[Key];
};

type DefaultPathsOptions = {
  depth: 7;
  anyArrayIndexAccessor: `${number}`;
};

type PathsOptions = {
  depth: number;
  anyArrayIndexAccessor: string;
};

type UnsafePaths<Type, Options extends Required<PathsOptions>> = Type;

type Paths<Type, OverridePathOptions extends Partial<PathsOptions> = {}> = UnsafePaths<
  Type,
  CreateTypeOptions<PathsOptions, OverridePathOptions, DefaultPathsOptions>
>;

type Ok = Paths<{ value: string }>;
"#,
    );
    let args = ["--noEmit", "--pretty", "false", "--strict", "repro.ts"];
    let Some((tsc_code, tsc_output)) = run_tsc_with_exit_code(&temp.path, &args) else {
        println!("skipping: tsc not found");
        return;
    };
    let Some((tsz_code, tsz_output)) = run_tsz_with_exit_code(&temp.path, &args) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_eq!(tsc_code, 0, "tsc should accept repro:\n{tsc_output}");
    assert_eq!(tsz_code, 0, "tsz should accept repro:\n{tsz_output}");
    assert!(
        !tsz_output.contains("TS2344"),
        "dependent Required<T> constraints must not emit TS2344:\n{tsz_output}"
    );
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

/// tsz's deliberate `--generateCpuProfile` help description (#3941): tsz *rejects* that
/// flag as unsupported (see the `unsupported --generateCpuProfile` exit test), so it is
/// honest about it rather than advertising tsc's "Generates a CPU profile." text.
const TSZ_GENERATE_CPU_PROFILE_HELP: &str =
    "Unsupported in tsz; use --generateTrace for native trace output.";

/// Replace the single line immediately following a `--generateCpuProfile` header with a
/// stable placeholder, so the surrounding `--help --all` output can be diffed for exact
/// parity while tolerating the one documented tsz divergence (see `assert_tsc_tsz_help_all_match`).
fn mask_generate_cpu_profile_description(normalized: &str) -> String {
    let mut mask_next = false;
    normalized
        .lines()
        .map(|line| {
            if std::mem::take(&mut mask_next) {
                "<generateCpuProfile description>"
            } else {
                mask_next = line.trim() == "--generateCpuProfile";
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The description line immediately following the `--generateCpuProfile` header, if present.
fn generate_cpu_profile_description(normalized: &str) -> Option<String> {
    let mut lines = normalized.lines();
    for line in lines.by_ref() {
        if line.trim() == "--generateCpuProfile" {
            return lines.next().map(|next| next.trim().to_string());
        }
    }
    None
}

/// Like `assert_tsc_tsz_match_with_exit_code`, but tolerant of the single documented
/// `--generateCpuProfile` help-line divergence. tsz's own divergent line is pinned to the
/// expected #3941 text (so an accidental revert is caught, not masked away); every other
/// line is held to exact parity with tsc.
fn assert_tsc_tsz_help_all_match(cwd: &Path, args: &[&str], label: &str) {
    let (tsc_code, tsc_out) = run_tsc_with_exit_code(cwd, args).expect("tsc failed to run");
    let (tsz_code, tsz_out) = run_tsz_with_exit_code(cwd, args).expect("tsz failed to run");
    assert_eq!(
        tsc_code, tsz_code,
        "{label}: exit code mismatch: tsc={tsc_code}, tsz={tsz_code}\ntsc output:\n{tsc_out}\ntsz output:\n{tsz_out}"
    );
    let tsc_norm = normalize_output(&tsc_out);
    let tsz_norm = normalize_output(&tsz_out);
    assert_eq!(
        generate_cpu_profile_description(&tsz_norm).as_deref(),
        Some(TSZ_GENERATE_CPU_PROFILE_HELP),
        "{label}: tsz should keep its honest --generateCpuProfile help text.\ntsz output:\n{tsz_norm}"
    );
    let tsc_masked = mask_generate_cpu_profile_description(&tsc_norm);
    let tsz_masked = mask_generate_cpu_profile_description(&tsz_norm);
    if let Some(diff) = diff_outputs(&tsc_masked, &tsz_masked) {
        panic!("{label}: output mismatch (after masking the documented --generateCpuProfile divergence).\n{diff}\n\ntsc:\n{tsc_masked}\n\ntsz:\n{tsz_masked}");
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

include!("part_00_tail.rs");
