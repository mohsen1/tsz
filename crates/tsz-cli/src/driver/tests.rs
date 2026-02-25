use super::FileReadResult;
use super::check_module_resolution_compatibility;
use super::check_module_resolution_compatibility_mut;
use super::no_input_diagnostics_for_config;
use super::read_source_file;
use crate::config::ResolvedCompilerOptions;
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
