#[test]
fn test_export_signature_recompute_is_idempotent() {
    let source = "export const x = 1;\nexport function y() {}\nexport class Z {}";
    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig1 = ExportSignature::compute(&binder, file_name);
    let sig2 = ExportSignature::compute(&binder, file_name);
    let sig3 = ExportSignature::compute(&binder, file_name);

    assert_eq!(
        sig1, sig2,
        "Recomputed signature should be identical (1 vs 2)"
    );
    assert_eq!(
        sig2, sig3,
        "Recomputed signature should be identical (2 vs 3)"
    );
}

#[test]
fn test_export_only_type_keyword_preserves_on_body_change() {
    // export type should not change signature when the type body stays the same name/kind
    let source_a = "export type MyType = string;";
    let source_b = "export type MyType = number;";

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

    // The signature is based on name and kind, not the type definition body,
    // so changing string->number should preserve the signature.
    assert_eq!(
        sig_a, sig_b,
        "Changing type alias definition body should not change export signature"
    );
}

#[test]
fn test_export_enum_adding_member_preserves_signature() {
    // Adding enum members doesn't change the export name set
    let source_a = "export enum Color { Red }";
    let source_b = "export enum Color { Red, Green, Blue }";

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

    // The signature hashes exported name + flags, not individual members
    assert_eq!(
        sig_a, sig_b,
        "Adding enum members should not change export signature"
    );
}

#[test]
fn test_export_function_return_type_change_preserves_signature() {
    let source_a = "export function foo(): string { return ''; }";
    let source_b = "export function foo(): number { return 0; }";

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

    // Signature is based on name+flags, not return type
    assert_eq!(
        sig_a, sig_b,
        "Changing function return type should not change export signature"
    );
}

#[test]
fn test_export_adding_private_namespace_preserves_signature() {
    let source_a = "export const x = 1;";
    let source_b = "namespace InternalNS { export const y = 2; }\nexport const x = 1;";

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
        "Adding a private namespace should not change export signature"
    );
}

#[test]
fn test_export_class_method_body_change_preserves_signature() {
    let source_a = "export class Svc { run() { return 1; } }";
    let source_b = "export class Svc { run() { return 2; } }";

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
        "Changing class method body should not change export signature"
    );
}

#[test]
fn test_export_multiple_types_different_kinds_changes_signature() {
    // Going from exporting a function to exporting a class with the same name
    // should change because the symbol flags differ
    let source_a = "export function Thing() {}";
    let source_b = "export class Thing {}";

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
        "Changing export kind (function -> class) with same name should change signature"
    );
}

#[test]
fn test_export_signature_no_exports_same_file_name() {
    // Two different sources with zero exports should have the same signature
    let source_a = "const a = 1;";
    let source_b = "function internal() { return 42; }";

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
        "Two files with no exports should have the same signature"
    );
}

#[test]
fn test_export_signature_type_alias() {
    let source = "export type ID = string;";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_interface() {
    let source = "export interface Foo { x: number; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

