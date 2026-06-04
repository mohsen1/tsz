#[test]
fn test_batch_style_project_mode_keeps_ts7005_for_imported_dts_export() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
      "compilerOptions": {
        "jsx": "react",
        "module": "commonjs",
        "target": "es2015"
      },
      "include": ["*.ts", "*.tsx", "*.d.ts"]
    }"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("file.tsx"),
        r#"declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}"#,
    )
    .expect("write jsx declarations");
    fs::write(dir.path().join("test.d.ts"), "export var React;\n").expect("write dts");
    fs::write(
        dir.path().join("react-consumer.tsx"),
        r#"import { React } from "./test";
var foo: any;
var spread1 = <div x='' {...foo} y='' />;"#,
    )
    .expect("write consumer");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("batch-style args");
    let result =
        compile(&args, Path::new(env!("CARGO_MANIFEST_DIR"))).expect("batch compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&7005), "expected TS7005, got: {codes:?}");
}

#[test]
fn test_project_mode_reports_global_nan_equality_ts2845() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "noEmit": true
  },
  "include": ["*.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("test.ts"),
        r#"declare const x: number;

if (x === NaN) {}
if (NaN === x) {}

function t1(value: number, NaN: number) {
    return value === NaN;
}
"#,
    )
    .expect("write test");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let ts2845 = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2845)
        .count();
    assert_eq!(
        ts2845, 2,
        "expected TS2845 for global NaN comparisons only, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_compile_project_reports_template_literal_generic_constraint_ts2322() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "noEmit": true
  },
  "include": ["*.ts", "**/*.ts"],
  "exclude": ["node_modules"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("test.ts"),
        r#"interface NMap {
  1: 'A'
  2: 'B'
  3: 'C'
  4: 'D'
}

declare const g: <T extends 1 | 2 | 3>(x: `${T}`) => NMap[T]

type G1 = <T extends 1 | 2 | 3>(x: `${T}`) => NMap[T]
const g1: G1 = g

type G2 = <T extends 1 | 2 | 3 | 4>(x: `${T}`) => NMap[T]
const g2: G2 = g

type G3 = <T extends 1 | 2>(x: `${T}`) => NMap[T]
const g3: G3 = g
"#,
    )
    .expect("write test");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let direct_args = CliArgs::try_parse_from([
        "tsz",
        dir.path().join("test.ts").to_string_lossy().as_ref(),
        "--strict",
        "--target",
        "esnext",
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("direct args");
    let direct_result = compile(&direct_args, dir.path()).expect("direct compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.file.ends_with("test.ts")
                && diag.message_text.contains(
                    "Type '<T extends 1 | 2 | 3>(x: `${T}`) => NMap[T]' is not assignable to type 'G2'",
                )
        }),
        "Expected project-mode compile to preserve template-literal generic constraint TS2322, got: {:?}",
        result.diagnostics
    );
    assert!(
        direct_result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.file.ends_with("test.ts")
                && diag.message_text.contains(
                    "Type '<T extends 1 | 2 | 3>(x: `${T}`) => NMap[T]' is not assignable to type 'G2'",
                )
        }),
        "Expected direct compile to preserve template-literal generic constraint TS2322, got: {:?}",
        direct_result.diagnostics
    );
}

#[test]
fn test_compile_project_keeps_nolib_global_diagnostics_with_deprecation_errors() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "esnext",
    "module": "amd",
    "noLib": true,
    "declaration": true,
    "outFile": "bundle.js"
  },
  "files": ["fakelib.ts", "file1.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("fakelib.ts"),
        r#"interface Object {}
interface Array<T> {}
interface String {}
interface Boolean {}
interface Number {}
interface Function {}
interface RegExp {}
interface IArguments {}
"#,
    )
    .expect("write fakelib");
    fs::write(
        dir.path().join("file1.ts"),
        r#"/// <reference lib="dom" />
export declare interface HTMLElement { field: string; }
export const elem: HTMLElement = { field: "a" };
"#,
    )
    .expect("write file1");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5107), "expected TS5107, got: {codes:?}");
    assert!(codes.contains(&5101), "expected TS5101, got: {codes:?}");

    let ts2318: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2318)
        .collect();
    assert!(
        ts2318
            .iter()
            .any(|d| d.message_text.contains("CallableFunction")),
        "expected TS2318 for CallableFunction, got: {:?}",
        result.diagnostics
    );
    assert!(
        ts2318
            .iter()
            .any(|d| d.message_text.contains("NewableFunction")),
        "expected TS2318 for NewableFunction, got: {:?}",
        result.diagnostics
    );
}

