use super::CompilationCache;
use super::FileReadResult;
use super::check_module_resolution_compatibility;
use super::check_module_resolution_compatibility_mut;
use super::compilation_cache_to_build_info;
use super::compile;
use super::find_tsconfig;
use super::is_declaration_emit_blocking_diagnostic_code;
use super::no_input_diagnostics_for_config;
use super::read_source_file;
use crate::args::CliArgs;
use crate::config::{CompilerOptions, ResolvedCompilerOptions};
use crate::incremental::compute_file_version;
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

const fn is_grammar_error_for_deprecation_priority(code: u32) -> bool {
    matches!(
        code,
        8002 | 8003
            | 8004
            | 8006
            | 8008
            | 8009
            | 8010
            | 8011
            | 8013
            | 8015
            | 8016
            | 8017
            | 8018
            | 8037
            | 8038
            | 8039
    ) || matches!(code, 17002 | 17006 | 17007 | 17008 | 17012)
        || matches!(
            code,
            1002 | 1003
                | 1005
                | 1011
                | 1034
                | 1109
                | 1110
                | 1121
                | 1124
                | 1125
                | 1126
                | 1127
                | 1128
                | 1131
                | 1134
                | 1137
                | 1144
                | 1145
                | 1198
                | 1199
                | 1389
                | 1433
                | 1434
                | 1436
                | 1440
                | 1442
                | 1489
        )
        || matches!(code, 2458 | 2754)
}

#[test]
fn find_tsconfig_walks_up_from_project_subdirectory() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("tsconfig.json");
    fs::write(&config, "{}").expect("write tsconfig");
    let nested = dir.path().join("packages/zod/src");
    fs::create_dir_all(&nested).expect("create nested project dir");

    assert_eq!(find_tsconfig(&nested), Some(config));
}

#[test]
fn compilation_cache_build_info_uses_source_hash_for_file_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source_path = dir.path().join("src/index.ts");
    fs::create_dir_all(source_path.parent().unwrap()).expect("create source dir");
    fs::write(&source_path, "export const value = 1;").expect("write source");

    let mut cache = CompilationCache::default();
    cache.export_hashes.insert(source_path.clone(), 0x1234);

    let build_info = compilation_cache_to_build_info(
        &cache,
        std::slice::from_ref(&source_path),
        dir.path(),
        &ResolvedCompilerOptions::default(),
    );
    let file_info = build_info
        .get_file_info("src/index.ts")
        .expect("source file should be recorded");

    assert_eq!(
        file_info.version,
        compute_file_version(&source_path).unwrap()
    );
    assert_eq!(file_info.signature.as_deref(), Some("0000000000001234"));
}

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

