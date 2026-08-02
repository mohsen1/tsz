use super::*;

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

/// TS7 removed-option values are diagnosed before source grammar checking.
#[test]
fn test_es5_target_removal_stops_before_grammar_diagnostics() {
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

    let has_5108 = diagnostics.iter().any(|d| d.code == 5108);
    assert!(
        has_5108,
        "Expected TS5108 for removed target=ES5, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1124),
        "TS5108 should stop before source grammar diagnostics, got {diagnostics:?}"
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

/// Regression test for `conformance/.../privateIdentifierChain.1.ts`.
///
/// A private identifier in an optional chain (`o?.a.#b`) is a parser grammar
/// error (`TS18030`, emitted by `parseErrorAtRange`). Because tsc surfaces it
/// in the syntactic phase, `emitFilesAndReportErrors` then skips
/// `getSemanticDiagnostics` for the whole program. So the receiver's own
/// possibly-nullish diagnostics (`TS2532`/`TS18048`) — and any unrelated
/// semantic error in another file — must be suppressed, while `TS18030`
/// itself survives.
#[test]
fn private_identifier_chain_grammar_error_suppresses_semantic_diagnostics_program_wide() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    // `this?.a.#b`: `a?` is optional so the receiver `this?.a` is possibly
    // undefined; without the syntactic gate the checker would emit TS2532.
    fs::write(
        base.join("a.ts"),
        "class A {\n    a?: A;\n    #b?: A;\n    m() {\n        this?.a.#b;\n    }\n}\n",
    )
    .expect("write a.ts");
    // An unrelated semantic error in a second file proves the gate is
    // program-wide, not just file-local.
    fs::write(base.join("b.ts"), "let n: number = \"s\";\n").expect("write b.ts");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "target": "esnext",
            "useDefineForClassFields": false,
            "noEmit": true
          },
          "files": ["a.ts", "b.ts"]
        }"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&18030),
        "Expected TS18030 for the private identifier in an optional chain, got: {:?}",
        result.diagnostics,
    );
    for &suppressed in &[2532_u32, 18048, 2322] {
        assert!(
            !codes.contains(&suppressed),
            "TS{suppressed} must be suppressed program-wide when a TS18030 grammar error exists; got: {:?}",
            result.diagnostics,
        );
    }
}

/// Control for the gate above: with a *public* member continuation
/// (`this?.a.b`) there is no grammar error, so the receiver's possibly-nullish
/// `TS2532` must still be reported. Guards against over-suppression.
#[test]
fn public_optional_chain_continuation_keeps_possibly_nullish_diagnostic() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    fs::write(
        base.join("a.ts"),
        "class A {\n    a?: A;\n    b?: A;\n    m() {\n        this?.a.b;\n    }\n}\n",
    )
    .expect("write a.ts");
    fs::write(
        base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "target": "esnext",
            "useDefineForClassFields": false,
            "noEmit": true
          },
          "files": ["a.ts"]
        }"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2532),
        "Expected TS2532 for the possibly-undefined receiver `this?.a`, got: {:?}",
        result.diagnostics,
    );
    assert!(
        !codes.contains(&18030),
        "No private identifier here, so TS18030 must not appear, got: {:?}",
        result.diagnostics,
    );
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
fn module_none_outfile_dynamic_import_is_rejected_before_emit() {
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

    assert!(
        result.diagnostics.iter().any(|diag| diag.code == 6046),
        "expected TS6046 for module=none, got: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diag| diag.code == 5102),
        "expected TS5102 for outFile, got: {:?}",
        result.diagnostics
    );
    assert!(
        result.emitted_files.is_empty() && !dir.path().join("a.js").exists(),
        "removed compiler options must stop emit, emitted: {:?}",
        result.emitted_files
    );
}

#[test]
fn module_none_outfile_native_dynamic_import_is_rejected_before_emit() {
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
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code == 6046),
        "expected TS6046 for module=none, got: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|diag| diag.code == 5102),
        "expected TS5102 for outFile, got: {:?}",
        result.diagnostics
    );
    assert!(
        result.emitted_files.is_empty() && !dir.path().join("a.js").exists(),
        "removed compiler options must stop emit, emitted: {:?}",
        result.emitted_files
    );
}