#[cfg(unix)]
#[test]
fn test_compile_preserve_symlinks_emits_ts2307_for_original_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp dir");
    fs::create_dir_all(dir.path().join("linked")).expect("create linked dir");
    fs::create_dir_all(dir.path().join("app/node_modules/real")).expect("create real dir");
    fs::create_dir_all(dir.path().join("app/node_modules/linked"))
        .expect("create linked alias dir");
    fs::create_dir_all(dir.path().join("app/node_modules/linked2"))
        .expect("create linked2 alias dir");

    fs::write(
        dir.path().join("linked/index.d.ts"),
        "export { real } from \"real\";\nexport class C { private x; }\n",
    )
    .expect("write linked declaration");
    fs::write(
        dir.path().join("app/node_modules/real/index.d.ts"),
        "export const real: string;\n",
    )
    .expect("write real declaration");
    fs::write(
        dir.path().join("app/app.ts"),
        "/// <reference types=\"linked\" />\nimport { C as C1 } from \"linked\";\nimport { C as C2 } from \"linked2\";\nlet x = new C1();\nx = new C2();\n",
    )
    .expect("write app");
    symlink(
        dir.path().join("linked/index.d.ts"),
        dir.path().join("app/node_modules/linked/index.d.ts"),
    )
    .expect("symlink linked");
    symlink(
        dir.path().join("linked/index.d.ts"),
        dir.path().join("app/node_modules/linked2/index.d.ts"),
    )
    .expect("symlink linked2");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "moduleResolution": "bundler",
    "preserveSymlinks": true
  },
  "include": ["**/*"],
  "exclude": ["node_modules"]
}"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz"]).expect("default args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&2307), "expected TS2307, got: {codes:?}");

    let project = dir.path().to_string_lossy().to_string();
    let batch_args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("batch args");
    let batch_result = compile(&batch_args, Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("batch compile succeeds");
    let batch_codes: Vec<u32> = batch_result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        batch_codes.contains(&2307),
        "expected batch-style compile to include TS2307, got: {batch_codes:?}"
    );
}

/// TS17009 ("super before this") is a checker-level semantic error,
/// NOT a grammar error. It must NOT suppress TS5107 deprecation diagnostics.
#[test]
fn test_ts17009_does_not_suppress_deprecation() {
    assert!(
        !is_grammar_error_for_deprecation_priority(17009),
        "TS17009 is a semantic error and must not suppress TS5107"
    );
}

/// TS17011 ("super before property access") is a checker-level semantic error,
/// NOT a grammar error. It must NOT suppress TS5107 deprecation diagnostics.
#[test]
fn test_ts17011_does_not_suppress_deprecation() {
    assert!(
        !is_grammar_error_for_deprecation_priority(17011),
        "TS17011 is a semantic error and must not suppress TS5107"
    );
}

/// TS17006/17007 (exponentiation LHS) ARE grammar-level errors that
/// correctly suppress TS5107 in tsc.
#[test]
fn test_exponentiation_errors_do_suppress_deprecation() {
    assert!(
        is_grammar_error_for_deprecation_priority(17006),
        "TS17006 should suppress TS5107"
    );
    assert!(
        is_grammar_error_for_deprecation_priority(17007),
        "TS17007 should suppress TS5107"
    );
}

/// 8xxx JS grammar errors and specific 1xxx parser errors should suppress TS5107.
#[test]
fn test_grammar_error_classification() {
    // 8xxx: JS grammar errors (8024 is JSDoc, not grammar)
    assert!(is_grammar_error_for_deprecation_priority(8002));
    assert!(!is_grammar_error_for_deprecation_priority(8024));
    // 1xxx parser errors in whitelist
    assert!(is_grammar_error_for_deprecation_priority(1003));
    assert!(is_grammar_error_for_deprecation_priority(1005));
    assert!(is_grammar_error_for_deprecation_priority(1125));
    assert!(is_grammar_error_for_deprecation_priority(1128));
    assert!(is_grammar_error_for_deprecation_priority(1436));
    // Semantic errors: must NOT be grammar errors
    assert!(!is_grammar_error_for_deprecation_priority(2322));
    assert!(!is_grammar_error_for_deprecation_priority(2345));
    assert!(!is_grammar_error_for_deprecation_priority(2358));
    assert!(!is_grammar_error_for_deprecation_priority(2559));
}

