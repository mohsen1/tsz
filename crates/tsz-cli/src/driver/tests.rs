use super::FileReadResult;
use super::check_module_resolution_compatibility;
use super::check_module_resolution_compatibility_mut;
use super::format_extended_diagnostics_phase_progress;
use super::format_extended_diagnostics_residency_snapshot;
use super::no_input_diagnostics_for_config;
use super::read_source_file;
use crate::config::ResolvedCompilerOptions;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
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

    let mut printer = PrinterOptions::default();
    printer.module = ModuleKind::CommonJS;
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
            "module {:?} should be accepted with Node16 resolution",
            module
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
            "module {:?} should be accepted with NodeNext resolution",
            module
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
        FileReadResult::Binary(text) => {
            assert!(!text.is_empty(), "binary text payload should be preserved");
            assert_eq!(text.as_bytes()[0], b'G');
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
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![5052, 18003]);
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
    // 8xxx: JS grammar errors
    assert!(is_grammar_error_for_deprecation_priority(8024));
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

#[test]
fn test_format_extended_diagnostics_phase_progress() {
    let line =
        format_extended_diagnostics_phase_progress("build_program", Duration::from_millis(1250));
    assert_eq!(line, "[extendedDiagnostics] phase build_program: 1250.00ms");
}

#[test]
fn test_format_extended_diagnostics_residency_snapshot() {
    let line = format_extended_diagnostics_residency_snapshot(
        &tsz::parallel::MergedProgramResidencyStats {
            file_count: 4,
            bound_file_arena_count: 4,
            unique_arena_count: 3,
            symbol_arena_count: 20,
            declaration_arena_bucket_count: 5,
            declaration_arena_mapping_count: 7,
            global_symbol_count: 11,
            file_local_symbol_count: 13,
            module_export_symbol_count: 17,
            cross_file_node_symbol_arena_count: 2,
            lib_symbol_count: 19,
        },
    );

    assert!(line.starts_with("[extendedDiagnostics] residency "));
    assert!(line.contains("files=4"));
    assert!(line.contains("bound_file_arenas=4"));
    assert!(line.contains("unique_arenas=3"));
    assert!(line.contains("global_symbols=11"));
    assert!(line.contains("file_local_symbols=13"));
    assert!(line.contains("module_export_symbols=17"));
    assert!(line.contains("cross_file_symbol_arenas=2"));
    assert!(line.contains("lib_symbols=19"));
    assert!(line.contains("symbol_entries=20"));
    assert!(line.contains("declaration_buckets=5"));
    assert!(line.contains("declaration_mappings=7"));
}