/// `--jsxFragmentFactory 234` is invalid (digits aren't a valid identifier
/// chain) and tsc falls back to `React.Fragment` for emit while still
/// surfacing the option-level diagnostic. `--jsxFactory` is asymmetric:
/// tsc preserves it verbatim even when invalid (mirrors the
/// `reactNamespaceInvalidInput` baseline). This test pins the fragment
/// fallback by checking the resolved emitter option after parsing CLI args.
#[test]
fn test_invalid_jsx_fragment_factory_falls_back_to_react_fragment() {
    let args = CliArgs::try_parse_from(["tsz", "--jsxFactory", "h", "--jsxFragmentFactory", "234"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert_eq!(options.checker.jsx_factory, "h");
    assert_eq!(
        options.checker.jsx_fragment_factory, "React.Fragment",
        "Invalid jsxFragmentFactory must fall back to React.Fragment"
    );
}

/// Asymmetric to the test above: invalid `--jsxFactory` is preserved
/// verbatim, matching tsc's `reactNamespaceInvalidInput` baseline where
/// `my-React-Lib.createElement` is emitted unchanged despite the dashes.
#[test]
fn test_invalid_jsx_factory_preserved_verbatim() {
    let args = CliArgs::try_parse_from(["tsz", "--jsxFactory", "my-React-Lib.createElement"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");
    assert_eq!(
        options.checker.jsx_factory, "my-React-Lib.createElement",
        "Invalid jsxFactory should be preserved (matches tsc reactNamespace behavior)"
    );
}

#[test]
fn test_cli_module_resolution_recomputes_package_json_defaults() {
    let args =
        CliArgs::try_parse_from(["tsz", "--moduleResolution", "node16", "--module", "node16"])
            .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert_eq!(
        options.module_resolution,
        Some(ModuleResolutionKind::Node16)
    );
    assert!(options.resolve_package_json_exports);
    assert!(options.resolve_package_json_imports);
    assert!(!options.checker.implied_classic_resolution);
}

#[test]
fn test_cli_explicit_package_json_resolution_false_overrides_derived_default() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--moduleResolution",
        "node16",
        "--module",
        "node16",
        "--resolvePackageJsonExports",
        "false",
        "--resolvePackageJsonImports",
        "false",
    ])
    .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(!options.resolve_package_json_exports);
    assert!(!options.resolve_package_json_imports);
}

#[test]
fn test_cli_module_resolution_preserves_config_package_json_resolution_false() {
    let args =
        CliArgs::try_parse_from(["tsz", "--moduleResolution", "node16", "--module", "node16"])
            .expect("parse args");
    let mut options = ResolvedCompilerOptions {
        resolve_package_json_exports: false,
        resolve_package_json_imports: false,
        ..Default::default()
    };
    let config_options = CompilerOptions {
        resolve_package_json_exports: Some(false),
        resolve_package_json_imports: Some(false),
        ..Default::default()
    };
    super::apply_cli_overrides_with_config_options(&mut options, &args, Some(&config_options))
        .expect("apply overrides");

    assert!(!options.resolve_package_json_exports);
    assert!(!options.resolve_package_json_imports);
}

/// `--strict false` on the command line must override `strict: true` from
/// `tsconfig.json`. `preprocess_args` forwards the explicit-false intent
/// through the hidden `--__explicitly-disabled-bool-flag` side-channel; this
/// test simulates the post-preprocess args directly. Issue #3861.
#[test]
fn test_cli_strict_false_overrides_config_strict_true() {
    let args = CliArgs::try_parse_from(["tsz", "--__explicitly-disabled-bool-flag=strict"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.strict = true;
    options.checker.no_implicit_any = true;
    options.checker.strict_null_checks = true;
    options.checker.always_strict = true;
    let config_options = CompilerOptions {
        strict: Some(true),
        ..Default::default()
    };
    super::apply_cli_overrides_with_config_options(&mut options, &args, Some(&config_options))
        .expect("apply overrides");

    assert!(
        !options.checker.strict,
        "--strict false must flip checker.strict to false"
    );
    assert!(
        !options.checker.no_implicit_any,
        "--strict false must reset the strict-family expansion"
    );
    assert!(!options.checker.strict_null_checks);
    assert!(!options.checker.always_strict);
    assert!(!options.printer.always_strict);
}

#[test]
fn test_cli_no_emit_false_overrides_config_no_emit_true() {
    let args = CliArgs::try_parse_from(["tsz", "--__explicitly-disabled-bool-flag=noEmit"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions {
        no_emit: true,
        ..Default::default()
    };
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        !options.no_emit,
        "--noEmit false must flip no_emit to false (issue #3861)"
    );
}

#[test]
fn test_cli_no_unused_locals_false_overrides_config() {
    let args = CliArgs::try_parse_from(["tsz", "--__explicitly-disabled-bool-flag=noUnusedLocals"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.no_unused_locals = true;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(!options.checker.no_unused_locals);
}

/// An explicit `--strictNullChecks true` after `--strict false` must still
/// re-enable strictNullChecks — the `Option<bool>` family override is applied
/// after the `--strict` expansion/contraction, matching tsc's "individual
/// flags win" rule. This guards the order between
/// `apply_explicitly_disabled_bool_flags` and the strict-family CLI overrides.
#[test]
fn test_cli_strict_false_then_strict_null_checks_true_keeps_strict_null_checks() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--__explicitly-disabled-bool-flag=strict",
        "--strictNullChecks=true",
    ])
    .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.strict = true;
    options.checker.strict_null_checks = true;
    options.checker.no_implicit_any = true;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(!options.checker.strict);
    assert!(!options.checker.no_implicit_any, "expansion still reset");
    assert!(
        options.checker.strict_null_checks,
        "explicit --strictNullChecks true wins over --strict false expansion reset"
    );
}

/// `--strictBuiltinIteratorReturn=false` must reach `checker.strict_builtin_iterator_return`.
/// Previously the CLI parsed the flag but never applied it, so the option was silently
/// ignored — `BuiltinIteratorReturn` kept resolving to `undefined` regardless. (issue #3522)
#[test]
fn test_cli_strict_builtin_iterator_return_false_applied() {
    let args = CliArgs::try_parse_from(["tsz", "--strictBuiltinIteratorReturn=false"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.strict_builtin_iterator_return = true;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        !options.checker.strict_builtin_iterator_return,
        "--strictBuiltinIteratorReturn=false must flip the option to false"
    );
}

#[test]
fn test_cli_strict_builtin_iterator_return_true_applied() {
    let args =
        CliArgs::try_parse_from(["tsz", "--strictBuiltinIteratorReturn=true"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.strict_builtin_iterator_return = false;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        options.checker.strict_builtin_iterator_return,
        "--strictBuiltinIteratorReturn=true must flip the option to true"
    );
}

/// `--strict` should expand to `strict_builtin_iterator_return = true` to match tsc's
/// strict-family expansion. The config-loader path already does this; the CLI path was
/// missing the assignment.
#[test]
fn test_cli_strict_expands_strict_builtin_iterator_return() {
    let args = CliArgs::try_parse_from(["tsz", "--strict"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.strict_builtin_iterator_return = false;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        options.checker.strict_builtin_iterator_return,
        "--strict must enable strict_builtin_iterator_return as part of the strict-family expansion"
    );
}

#[test]
fn test_cli_strict_does_not_enable_no_implicit_returns() {
    let args = CliArgs::try_parse_from(["tsz", "--strict"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.no_implicit_returns = false;
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        !options.checker.no_implicit_returns,
        "--strict must NOT enable no_implicit_returns (not part of the strict family)"
    );
}

/// An explicit `--strictBuiltinIteratorReturn=false` after `--strict` must still
/// disable the option — individual flag overrides win over the strict expansion.
#[test]
fn test_cli_strict_then_strict_builtin_iterator_return_false_keeps_false() {
    let args = CliArgs::try_parse_from(["tsz", "--strict", "--strictBuiltinIteratorReturn=false"])
        .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(
        options.checker.strict,
        "--strict still enables checker.strict"
    );
    assert!(
        !options.checker.strict_builtin_iterator_return,
        "explicit --strictBuiltinIteratorReturn=false wins over --strict expansion"
    );
}

#[test]
fn test_cli_no_unchecked_side_effect_imports_overrides_config_false() {
    let args =
        CliArgs::try_parse_from(["tsz", "--noUncheckedSideEffectImports"]).expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    options.checker.no_unchecked_side_effect_imports = false;
    let config_options = CompilerOptions {
        no_unchecked_side_effect_imports: Some(false),
        ..Default::default()
    };
    super::apply_cli_overrides_with_config_options(&mut options, &args, Some(&config_options))
        .expect("apply overrides");

    assert!(options.checker.no_unchecked_side_effect_imports);
}

#[test]
fn test_compile_no_unchecked_side_effect_imports_cli_reenables_ts2882() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("index.ts"), "import \"./missing.css\";\n").expect("write source");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "noEmit": true,
    "noUncheckedSideEffectImports": false
  },
  "files": ["index.ts"]
}"#,
    )
    .expect("write tsconfig");

    let config_args = CliArgs::try_parse_from(["tsz", "-p", "tsconfig.json"]).expect("parse args");
    let config_result = compile(&config_args, dir.path()).expect("compile config");
    assert!(
        config_result
            .diagnostics
            .iter()
            .all(|diag| diag.code != 2882),
        "config false should suppress TS2882, got: {:?}",
        config_result.diagnostics
    );

    let cli_args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--noUncheckedSideEffectImports",
    ])
    .expect("parse args");
    let cli_result = compile(&cli_args, dir.path()).expect("compile cli override");
    assert!(
        cli_result.diagnostics.iter().any(|diag| diag.code == 2882),
        "CLI override should re-enable TS2882, got: {:?}",
        cli_result.diagnostics
    );
}