fn is_config_level_code(code: u32) -> bool {
    matches!(
        code,
        2318 | 5024 | 5053 | 5069 | 5070 | 5071 | 5095 | 5101 | 5102 | 6059 | 6082 | 18003
    )
}

/// Config-level codes should be recognized correctly.
#[test]
fn test_config_level_code_classification() {
    assert!(is_config_level_code(2318)); // Cannot find global type
    assert!(is_config_level_code(5024)); // Compiler option requires value
    assert!(is_config_level_code(5053)); // Option conflict
    assert!(is_config_level_code(6082)); // Only emit .d.ts
    assert!(is_config_level_code(18003)); // No inputs found

    // Semantic errors must NOT be config-level
    assert!(!is_config_level_code(2322)); // Type not assignable
    assert!(!is_config_level_code(2339)); // Property does not exist
    assert!(!is_config_level_code(1124)); // Digit expected (grammar)
}

/// ES5 target + grammar errors: grammar errors should be emitted,
/// TS5107 deprecation should be suppressed.
#[test]
fn test_es5_target_grammar_errors_suppress_deprecation() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    // Write a test file with a grammar error (1e+ = missing exponent digit → TS1124)
    fs::write(base.join("test.ts"), "1e+\n").expect("write test.ts");
    // ES5 target without ignoreDeprecations
    fs::write(
        base.join("tsconfig.json"),
        r#"{"compilerOptions": {"target": "ES5", "noEmit": true}}"#,
    )
    .expect("write tsconfig.json");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        base.to_str().unwrap(),
        "--pretty",
        "false",
    ])
    .unwrap();
    let result = compile(&args, base).expect("compile succeeds");
    let diagnostics = &result.diagnostics;

    // Should contain TS1124 (grammar error)
    let has_1124 = diagnostics.iter().any(|d| d.code == 1124);
    assert!(
        has_1124,
        "Expected TS1124 (Digit expected) for '1e+' with ES5 target"
    );

    // Should NOT contain TS5107 (grammar errors suppress deprecation)
    let has_5107 = diagnostics.iter().any(|d| d.code == 5107);
    assert!(
        !has_5107,
        "TS5107 should be suppressed when grammar errors are present"
    );
}

#[test]
fn test_types_entry_with_explicit_type_roots_still_emits_ts2688() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    fs::create_dir_all(base.join("typings")).expect("create typings dir");
    fs::create_dir_all(base.join("node_modules/phaser/types")).expect("create phaser types dir");
    fs::write(
        base.join("typings/dummy.d.ts"),
        "declare const dummy: number;\n",
    )
    .expect("write dummy type root");
    fs::write(
        base.join("node_modules/phaser/types/phaser.d.ts"),
        "declare const phaserValue: number;\n",
    )
    .expect("write phaser d.ts");
    fs::write(
        base.join("node_modules/phaser/package.json"),
        r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#,
    )
    .expect("write phaser package.json");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["typings"],
            "types": ["phaser"]
          },
          "files": ["index.ts"]
        }"#,
    )
    .expect("write tsconfig");
    fs::write(base.join("index.ts"), "phaserValue;\n").expect("write index.ts");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 when explicit typeRoots does not contain the requested package, got: {:?}",
        result.diagnostics
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected fallback package globals to stay visible, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn no_check_suppresses_unresolved_triple_slash_type_reference_errors() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noCheck": true
          },
          "files": ["index.ts"]
        }"#,
    )
    .expect("write tsconfig");
    fs::write(
        base.join("index.ts"),
        "/// <reference types=\"missing\" />\nconst value = 1;\n",
    )
    .expect("write index.ts");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR),
        "noCheck should suppress unresolved triple-slash type reference TS2688, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn no_check_keeps_compiler_options_types_errors() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noCheck": true,
            "types": ["missing"]
          },
          "files": ["index.ts"]
        }"#,
    )
    .expect("write tsconfig");
    fs::write(base.join("index.ts"), "const value = 1;\n").expect("write index.ts");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR),
        "noCheck should still report unresolved compilerOptions.types TS2688, got: {:?}",
        result.diagnostics
    );
}

