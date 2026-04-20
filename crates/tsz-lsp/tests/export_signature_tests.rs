use super::*;

// Helper to parse, bind, and compute an ExportSignature for a source string.
fn compute_sig(source: &str) -> ExportSignature {
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    ExportSignature::compute(&binder, file_name)
}

// Helper to parse, bind, and build an ExportSignatureInput for a source string.
fn compute_input(source: &str) -> ExportSignatureInput {
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    ExportSignatureInput::from_binder(&binder, file_name)
}
include!("export_signature_tests_parts/part_00.rs");
include!("export_signature_tests_parts/part_01.rs");
include!("export_signature_tests_parts/part_02.rs");
include!("export_signature_tests_parts/part_03.rs");
include!("export_signature_tests_parts/part_04.rs");
include!("export_signature_tests_parts/part_05.rs");
include!("export_signature_tests_parts/part_06.rs");
include!("export_signature_tests_parts/part_07.rs");
include!("export_signature_tests_parts/part_08.rs");
include!("export_signature_tests_parts/part_09.rs");
include!("export_signature_tests_parts/part_10.rs");
include!("export_signature_tests_parts/part_11.rs");
include!("export_signature_tests_parts/part_12.rs");
include!("export_signature_tests_parts/part_13.rs");