#[test]
fn test_cli_source_map_satisfies_config_map_root_validation() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.ts"), "export const x = 1;\n").expect("write source");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "files": ["a.ts"],
  "compilerOptions": {
    "mapRoot": "maps"
  }
}"#,
    )
    .expect("write tsconfig");

    let config_args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("parse config args");
    let config_result = compile(&config_args, dir.path()).expect("compile config");
    assert!(
        config_result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
        }),
        "mapRoot without sourceMap/declarationMap should still report TS5069, got: {:?}",
        config_result.diagnostics
    );

    let cli_args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--sourceMap",
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("parse cli args");
    let cli_result = compile(&cli_args, dir.path()).expect("compile cli override");
    assert!(
        cli_result.diagnostics.iter().all(|diag| {
            diag.code
                != diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
        }),
        "CLI --sourceMap should satisfy mapRoot validation, got: {:?}",
        cli_result.diagnostics
    );
}

#[test]
fn test_cli_emit_declaration_only_inherits_config_declaration() {
    // Regression for #3500: `--emitDeclarationOnly` on the CLI must not
    // report TS5069 when `declaration: true` (or `composite: true`) is
    // supplied via tsconfig.json. The early CLI-only short-circuit was
    // removed in favor of the driver/config validation merge, which already
    // honors the prerequisite.
    for (key, value) in [("declaration", "true"), ("composite", "true")] {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("a.ts"), "export const x: number = 1;\n").expect("write source");
        fs::write(
            dir.path().join("tsconfig.json"),
            format!(
                r#"{{
  "files": ["a.ts"],
  "compilerOptions": {{
    "{key}": {value},
    "outDir": "dist"
  }}
}}"#
            ),
        )
        .expect("write tsconfig");

        let args = CliArgs::try_parse_from([
            "tsz",
            "-p",
            "tsconfig.json",
            "--emitDeclarationOnly",
            "--pretty",
            "false",
        ])
        .expect("parse args");
        let result = compile(&args, dir.path()).expect("compile");
        assert!(
            result.diagnostics.iter().all(|diag| {
                diag.code
                    != diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
            }),
            "config {key}:{value} should satisfy --emitDeclarationOnly TS5069 prerequisite, got: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn test_cli_declaration_map_inherits_config_declaration() {
    // Regression for #3712: `--declarationMap` on the CLI must not report
    // TS5069 when `declaration: true` is supplied via tsconfig.json. The CLI
    // validation shim has to merge the config's effective declaration/composite
    // state before checking TS5069 prerequisites.
    for (key, value) in [("declaration", "true"), ("composite", "true")] {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("a.ts"), "export const x: number = 1;\n").expect("write source");
        fs::write(
            dir.path().join("tsconfig.json"),
            format!(
                r#"{{
  "files": ["a.ts"],
  "compilerOptions": {{
    "{key}": {value},
    "outDir": "dist"
  }}
}}"#
            ),
        )
        .expect("write tsconfig");

        let args = CliArgs::try_parse_from([
            "tsz",
            "-p",
            "tsconfig.json",
            "--declarationMap",
            "--pretty",
            "false",
        ])
        .expect("parse args");
        let result = compile(&args, dir.path()).expect("compile");
        assert!(
            result.diagnostics.iter().all(|diag| {
                diag.code
                    != diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
            }),
            "config {key}:{value} should satisfy --declarationMap TS5069 prerequisite, got: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn test_cli_declaration_map_without_declaration_still_reports_ts5069() {
    // Counter-test for #3712: when neither CLI nor tsconfig.json supplies
    // `declaration` or `composite`, `--declarationMap` must still report
    // TS5069. Otherwise the merge would silently swallow the legitimate error.
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.ts"), "export const x: number = 1;\n").expect("write source");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "files": ["a.ts"],
  "compilerOptions": {
    "outDir": "dist"
  }
}"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--declarationMap",
        "--pretty",
        "false",
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile");
    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
        }),
        "--declarationMap with no declaration/composite should still report TS5069, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_cli_isolated_declarations_inherits_config_declaration() {
    // Mirror of #3712 for `--isolatedDeclarations`: also a Group-1 TS5069
    // trigger that must merge config-level `declaration`/`composite`.
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.ts"), "export const x: number = 1;\n").expect("write source");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "files": ["a.ts"],
  "compilerOptions": {
    "declaration": true,
    "outDir": "dist"
  }
}"#,
    )
    .expect("write tsconfig");

    let args = CliArgs::try_parse_from([
        "tsz",
        "-p",
        "tsconfig.json",
        "--isolatedDeclarations",
        "--pretty",
        "false",
    ])
    .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile");
    assert!(
        result.diagnostics.iter().all(|diag| {
            diag.code
                != diagnostic_codes::OPTION_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_OR_OPTION
        }),
        "config declaration:true should satisfy --isolatedDeclarations TS5069 prerequisite, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_cli_overrides_apply_type_and_declaration_options() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--types",
        "node,jest",
        "--typeRoots",
        "types,node_modules/@types",
        "--declarationDir",
        "declarations",
    ])
    .expect("parse args");
    let mut options = ResolvedCompilerOptions::default();
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert_eq!(
        options.types,
        Some(vec!["node".to_string(), "jest".to_string()])
    );
    assert_eq!(
        options.type_roots,
        Some(vec![
            Path::new("types").to_path_buf(),
            Path::new("node_modules/@types").to_path_buf()
        ])
    );
    assert_eq!(
        options.declaration_dir.as_deref(),
        Some(Path::new("declarations"))
    );
}