/// When a JavaScript source file contains TypeScript-only syntax (e.g.,
/// `import x = require(...)`), tsc emits TS8002 from
/// `getJSSyntacticDiagnosticsForFile`. Because that diagnostic flows through
/// `getSyntacticDiagnostics`, tsc's `emitFilesAndReportErrors` short-circuits
/// `getSemanticDiagnostics` for *every* file in the program — so any other
/// semantic error (TS2305 missing exported member, TS1192 no default export,
/// TS2591 missing 'require' name, etc.) is suppressed.
///
/// Regression test for the behaviour exercised by
/// `compiler/modulePreserve4.ts`. Ensures a `.cjs` import-equals in a
/// multi-file program suppresses semantic noise across the program but
/// keeps the JS-syntactic TS8002 itself.
#[test]
fn js_only_syntactic_error_suppresses_semantic_diagnostics_program_wide() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    fs::write(base.join("a.ts"), "export const x = 0;\n").expect("write a.ts");
    fs::write(
        base.join("main.cjs"),
        "import { x, y } from \"./a\";\nimport a1 = require(\"./a\");\n",
    )
    .expect("write main.cjs");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "preserve",
            "target": "esnext",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["a.ts", "main.cjs"]
        }"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    // The JS-syntactic error tsc would surface in the syntactic phase.
    assert!(
        codes.contains(&8002),
        "Expected TS8002 'import = require can only be used in TypeScript files' to be reported, got: {:?}",
        result.diagnostics,
    );

    // tsc skips semantic checking for the whole program when any
    // JS-only-syntactic error is present, so these checker-emitted
    // diagnostics must NOT appear.
    for &suppressed in &[2305_u32, 2591, 1192] {
        assert!(
            !codes.contains(&suppressed),
            "TS{} should be suppressed program-wide when any JS-only-syntactic error exists; got diagnostics: {:?}",
            suppressed,
            result.diagnostics,
        );
    }
}

/// Regression test for `conformance/salsa/plainJSGrammarErrors.ts`.
///
/// When a JavaScript file emits a TS-only-syntactic diagnostic (e.g. `TS8009`
/// for a `const` modifier in a class body), tsc's `emitFilesAndReportErrors`
/// short-circuits `getSemanticDiagnostics` program-wide. Checker/binder
/// grammar checks like the break/continue family (`TS1104`/`TS1105`/`TS1107`)
/// must therefore NOT be reported alongside the gate-trigger `TS8xxx` codes.
#[test]
fn js_only_syntactic_gate_suppresses_break_continue_grammar_checks() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    // A `const` field in a class is TS-only syntax in a JS file → TS8009.
    // The break/continue at top level and the cross-function-boundary jump
    // would normally trigger TS1104/TS1105/TS1107 — but tsc skips all
    // semantic diagnostics once any TS8xxx fires from
    // `getJSSyntacticDiagnosticsForFile`.
    fs::write(
        base.join("test.js"),
        "class C {\n    const x = 1\n}\nfunction crossFunctionBoundary() {\n    outer: for(;;) {\n        function test() {\n            break outer\n        }\n    }\n}\nbreak\ncontinue\n",
    )
    .expect("write test.js");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "allowJs": true,
            "noEmit": true
          },
          "files": ["test.js"]
        }"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    // The gate trigger must still be reported.
    assert!(
        codes.contains(&8009),
        "Expected TS8009 'The const modifier can only be used in TypeScript files' to be reported, got: {:?}",
        result.diagnostics,
    );

    // tsc skips semantic checking program-wide when any JS-only-syntactic
    // error is present, so these checker-emitted grammar checks must NOT
    // appear (each one would otherwise be a fingerprint mismatch with tsc).
    for &suppressed in &[1104_u32, 1105, 1107] {
        assert!(
            !codes.contains(&suppressed),
            "TS{} should be suppressed program-wide when any JS-only-syntactic error exists; got diagnostics: {:?}",
            suppressed,
            result.diagnostics,
        );
    }
}

