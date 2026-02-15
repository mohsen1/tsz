use super::FileReadResult;
use super::check_module_resolution_compatibility;
use super::read_source_file;
use crate::config::ResolvedCompilerOptions;
use std::io::Write;
use tempfile::NamedTempFile;
use tsz::config::ModuleResolutionKind;
use tsz_common::common::ModuleKind;

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