#[test]
fn jsdoc_bare_module_imports_keep_import_types_and_report_ts1340() {
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

    let ts1340 = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 1340)
        .collect::<Vec<_>>();
    assert_eq!(
        ts1340.len(),
        4,
        "each bare import of a value-only CommonJS module is TS1340: {:?}",
        result.diagnostics,
    );

    let dts = fs::read_to_string(dir.path().join("out/file.d.ts")).expect("read file.d.ts");
    assert!(
        dts.contains("type BaseFactory = import('./base');"),
        "expected BaseFactory to preserve the invalid bare import type: {dts}"
    );
    assert!(
        dts.contains("type BaseFactoryFactory = (factory: import('./base')) => any;"),
        "expected callback parameter to preserve the invalid bare import type: {dts}"
    );
    assert!(
        dts.contains("declare const couldntThinkOfAny: {};"),
        "expected the JSDoc enum declaration to retain its const fallback: {dts}"
    );
    assert!(
        !dts.contains("declare namespace couldntThinkOfAny"),
        "did not expect the JSDoc enum to synthesize an empty namespace: {dts}"
    );
    assert!(
        dts.contains("type MakerAlias = import('./maker');"),
        "expected renamed typedef import to remain an import type: {dts}"
    );
    assert!(
        dts.contains("type MakerConsumer = (renamed: import('./maker')) => any;"),
        "expected renamed callback import to remain an import type: {dts}"
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

/// tsc's `emitFilesAndReportErrors` runs `getSyntacticDiagnostics` first and
/// only computes `getSemanticDiagnostics` when the syntactic phase produced
/// nothing. Several `TS1xxx` codes are emitted by tsc's *binder* (the
/// `checkStrictMode*` family pushes onto `file.bindDiagnostics`) or its
/// *checker* (`checkBreakOrContinueStatement`), so tsc surfaces them through
/// the semantic phase and drops them program-wide as soon as any file has a
/// real parse error.
///
/// tsz's gate used `code < 2000` as a proxy for "the parser emitted this",
/// which wrongly retained every one of them. Verified against the pinned tsc
/// oracle: each construct alone reports its code (the control below), and the
/// same construct in a program that also contains a parse error reports
/// nothing but the parse error.
#[test]
fn real_syntax_error_suppresses_checker_routed_ts1xxx_grammar_program_wide() {
    // (file body, code it reports on its own)
    let subjects: &[(&str, u32)] = &[
        ("export {}\nlbl: var v = 1;\n", 1344), // A label is not allowed here.
        (
            "export {}\nfunction f() { var eval = 1; return eval; }\n",
            1215,
        ), // Invalid use of 'eval'.
        ("export {}\ncontinue;\n", 1104),       // 'continue' outside an iteration statement.
        ("export {}\nbreak;\n", 1105),          // 'break' outside an iteration/switch statement.
        ("export {}\nfunction f() { continue; }\n", 1107), // Jump target crosses function boundary.
    ];

    for (source, code) in subjects {
        // Control: on its own, the construct still reports its diagnostic.
        // Without this the assertion below would pass for a gate that simply
        // deleted the check outright.
        let dir = tempfile::TempDir::new().expect("temp dir");
        let base = dir.path();
        fs::write(base.join("a.ts"), source).expect("write a.ts");
        fs::write(
            base.join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":true,"target":"es2015","noEmit":true},"files":["a.ts"]}"#,
        )
        .expect("write tsconfig");
        let args =
            CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
        let result = compile(&args, base).expect("compile should succeed");
        let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(code),
            "control: TS{code} should still be reported when the program parses cleanly, got: {:?}",
            result.diagnostics,
        );

        // Gate: a real parse error anywhere in the program drops it.
        let dir = tempfile::TempDir::new().expect("temp dir");
        let base = dir.path();
        fs::write(base.join("a.ts"), source).expect("write a.ts");
        fs::write(base.join("bad.ts"), "export function broken( {\n").expect("write bad.ts");
        fs::write(
            base.join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":true,"target":"es2015","noEmit":true},"files":["a.ts","bad.ts"]}"#,
        )
        .expect("write tsconfig");
        let args =
            CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
        let result = compile(&args, base).expect("compile should succeed");
        let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1005),
            "the TS1005 parse error itself must survive the gate, got: {:?}",
            result.diagnostics,
        );
        assert!(
            !codes.contains(code),
            "TS{code} is emitted from tsc's binder/checker, so a real parse error anywhere in the program must suppress it; got: {:?}",
            result.diagnostics,
        );
    }
}

/// The `labeledStatementDeclarationListInLoopNoCrash3/4` corpus rows: an
/// unterminated template literal makes the scanner re-lex the remaining
/// template text as source, and the recovered token stream contains
/// `height: var(...)` — an identifier, a colon and a `var` keyword, which both
/// tsc and tsz parse as a labeled statement wrapping a variable statement.
/// tsc never reports the resulting TS1344 because the file's parse errors
/// already short-circuited its semantic phase.
#[test]
fn unterminated_template_recovery_does_not_report_label_grammar_error() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let base = dir.path();

    fs::write(
        base.join("a.ts"),
        "export class C {\n  m(size: any) {\n    this.f(`${size});\n    for (const item of size) {\n      this.f(\n        [\n          `height: var(--x-${item}-h)`,\n        ].join(';')\n      );\n    }\n  }\n  f(x: any) { return x; }\n}\n",
    )
    .expect("write a.ts");
    fs::write(
        base.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"target":"es2015","noEmit":true},"files":["a.ts"]}"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from(["tsz", "--project", "tsconfig.json"]).expect("parse args");
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&1160),
        "expected the unterminated-template parse error to be reported, got: {:?}",
        result.diagnostics,
    );
    assert!(
        !codes.contains(&1344),
        "TS1344 must not survive the program's parse errors, got: {:?}",
        result.diagnostics,
    );
}