#[test]
fn module_preserve_checked_js_resolved_require_does_not_emit_missing_node_global() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    fs::create_dir_all(base.join("node_modules/dep")).expect("create dep package");
    fs::write(
        base.join("node_modules/dep/package.json"),
        r#"{
          "name": "dep",
          "exports": {
            "import": "./import.mjs",
            "require": "./require.js"
          }
        }"#,
    )
    .expect("write package");
    fs::write(
        base.join("node_modules/dep/import.d.mts"),
        "export const esm: \"esm\";\n",
    )
    .expect("write import types");
    fs::write(
        base.join("node_modules/dep/require.d.ts"),
        "declare const cjs: \"cjs\";\nexport = cjs;\n",
    )
    .expect("write require types");
    fs::write(
        base.join("index.ts"),
        "import { esm } from \"dep\";\nimport cjs = require(\"dep\");\n",
    )
    .expect("write index");
    fs::write(
        base.join("main.js"),
        "import { esm } from \"dep\";\nconst cjs = require(\"dep\");\n",
    )
    .expect("write main");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "preserve",
            "target": "esnext",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["index.ts", "main.js"]
        }"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");

    let node_global_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2591)
        .collect();
    assert!(
        node_global_diags.is_empty(),
        "module preserve require forms should not emit TS2591 when the package resolves; got: {node_global_diags:?}; all diagnostics: {:?}",
        result.diagnostics,
    );
}

#[test]
fn isolated_declaration_codes_block_declaration_emit() {
    // Issue #3709 follow-up: TS9007/TS9011/etc. must suppress `.d.ts`
    // emission for the affected source file. tsc refuses to write a
    // declaration file when isolated-declaration constraints are violated.
    for code in [6232, 9007, 9008, 9010, 9011, 9012, 9013, 9015, 9019, 9039] {
        assert!(
            is_declaration_emit_blocking_diagnostic_code(code),
            "TS{code} (isolated-declarations family) should block declaration emit"
        );
    }
}

#[test]
fn non_isolated_declaration_codes_do_not_block_declaration_emit() {
    // Codes outside the 9007–9039 range and TS4020 must not be flagged as
    // declaration-emit blockers — they're either pure type errors or
    // syntactic diagnostics that don't gate `.d.ts` writing.
    for code in [2322, 2339, 2345, 2741, 7006, 9006, 9040] {
        assert!(
            !is_declaration_emit_blocking_diagnostic_code(code),
            "TS{code} should not block declaration emit"
        );
    }
}

#[test]
fn cross_file_commonjs_merge_blocks_all_declaration_outputs() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("index.js"),
        r#"const m = require("./exporter");

module.exports = m.named;
module.exports.memberName = "thing";
"#,
    )
    .expect("write index");
    fs::write(
        dir.path().join("exporter.js"),
        r#"export function named() {}
"#,
    )
    .expect("write exporter");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--allowJs",
        "--checkJs",
        "--lib",
        "es6",
        "--outDir",
        "out",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "index.js",
        "exporter.js",
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED
        }),
        "expected TS6232, got: {:?}",
        result.diagnostics
    );
    assert!(
        !dir.path().join("out/index.d.ts").exists(),
        "index.d.ts should not be emitted after TS6232"
    );
    assert!(
        !dir.path().join("out/exporter.d.ts").exists(),
        "exporter.d.ts should not be emitted after TS6232"
    );
}

#[test]
fn cross_file_commonjs_default_export_merge_emits_declarations() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("index.js"),
        r#"const m = require("./exporter");

module.exports = m.default;
module.exports.memberName = "thing";
"#,
    )
    .expect("write index");
    fs::write(
        dir.path().join("exporter.js"),
        r#"function validate() {}

export default validate;
"#,
    )
    .expect("write exporter");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--allowJs",
        "--checkJs",
        "--lib",
        "es6",
        "--outDir",
        "out",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "index.js",
        "exporter.js",
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile");

    assert!(
        !result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED
        }),
        "did not expect TS6232, got: {:?}",
        result.diagnostics
    );
    assert!(
        dir.path().join("out/index.d.ts").exists(),
        "index.d.ts should be emitted for default export merges"
    );
    assert!(
        dir.path().join("out/exporter.d.ts").exists(),
        "exporter.d.ts should be emitted for default export merges"
    );
}