#[test]
fn test_cli_overrides_apply_umd_and_synthetic_default_interop_options() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--allowUmdGlobalAccess",
        "--allowSyntheticDefaultImports",
        "false",
    ])
    .expect("parse args");
    let mut options = ResolvedCompilerOptions {
        allow_synthetic_default_imports: true,
        checker: tsz::checker::context::CheckerOptions {
            allow_synthetic_default_imports: true,
            ..Default::default()
        },
        ..Default::default()
    };
    super::apply_cli_overrides(&mut options, &args).expect("apply overrides");

    assert!(options.checker.allow_umd_global_access);
    assert!(!options.allow_synthetic_default_imports);
    assert!(!options.checker.allow_synthetic_default_imports);
}

#[test]
fn test_compile_invalid_react_namespace_reports_config_error_without_jsx_cascade() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "jsx": "react",
    "reactNamespace": "my-React-Lib"
  },
  "include": ["index.tsx"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("index.tsx"), "<foo data/>;\n").expect("write tsx");

    let args = CliArgs::try_parse_from(["tsz"]).expect("default args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![diagnostic_codes::INVALID_VALUE_FOR_REACTNAMESPACE_IS_NOT_A_VALID_IDENTIFIER],
        "Expected only TS5059 for invalid reactNamespace, got: {:?}",
        result.diagnostics
    );
    assert!(result.diagnostics[0].file.ends_with("tsconfig.json"));
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
fn test_compile_project_keeps_unimported_external_module_type_alias_unresolved() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["Helpers.ts", "FromFactor.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("Helpers.ts"),
        "export type StringKeyOf<TObj> = Extract<string, keyof TObj>;\n",
    )
    .expect("write Helpers.ts");
    fs::write(
        dir.path().join("FromFactor.ts"),
        "export type RowToColumns<TColumns> = {\n    [TName in StringKeyOf<TColumns>]: any;\n};\n",
    )
    .expect("write FromFactor.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_NAME
                && diag.message_text.contains("StringKeyOf")
        }),
        "Expected TS2304 for unimported external-module type alias in compile(), got: {:?}",
        result.diagnostics
    );
}

