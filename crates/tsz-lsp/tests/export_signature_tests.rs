use super::*;

#[test]
fn test_body_edit_preserves_signature() {
    // Two files with the same exports but different function bodies
    // should produce the same export signature.
    let source_a = "export function foo() { return 1; }";
    let source_b = "export function foo() { return 2; }";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_eq!(
        sig_a, sig_b,
        "Body-only edit should not change export signature"
    );
}

#[test]
fn test_adding_export_changes_signature() {
    let source_a = "export function foo() { return 1; }";
    let source_b = "export function foo() { return 1; }\nexport function bar() { return 2; }";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_ne!(sig_a, sig_b, "Adding an export should change the signature");
}

#[test]
fn test_removing_export_changes_signature() {
    let source_a = "export function foo() {}\nexport function bar() {}";
    let source_b = "export function foo() {}\nfunction bar() {}";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_ne!(
        sig_a, sig_b,
        "Removing an export should change the signature"
    );
}

#[test]
fn test_comment_edit_preserves_signature() {
    let source_a = "// version 1\nexport const x = 1;";
    let source_b = "// version 2\nexport const x = 1;";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_eq!(
        sig_a, sig_b,
        "Comment-only edit should not change export signature"
    );
}

#[test]
fn test_private_addition_preserves_signature() {
    let source_a = "export function foo() {}";
    let source_b = "const helper = 42;\nexport function foo() {}";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_eq!(
        sig_a, sig_b,
        "Adding a private symbol should not change export signature"
    );
}

#[test]
fn test_no_exports_consistent() {
    let source_a = "const x = 1;";
    let source_b = "const x = 2; const y = 3;";

    let file_name = "test.ts";

    let mut parser_a = tsz_parser::ParserState::new(file_name.to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new(file_name.to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, file_name);
    let sig_b = ExportSignature::compute(&binder_b, file_name);

    assert_eq!(
        sig_a, sig_b,
        "Files with no exports should have the same signature"
    );
}