#[test]
fn module_none_outfile_dynamic_import_downlevels_without_bundling_js_module() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "none",
    "allowJs": true,
    "outFile": "a.js"
  },
  "files": ["a.ts", "b.js"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("a.ts"), r#"const foo = import("./b");"#).expect("write a");
    fs::write(dir.path().join("b.js"), "export default 1;\n").expect("write b");
    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let bundle_path = fs::canonicalize(dir.path())
        .expect("canonical dir")
        .join("a.js");
    assert!(
        result.emitted_files.iter().any(|path| path == &bundle_path),
        "expected bundle to be written, emitted: {:?}",
        result.emitted_files
    );
    let bundle = fs::read_to_string(bundle_path).expect("read bundle");
    assert!(
        bundle.contains(r#"const foo = Promise.resolve().then(() => __importStar(require("b")));"#),
        "module none outFile dynamic import should downlevel through require().\nOutput:\n{bundle}"
    );
    assert!(
        !bundle.contains("exports.default") && !bundle.contains("Object.defineProperty(exports"),
        "dynamic JS module dependency should not be concatenated into the script bundle.\nOutput:\n{bundle}"
    );
}

#[test]
fn module_none_outfile_native_dynamic_import_still_skips_js_module_body() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2020",
    "module": "none",
    "allowJs": true,
    "outFile": "a.js"
  },
  "files": ["a.ts", "b.js"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("a.ts"), r#"const foo = import("./b");"#).expect("write a");
    fs::write(dir.path().join("b.js"), "export default 1;\n").expect("write b");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    compile(&args, dir.path()).expect("compile succeeds");

    let bundle = fs::read_to_string(dir.path().join("a.js")).expect("read bundle");
    assert!(
        bundle.contains(r#"const foo = import("./b");"#),
        "native dynamic import should be preserved for ES2020.\nOutput:\n{bundle}"
    );
    assert!(
        !bundle.contains("exports.default") && !bundle.contains("Object.defineProperty(exports"),
        "dynamic JS module dependency should not be concatenated into the script bundle.\nOutput:\n{bundle}"
    );
}

#[test]
fn jsdoc_bare_module_imports_inline_commonjs_callable_static_surface_in_declaration_emit() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("base.js"),
        r#"class Base {}
function couldntThinkOfAny() {
    return {};
}
couldntThinkOfAny.Base = Base;
module.exports = couldntThinkOfAny;
"#,
    )
    .expect("write base");
    fs::write(
        dir.path().join("maker.js"),
        r#"class Widget {}
function makeThing() {
    return {};
}
makeThing.Widget = Widget;
module.exports = makeThing;
"#,
    )
    .expect("write maker");
    fs::write(
        dir.path().join("file.js"),
        r#"/** @typedef {import('./base')} BaseFactory */
/** @callback BaseFactoryFactory
 * @param {import('./base')} factory
 */
/** @enum {import('./base')} */
const couldntThinkOfAny = {};

/** @typedef {import('./maker')} MakerAlias */
/** @callback MakerConsumer
 * @param {import('./maker')} renamed
 */
function use() {}
"#,
    )
    .expect("write file");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--allowJs",
        "--checkJs",
        "--lib",
        "es6",
        "--outDir",
        "out",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "base.js",
        "maker.js",
        "file.js",
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile");

    assert!(
        result.diagnostics.is_empty(),
        "did not expect diagnostics, got: {:?}",
        result.diagnostics
    );

    let dts = fs::read_to_string(dir.path().join("out/file.d.ts")).expect("read file.d.ts");
    assert!(
        dts.contains(
            r#"type BaseFactory = {
    (): {};
    Base: {
        new (): {};
    };
};"#
        ),
        "expected BaseFactory to inline callable/static import surface: {dts}"
    );
    assert!(
        dts.contains(
            r#"type BaseFactoryFactory = (factory: {
    (): {};
    Base: {
        new (): {};
    };
}) => any;"#
        ),
        "expected callback parameter import to inline callable/static surface: {dts}"
    );
    assert!(
        dts.contains("declare const couldntThinkOfAny: {};"),
        "expected JSDoc enum bare import expansion to emit const fallback: {dts}"
    );
    assert!(
        !dts.contains("declare namespace couldntThinkOfAny"),
        "did not expect enum bare import expansion to synthesize an empty namespace: {dts}"
    );
    assert!(
        dts.contains(
            r#"type MakerAlias = {
    (): {};
    Widget: {
        new (): {};
    };
};"#
        ),
        "expected renamed typedef import to inline callable/static surface: {dts}"
    );
    assert!(
        dts.contains(
            r#"type MakerConsumer = (renamed: {
    (): {};
    Widget: {
        new (): {};
    };
}) => any;"#
        ),
        "expected renamed callback import to inline callable/static surface: {dts}"
    );
}