// Variant: different user-chosen names prove the check is structural, not name-keyed.
#[test]
fn test_compile_project_mapped_unimported_constraint_variant_names() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["TypeUtils.ts", "Schema.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("TypeUtils.ts"),
        "export type KeysOf<TObj> = Extract<string, keyof TObj>;\n",
    )
    .expect("write TypeUtils.ts");
    fs::write(
        dir.path().join("Schema.ts"),
        "export type Projection<TRow> = {\n    [TCol in KeysOf<TRow>]: unknown;\n};\n",
    )
    .expect("write Schema.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_NAME && diag.message_text.contains("KeysOf")
        }),
        "Expected TS2304 for unimported KeysOf in mapped type constraint, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_compile_project_script_keeps_unimported_external_module_type_alias_unresolved() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["Helpers.ts", "consumer.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("Helpers.ts"),
        "export type Hidden = string;\n",
    )
    .expect("write Helpers.ts");
    fs::write(dir.path().join("consumer.ts"), "let x: Hidden;\n").expect("write consumer.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_NAME && diag.message_text.contains("Hidden")
        }),
        "Expected TS2304 for script consumer of unimported external-module type alias, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_compile_project_dts_module_alias_requires_import() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["helpers.d.ts", "consumer.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("helpers.d.ts"),
        "export type Hidden = string;\n",
    )
    .expect("write helpers.d.ts");
    fs::write(dir.path().join("consumer.ts"), "export let x: Hidden;\n")
        .expect("write consumer.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_NAME && diag.message_text.contains("Hidden")
        }),
        "Expected TS2304 for unimported exported alias from .d.ts module, got: {:?}",
        result.diagnostics
    );
}

