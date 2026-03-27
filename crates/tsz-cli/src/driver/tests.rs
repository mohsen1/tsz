use super::FileReadResult;
use super::check_module_resolution_compatibility;
use super::check_module_resolution_compatibility_mut;
use super::compile;
use super::no_input_diagnostics_for_config;
use super::read_source_file;
use crate::args::CliArgs;
use crate::config::ResolvedCompilerOptions;
use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tsz::checker::diagnostics::diagnostic_codes;
use tsz::config::ModuleResolutionKind;
use tsz::emitter::PrinterOptions;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

#[test]
fn test_module_resolution_requires_matching_module() {
    let resolved = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
        ..Default::default()
    };

    let diag = check_module_resolution_compatibility(&resolved, None);
    assert!(diag.is_some());
}

#[test]
fn test_module_resolution_incompatibility_preserves_existing_config_diagnostics() {
    let mut config_diagnostics = vec![Diagnostic::error(
        "tsconfig.json".to_string(),
        1,
        5,
        "pre-existing config diagnostic".to_string(),
        18003,
    )];

    let printer = PrinterOptions {
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let resolved = ResolvedCompilerOptions {
        printer,
        module_resolution: Some(ModuleResolutionKind::Node16),
        ..Default::default()
    };

    let had_error = check_module_resolution_compatibility_mut(
        &resolved,
        Some(Path::new("tsconfig.json")),
        &mut config_diagnostics,
    );
    assert!(had_error);
    assert_eq!(config_diagnostics.len(), 2);
    let codes: Vec<u32> = config_diagnostics.iter().map(|diag| diag.code).collect();
    assert!(codes.contains(&18003));
    assert!(codes.contains(
        &diagnostic_codes::OPTION_MODULE_MUST_BE_SET_TO_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO
    ));
}

/// Node18 and Node20 modules are accepted with Node16/NodeNext resolution (no TS5110).
#[test]
fn test_node18_node20_accepted_with_node16_resolution() {
    for module in [ModuleKind::Node18, ModuleKind::Node20] {
        let resolved = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            printer: PrinterOptions {
                module,
                ..Default::default()
            },
            ..Default::default()
        };
        let diag = check_module_resolution_compatibility(&resolved, None);
        assert!(
            diag.is_none(),
            "module {module:?} should be accepted with Node16 resolution"
        );
    }
}

#[test]
fn test_node18_node20_accepted_with_nodenext_resolution() {
    for module in [ModuleKind::Node18, ModuleKind::Node20] {
        let resolved = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::NodeNext),
            printer: PrinterOptions {
                module,
                ..Default::default()
            },
            ..Default::default()
        };
        let diag = check_module_resolution_compatibility(&resolved, None);
        assert!(
            diag.is_none(),
            "module {module:?} should be accepted with NodeNext resolution"
        );
    }
}

/// Non-node modules (e.g. ES2015, CommonJS) with node resolution should emit TS5110.
#[test]
fn test_non_node_module_rejected_with_nodenext_resolution() {
    let resolved = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        printer: PrinterOptions {
            module: ModuleKind::ES2015,
            ..Default::default()
        },
        ..Default::default()
    };
    let diag = check_module_resolution_compatibility(&resolved, None);
    assert!(
        diag.is_some(),
        "ES2015 should be rejected with NodeNext resolution"
    );
}

/// Non-node resolution (e.g. Classic) should never produce TS5110 regardless of module.
#[test]
fn test_classic_resolution_never_emits_ts5110() {
    let resolved = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Classic),
        printer: PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
        ..Default::default()
    };
    let diag = check_module_resolution_compatibility(&resolved, None);
    assert!(
        diag.is_none(),
        "Classic resolution should never emit TS5110"
    );
}