/// Regression: `export { } from "./missing"` (and the type-only variant)
/// must not emit TS2307. The export clause binds nothing from the module,
/// so tsc skips module resolution entirely. The rule is structural: a
/// present `NAMED_EXPORTS` clause with zero specifiers is the empty-clause
/// shape, regardless of the `type` modifier or the chosen module specifier
/// text. See issue #6688.
#[test]
fn test_empty_named_export_from_missing_module_does_not_emit_ts2307() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("file.ts"),
        r#"export type { } from "./does-not-exist-a";
export { } from "./does-not-exist-b";
export {};
"#,
    )
    .expect("write file");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        dir.path().join("file.ts").to_string_lossy().as_ref(),
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    let ts2307: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .collect();
    assert!(
        ts2307.is_empty(),
        "Did not expect TS2307 for empty `export {{ }} from \"...\"` or `export type {{ }} from \"...\"`. Got: {ts2307:?}"
    );
}

/// Adjacent shape: a non-empty `export type { X } from "./missing"` MUST
/// still emit TS2307 because the clause references a member of the module.
/// This guards against the empty-clause gate over-suppressing real
/// resolution errors. Two different specifier names exercise that the
/// fix is not keyed off any user-chosen identifier.
#[test]
fn test_nonempty_named_export_from_missing_module_still_emits_ts2307() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("file.ts"),
        r#"export type { Foo } from "./does-not-exist-a";
export { bar } from "./does-not-exist-b";
"#,
    )
    .expect("write file");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        dir.path().join("file.ts").to_string_lossy().as_ref(),
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    let ts2307_count = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .count();
    assert_eq!(
        ts2307_count, 2,
        "Expected two TS2307 diagnostics for non-empty re-exports from missing modules, got: {:?}",
        result.diagnostics
    );
}

/// Adjacent shape: `import type { } from "./missing"` and the non-type
/// variant `import { } from "./missing"` still resolve the module per tsc.
/// The empty-clause gate is intentionally export-side only.
#[test]
fn test_empty_named_import_from_missing_module_still_emits_ts2307() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("file_type.ts"),
        r#"import type { } from "./does-not-exist-c";
export {};
"#,
    )
    .expect("write file_type");
    fs::write(
        dir.path().join("file_value.ts"),
        r#"import { } from "./does-not-exist-d";
export {};
"#,
    )
    .expect("write file_value");

    for fname in ["file_type.ts", "file_value.ts"] {
        let args = CliArgs::try_parse_from([
            "tsz",
            "--noEmit",
            "--pretty",
            "false",
            dir.path().join(fname).to_string_lossy().as_ref(),
        ])
        .expect("parse args");
        let result = compile(&args, dir.path()).expect("compile succeeds");
        let ts2307_count = result
            .diagnostics
            .iter()
            .filter(|diag| {
                diag.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            })
            .count();
        assert_eq!(
            ts2307_count, 1,
            "Expected TS2307 for empty named import from missing module ({fname}); the export-side gate must not affect imports. Got: {:?}",
            result.diagnostics
        );
    }
}

/// Adjacent shape: `export * from "./missing"` (and the namespace and
/// type-only star variants) still emit TS2307. These have no
/// `NAMED_EXPORTS` clause — the export-clause is absent (`export *`) or
/// is a `NAMESPACE_EXPORT` node (`export * as ns`) — so the empty-clause
/// gate does not apply and the normal resolution path runs.
#[test]
fn test_wildcard_export_from_missing_module_still_emits_ts2307() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("file.ts"),
        r#"export * from "./does-not-exist-e";
export * as ns from "./does-not-exist-f";
export type * from "./does-not-exist-g";
"#,
    )
    .expect("write file");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--noEmit",
        "--pretty",
        "false",
        dir.path().join("file.ts").to_string_lossy().as_ref(),
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    let ts2307_count = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .count();
    assert_eq!(
        ts2307_count, 3,
        "Expected three TS2307 diagnostics for wildcard/namespace/type-only star re-exports from missing modules, got: {:?}",
        result.diagnostics
    );
}