// Variant: when the constraint type IS imported, no TS2304 must be produced.
#[test]
fn test_compile_project_mapped_imported_constraint_no_ts2304() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["Helpers.ts", "Model.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("Helpers.ts"),
        "export type StringKeyOf<TObj> = Extract<string, keyof TObj>;\n",
    )
    .expect("write Helpers.ts");
    fs::write(
        dir.path().join("Model.ts"),
        concat!(
            "import type { StringKeyOf } from \"./Helpers\";\n",
            "export type Row<TColumns> = {\n",
            "    [TName in StringKeyOf<TColumns>]: any;\n",
            "};\n",
        ),
    )
    .expect("write Model.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        !result.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_NAME
                && diag.message_text.contains("StringKeyOf")
        }),
        "Did not expect TS2304 when StringKeyOf is properly imported, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_compile_project_mixin_constructor_object_does_not_emit_ts2510() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015",
    "declaration": true
  },
  "files": ["wrapClass.ts", "index.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("wrapClass.ts"),
        r#"export function wrapClass(param: any) {
    return class Wrapped {
        foo() {
            return param;
        }
    }
}

export type Constructor<T = {}> = new (...args: any[]) => T;

export function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = Date.now();
    };
}
"#,
    )
    .expect("write wrapClass.ts");
    fs::write(
        dir.path().join("index.ts"),
        r#"import { wrapClass, Timestamped } from "./wrapClass";

export default wrapClass(0);

export class User {
    name = "";
}

export class TimestampedUser extends Timestamped(User) {
    constructor() {
        super();
    }
}
"#,
    )
    .expect("write index.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    assert!(
        !result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE),
        "Expected no TS2510 for mixin constructor object base, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_compile_self_referential_wildcard_reexport_does_not_emit_ts2307() {
    // Regression: when a package's typesVersions redirect + relative
    // `export * from "../"` re-export loops back to the same file, tsz used
    // to emit bogus TS2307 ("Cannot find module") diagnostics from its
    // re-export cycle detector. tsc handles the cycle silently — it just
    // treats the chain as contributing no transitive exports.
    //
    // See `typesVersionsDeclarationEmit.multiFileBackReferenceToSelf.ts`.
    let dir = tempfile::tempdir().expect("temp dir");
    let ext_dir = dir.path().join("node_modules").join("ext");
    fs::create_dir_all(ext_dir.join("ts3.1")).expect("create ts3.1 dir");
    fs::write(
        ext_dir.join("package.json"),
        r#"{
    "name": "ext",
    "version": "1.0.0",
    "types": "index",
    "typesVersions": {
        ">=3.1.0-0": { "*" : ["ts3.1/*"] }
    }
}"#,
    )
    .expect("write package.json");
    fs::write(
        ext_dir.join("index.d.ts"),
        "export interface A {}\nexport function fa(): A;\n",
    )
    .expect("write index.d.ts");
    fs::write(
        ext_dir.join("other.d.ts"),
        "export interface B {}\nexport function fb(): B;\n",
    )
    .expect("write other.d.ts");
    fs::write(
        ext_dir.join("ts3.1").join("index.d.ts"),
        "export * from \"../\";\n",
    )
    .expect("write ts3.1/index.d.ts");
    fs::write(
        ext_dir.join("ts3.1").join("other.d.ts"),
        "export * from \"../other\";\n",
    )
    .expect("write ts3.1/other.d.ts");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "esnext",
    "declaration": true
  },
  "files": ["main.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("main.ts"),
        r#"import { fa } from "ext";
import { fb } from "ext/other";

export const va = fa();
export const vb = fb();
"#,
    )
    .expect("write main.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
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
        "Did not expect TS2307 for a self-referential `export * from` cycle. Got: {ts2307:?}"
    );
}

#[test]
fn test_compile_project_default_reexport_duplicate_crash_matches_ts2307_count() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "target": "es2015"
  },
  "files": ["index.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(
        dir.path().join("index.ts"),
        r#"// @noTypesAndSymbols: true

export default function () { }
export { default } from './hi'
export { aa as default } from './hi'
"#,
    )
    .expect("write index.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from(["tsz", "--project", project.as_str(), "--pretty", "false"])
        .expect("project args");
    let result = compile(&args, dir.path()).expect("compile succeeds");

    let ts2307 = result
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .count();
    assert_eq!(
        ts2307, 2,
        "Expected exactly two TS2307 diagnostics for the unresolved re-export specifiers, got: {:?}",
        result.diagnostics
    );

    let ts2323 = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE)
        .count();
    assert_eq!(
        ts2323, 2,
        "Expected exactly two TS2323 diagnostics, got: {:?}",
        result.diagnostics
    );

    let ts2528 = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS)
        .count();
    assert_eq!(
        ts2528, 3,
        "Expected TS2528 on all three conflicting default exports, got: {:?}",
        result.diagnostics
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