#[test]
fn test_read_source_file_binary_with_control_bytes() {
    let mut file = NamedTempFile::new().expect("temporary file should be created");
    file.write_all(&[0x47, 0x40, 0x04, 0x04, 0x04, 0x04, 0x04])
        .expect("binary-like bytes should be written");
    file.flush().expect("temporary file should be flushed");

    let path = file.path().to_path_buf();
    let result = read_source_file(&path);
    match result {
        FileReadResult::Binary {
            text,
            suppress_parser_diagnostics,
        } => {
            assert!(!text.is_empty(), "binary text payload should be preserved");
            assert_eq!(text.as_bytes()[0], b'G');
            assert!(
                !suppress_parser_diagnostics,
                "control-byte binary should preserve parser diagnostics"
            );
        }
        _ => panic!("expected binary detection for control-byte file"),
    }
}

#[test]
fn test_read_source_file_text_is_not_binary() {
    let mut file = NamedTempFile::new().expect("temporary file should be created");
    file.write_all(b"const x = 1;\n")
        .expect("valid source text should be written");
    file.flush().expect("temporary file should be flushed");

    let path = file.path().to_path_buf();
    let result = read_source_file(&path);
    assert!(
        matches!(result, FileReadResult::Text(text) if text == "const x = 1;\n"),
        "expected valid UTF-8 text to remain text"
    );
}

#[test]
fn test_no_input_diagnostics_preserve_config_errors() {
    let config_diag = Diagnostic::error(
        "tsconfig.json".to_string(),
        10,
        7,
        "Option 'checkJs' cannot be specified without specifying option 'allowJs'.".to_string(),
        5052,
    );
    let diagnostics = no_input_diagnostics_for_config(
        vec![config_diag],
        Some(Path::new("tsconfig.json")),
        Some(&["*.ts".to_string()]),
        Some(&["node_modules".to_string()]),
        false,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![5052, 18003]);
}

#[test]
fn test_compile_emits_ts18003_with_explicit_default_include_and_only_mts_input() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
    "compilerOptions": {
        "module": "esnext",
        "moduleResolution": "node16",
        "allowJs": true
    },
    "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
    "exclude": ["node_modules"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("index.mts"), "export const x = 1;\n").expect("write mts");

    let args = CliArgs::try_parse_from(["tsz"]).expect("default args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "expected TS5110, got: {codes:?}");
    assert!(codes.contains(&18003), "expected TS18003, got: {codes:?}");
}

#[test]
fn test_compile_project_module_esnext_verbatim_const_enum_is_not_treated_as_commonjs() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "esnext",
    "verbatimModuleSyntax": true,
    "noEmit": true
  },
  "files": ["index.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("index.ts"),
        "export const enum E {
  A = 1,
}
",
    )
    .expect("write source");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&1287),
        "did not expect TS1287 for module=esnext verbatimModuleSyntax project: {codes:?}"
    );
    assert!(
        !codes.contains(&1295),
        "did not expect TS1295 for module=esnext verbatimModuleSyntax project: {codes:?}"
    );
}

#[test]
fn test_compile_emits_ts18003_in_batch_style_project_mode() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
      "compilerOptions": {
        "module": "esnext",
        "moduleResolution": "node16",
        "allowJs": true
      },
      "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
      "exclude": ["node_modules"]
    }"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("index.mts"), "export const x = 1;\n").expect("write mts");

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
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "expected TS5110, got: {codes:?}");
    assert!(codes.contains(&18003), "expected TS18003, got: {codes:?}");
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
    use super::is_grammar_error_for_deprecation_priority;
    assert!(
        !is_grammar_error_for_deprecation_priority(17009),
        "TS17009 is a semantic error and must not suppress TS5107"
    );
}

/// TS17011 ("super before property access") is a checker-level semantic error,
/// NOT a grammar error. It must NOT suppress TS5107 deprecation diagnostics.
#[test]
fn test_ts17011_does_not_suppress_deprecation() {
    use super::is_grammar_error_for_deprecation_priority;
    assert!(
        !is_grammar_error_for_deprecation_priority(17011),
        "TS17011 is a semantic error and must not suppress TS5107"
    );
}

/// TS17006/17007 (exponentiation LHS) ARE grammar-level errors that
/// correctly suppress TS5107 in tsc.
#[test]
fn test_exponentiation_errors_do_suppress_deprecation() {
    use super::is_grammar_error_for_deprecation_priority;
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
    use super::is_grammar_error_for_deprecation_priority;
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
