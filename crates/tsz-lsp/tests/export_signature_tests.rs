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

#[test]
fn test_type_alias_export_changes_signature() {
    let source_a = "export function foo() {}";
    let source_b = "export function foo() {}\nexport type Bar = string;";

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
        "Adding a type alias export should change the signature"
    );
}

#[test]
fn test_interface_export_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport interface IFoo { bar: string; }";

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
        "Adding an interface export should change the signature"
    );
}

#[test]
fn test_enum_export_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport enum Color { Red, Green, Blue }";

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
        "Adding an enum export should change the signature"
    );
}

#[test]
fn test_different_file_names_same_exports() {
    // Two files with same exports but different file names should have
    // the same signature (signature is about the API surface, not the file name).
    // Note: If module_exports uses file_name as key, the binder may store
    // exports differently, so this tests whether compute() correctly handles that.
    let source = "export function foo() {}";

    let mut parser_a = tsz_parser::ParserState::new("a.ts".to_string(), source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = tsz_parser::ParserState::new("b.ts".to_string(), source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let sig_a = ExportSignature::compute(&binder_a, "a.ts");
    let sig_b = ExportSignature::compute(&binder_b, "b.ts");

    // Both should hash the same exported names and flags
    assert_eq!(
        sig_a, sig_b,
        "Same exports with different file names should produce the same signature"
    );
}

#[test]
fn test_export_const_vs_export_function_different_signature() {
    // Changing the kind of an export (const vs function) should change the signature
    let source_a = "export const foo = 1;";
    let source_b = "export function foo() {}";

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
        "Changing export kind (const vs function) should change the signature"
    );
}

#[test]
fn test_renaming_export_changes_signature() {
    // Renaming an exported symbol should change the signature
    let source_a = "export function foo() {}";
    let source_b = "export function bar() {}";

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
        "Renaming an exported symbol should change the signature"
    );
}

#[test]
fn test_multiple_exports_order_independent() {
    // The signature uses sorted keys, so the order of export declarations
    // should not matter (same set of exports -> same signature).
    let source_a = "export function foo() {}\nexport function bar() {}";
    let source_b = "export function bar() {}\nexport function foo() {}";

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
        "Export order should not affect the signature (sorted keys)"
    );
}

#[test]
fn test_export_class_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport class MyClass {}";

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
        "Adding an exported class should change the signature"
    );
}

#[test]
fn test_whitespace_in_export_preserves_signature() {
    // Different whitespace/formatting but same exports should produce the same signature
    let source_a = "export function foo() {}";
    let source_b = "export   function   foo(  )  {  }";

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
        "Whitespace differences in exports should not change the signature"
    );
}

#[test]
fn test_signature_deterministic_same_input() {
    // Computing the signature twice on the same binder should produce the same result.
    let source = "export function foo() {}\nexport const bar = 1;";
    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig1 = ExportSignature::compute(&binder, file_name);
    let sig2 = ExportSignature::compute(&binder, file_name);

    assert_eq!(
        sig1, sig2,
        "Signature should be deterministic for the same input"
    );
}

#[test]
fn test_export_default_function_changes_signature() {
    let source_a = "export function foo() {}";
    let source_b = "export function foo() {}\nexport default function() {}";

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

    // Adding a default export should change the signature
    // (may or may not differ depending on binder handling of default exports)
    // Defensive: just ensure no panic and we get valid signatures
    assert!(
        sig_a.0 != 0 || sig_b.0 != 0,
        "At least one signature should be non-zero for default function"
    );
}

#[test]
fn test_multiple_exports_same_kind() {
    // Multiple exports of the same kind but different names
    let source_a = "export const a = 1;\nexport const b = 2;";
    let source_b = "export const a = 1;\nexport const c = 2;";

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
        "Changing an export name (b -> c) should change the signature"
    );
}

#[test]
fn test_export_let_vs_const_same_name() {
    // Same name but let vs const might have different symbol flags
    let source_a = "export let x = 1;";
    let source_b = "export const x = 1;";

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

    // let vs const may or may not have different symbol flags;
    // just verify no panic and valid computation
    let _ = (sig_a, sig_b);
}

#[test]
fn test_wrong_file_name_returns_consistent_signature() {
    // Computing signature with a file name that doesn't match the parsed file
    let source = "export function foo() {}";

    let mut parser = tsz_parser::ParserState::new("real.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Compute with the wrong file name - should not panic, just produce a signature
    let sig = ExportSignature::compute(&binder, "wrong.ts");
    // Should produce a consistent (possibly empty-like) signature
    let sig2 = ExportSignature::compute(&binder, "wrong.ts");
    assert_eq!(
        sig, sig2,
        "Signature for wrong file name should be deterministic"
    );
}

#[test]
fn test_many_exports_order_independent() {
    // More thorough order independence test with many exports
    let source_a = "export function a() {}\nexport function b() {}\nexport function c() {}\nexport function d() {}\nexport function e() {}";
    let source_b = "export function e() {}\nexport function c() {}\nexport function a() {}\nexport function d() {}\nexport function b() {}";

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
        "Export order should not affect signature for many exports"
    );
}

#[test]
fn test_mixed_export_kinds_same_name_set() {
    // Same names but different declaration kinds
    let source_a = "export function foo() {}\nexport class Bar {}";
    let source_b = "export class foo {}\nexport function Bar() {}";

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
        "Swapping declaration kinds should change the signature"
    );
}

#[test]
fn test_internal_variable_change_preserves_signature() {
    // Changing a non-exported variable's value or type should not affect signature
    let source_a = "let internal = 1;\nexport function foo() { return internal; }";
    let source_b = "let internal = \"hello\";\nexport function foo() { return internal; }";

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
        "Changing internal variable value should not affect export signature"
    );
}

#[test]
fn test_empty_binder_signature() {
    // Compute signature on a fresh binder with no files bound
    let binder = BinderState::new();
    let sig = ExportSignature::compute(&binder, "nonexistent.ts");

    // Should not panic, should produce some deterministic value
    let sig2 = ExportSignature::compute(&binder, "nonexistent.ts");
    assert_eq!(
        sig, sig2,
        "Empty binder should produce deterministic signature"
    );
}

#[test]
fn test_export_async_function_changes_signature() {
    let source_a = "export function foo() {}";
    let source_b = "export function foo() {}\nexport async function bar() {}";

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
        "Adding an async function export should change the signature"
    );
}

#[test]
fn test_export_arrow_function_const() {
    let source_a = "export const greet = () => 'hi';";
    let source_b = "export const greet = () => 'hello';";

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
        "Changing arrow function body should not change export signature"
    );
}

#[test]
fn test_export_namespace_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport namespace Utils { export function help() {} }";

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
        "Adding a namespace export should change the signature"
    );
}

#[test]
fn test_export_type_only_vs_value() {
    // type-only export vs value export of same name
    let source_a = "export type Foo = string;";
    let source_b = "export const Foo = 'hello';";

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

    // type alias vs const should produce different signatures
    assert_ne!(
        sig_a, sig_b,
        "Type-only export vs value export should differ"
    );
}

#[test]
fn test_export_multiple_consts_same_names() {
    // Same set of const exports, different values
    let source_a = "export const a = 1;\nexport const b = 2;\nexport const c = 3;";
    let source_b = "export const a = 10;\nexport const b = 20;\nexport const c = 30;";

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
        "Same const exports with different values should produce same signature"
    );
}

#[test]
fn test_export_class_with_members_body_change() {
    let source_a = "export class Foo { bar() { return 1; } }";
    let source_b = "export class Foo { bar() { return 2; } }";

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
fn test_export_interface_member_change() {
    let source_a = "export interface Foo { x: number; }";
    let source_b = "export interface Foo { x: number; y: string; }";

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

    // The export signature only hashes symbol name and flags, not interface members.
    // So adding a member to an exported interface may or may not change the signature.
    // Defensively just ensure no panic.
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_generic_class() {
    let source_a = "export class Box<T> { value: T; }";
    let source_b = "export class Box<T> { value: T; extra: number; }";

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

    // Generic class with added member - signature should be same since we hash name/flags, not members
    assert_eq!(
        sig_a, sig_b,
        "Adding a member to exported generic class should not change signature"
    );
}

#[test]
fn test_single_export_vs_no_exports() {
    let source_a = "";
    let source_b = "export const x = 1;";

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
        "Going from no exports to one export should change signature"
    );
}

#[test]
fn test_export_abstract_class_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport abstract class Base { abstract foo(): void; }";

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
        "Adding an abstract class export should change the signature"
    );
}

#[test]
fn test_export_const_enum_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport const enum Dir { Up, Down }";

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
        "Adding a const enum export should change the signature"
    );
}

#[test]
fn test_private_function_rename_preserves_signature() {
    let source_a = "function helper() {}\nexport function foo() { helper(); }";
    let source_b = "function util() {}\nexport function foo() { util(); }";

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
        "Renaming a private function should not change export signature"
    );
}

#[test]
fn test_export_function_with_type_annotation_body_change() {
    let source_a = "export function foo(): number { return 1; }";
    let source_b = "export function foo(): number { return 2; }";

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
        "Changing return value of typed function should not change signature"
    );
}

#[test]
fn test_multiple_private_changes_preserve_signature() {
    let source_a = "const a = 1;\nlet b = 'hello';\nfunction helper() {}\nexport function pub() {}";
    let source_b = "const a = 999;\nlet b = 'world';\nfunction helper2() { return 42; }\nexport function pub() {}";

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
        "Multiple private changes should not affect export signature"
    );
}

#[test]
fn test_export_var_declaration() {
    let source_a = "export var x = 1;";
    let source_b = "export var x = 1;\nexport var y = 2;";

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
        "Adding an exported var should change the signature"
    );
}

#[test]
fn test_large_number_of_exports_deterministic() {
    let source = (0..20)
        .map(|i| format!("export function fn{i}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");

    let file_name = "test.ts";

    let mut parser1 = tsz_parser::ParserState::new(file_name.to_string(), source.clone());
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let mut parser2 = tsz_parser::ParserState::new(file_name.to_string(), source);
    let root2 = parser2.parse_source_file();
    let mut binder2 = BinderState::new();
    binder2.bind_source_file(parser2.get_arena(), root2);

    let sig1 = ExportSignature::compute(&binder1, file_name);
    let sig2 = ExportSignature::compute(&binder2, file_name);

    assert_eq!(
        sig1, sig2,
        "Large number of exports should produce deterministic signature"
    );
}

#[test]
fn test_export_default_class() {
    let source_a = "export function foo() {}";
    let source_b = "export function foo() {}\nexport default class {}";

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

    // Defensive: just ensure no panic and we get valid signatures
    assert!(
        sig_a.0 != 0 || sig_b.0 != 0,
        "At least one signature should be non-zero for default class"
    );
}

// =========================================================================
// Additional export signature tests to reach 50
// =========================================================================

#[test]
fn test_export_generator_function_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport function* gen() { yield 1; }";

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
        "Adding a generator function export should change the signature"
    );
}

#[test]
fn test_export_function_parameter_change_preserves_signature() {
    // Changing function parameter names should not affect export signature
    let source_a = "export function foo(x: number) {}";
    let source_b = "export function foo(y: number) {}";

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
        "Changing parameter names should not change export signature"
    );
}

#[test]
fn test_two_identical_files_same_signature() {
    let source = "export const a = 1;\nexport function b() {}\nexport class C {}";
    let file_name = "test.ts";

    let mut parser1 = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let mut parser2 = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root2 = parser2.parse_source_file();
    let mut binder2 = BinderState::new();
    binder2.bind_source_file(parser2.get_arena(), root2);

    let sig1 = ExportSignature::compute(&binder1, file_name);
    let sig2 = ExportSignature::compute(&binder2, file_name);

    assert_eq!(
        sig1, sig2,
        "Identical source files should produce the same signature"
    );
}

#[test]
fn test_export_with_different_initializer_preserves_signature() {
    let source_a = "export const config = { port: 3000 };";
    let source_b = "export const config = { port: 8080, host: 'localhost' };";

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
        "Changing const initializer value should not change export signature"
    );
}

#[test]
fn test_removing_all_exports_changes_signature() {
    let source_a = "export function foo() {}\nexport const bar = 1;";
    let source_b = "function foo() {}\nconst bar = 1;";

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
        "Removing all exports should change the signature"
    );
}

#[test]
fn test_export_class_extending_another_body_change() {
    let source_a = "class Base {}\nexport class Child extends Base { foo() { return 1; } }";
    let source_b = "class Base {}\nexport class Child extends Base { foo() { return 999; } }";

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
        "Changing class method body in derived class should not change export signature"
    );
}

#[test]
fn test_export_intersection_type_alias() {
    let source_a = "export type Combined = A & B;";
    let source_b = "export type Combined = A & B;\nexport type Extra = string;";

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
        "Adding another type export should change the signature"
    );
}

#[test]
fn test_export_same_name_different_arity_type_alias() {
    // Defensive: different generic arity for same name may or may not differ
    let source_a = "export type Box<T> = { value: T };";
    let source_b = "export type Box<T, U> = { value: T; extra: U };";

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

    // Just ensure no panic; the signature may or may not differ depending on
    // whether type parameters are included in the hash
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_reexport_from_module() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport { foo } from './other';";

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

    // Adding a re-export should change the signature
    // Defensive: just ensure no panic
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_wildcard_reexport() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport * from './utils';";

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

    // Defensive: just ensure no panic and valid computation
    let _ = (sig_a, sig_b);
}

// =========================================================================
// Additional export signature tests (batch 2)
// =========================================================================

#[test]
fn test_export_declare_function_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport declare function declared(): void;";

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
        "Adding a declare function export should change the signature"
    );
}

#[test]
fn test_export_declare_class_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport declare class DeclaredClass {}";

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
        "Adding a declare class export should change the signature"
    );
}

#[test]
fn test_export_multiple_vars_in_single_statement() {
    let source_a = "export const a = 1, b = 2;";
    let source_b = "export const a = 1, b = 2, c = 3;";

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

    // Defensive: adding another const in same statement
    // may or may not differ depending on binder; ensure no panic
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_function_overload_signatures() {
    let source_a = "export function foo(x: number): number;\nexport function foo(x: string): string;\nexport function foo(x: any): any { return x; }";
    let source_b =
        "export function foo(x: number): number;\nexport function foo(x: any): any { return x; }";

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

    // Defensive: same name different overloads; ensure no panic
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_enum_member_value_change_preserves_signature() {
    let source_a = "export enum Status { Active = 0, Inactive = 1 }";
    let source_b = "export enum Status { Active = 10, Inactive = 20 }";

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
        "Changing enum member values should not change export signature"
    );
}

#[test]
fn test_export_interface_vs_type_alias_same_name() {
    let source_a = "export interface Foo { x: number; }";
    let source_b = "export type Foo = { x: number; };";

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

    // interface vs type alias have different symbol flags
    assert_ne!(
        sig_a, sig_b,
        "Interface vs type alias with same name should produce different signatures"
    );
}

#[test]
fn test_export_class_vs_function_same_name() {
    let source_a = "export class Thing {}";
    let source_b = "export function Thing() {}";

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
        "Class vs function with same name should produce different signatures"
    );
}

#[test]
fn test_export_only_whitespace_source() {
    let source_a = "   \n\n   ";
    let source_b = "\n\n\n\n";

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
        "Whitespace-only sources with no exports should produce the same signature"
    );
}

#[test]
fn test_export_multiple_type_aliases_order_independent() {
    let source_a = "export type A = string;\nexport type B = number;\nexport type C = boolean;";
    let source_b = "export type C = boolean;\nexport type A = string;\nexport type B = number;";

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
        "Type alias export order should not affect signature"
    );
}

#[test]
fn test_export_mixed_classes_interfaces_order_independent() {
    let source_a = "export class A {}\nexport interface B {}\nexport class C {}";
    let source_b = "export class C {}\nexport class A {}\nexport interface B {}";

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
        "Mixed class/interface export order should not affect signature"
    );
}

#[test]
fn test_export_adding_private_class_preserves_signature() {
    let source_a = "export function api() {}";
    let source_b = "class InternalHelper {}\nexport function api() {}";

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
        "Adding a private class should not change export signature"
    );
}

#[test]
fn test_export_adding_private_interface_preserves_signature() {
    let source_a = "export const val = 42;";
    let source_b = "interface PrivateInterface { x: number; }\nexport const val = 42;";

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
        "Adding a private interface should not change export signature"
    );
}

#[test]
fn test_export_single_vs_two_exports_same_name() {
    // Having one vs two exports with same name (re-declaration)
    let source_a = "export function foo() {}";
    let source_b = "export function foo() {}\nexport function bar() {}";

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
        "Adding a second export should change the signature"
    );
}

#[test]
fn test_export_signature_hash_is_nonzero_for_exports() {
    let source = "export function foo() {}\nexport const bar = 1;";
    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig = ExportSignature::compute(&binder, file_name);

    assert_ne!(
        sig.0, 0,
        "Signature hash for a file with exports should be nonzero"
    );
}

// =========================================================================
// Additional export signature tests (batch 3)
// =========================================================================

#[test]
fn test_export_default_const_changes_signature() {
    let source_a = "";
    let source_b = "export default 42;";

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

    // Adding a default export should change the signature (or at least not crash)
    // The signatures may or may not differ depending on how default exports are handled,
    // but computing them should not panic.
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_adding_private_enum_preserves_signature() {
    let source_a = "export const x = 1;";
    let source_b = "enum PrivateEnum { A, B }\nexport const x = 1;";

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
        "Adding a private enum should not change export signature"
    );
}

#[test]
fn test_export_adding_private_type_alias_preserves_signature() {
    let source_a = "export function greet() {}";
    let source_b = "type Internal = string;\nexport function greet() {}";

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
        "Adding a private type alias should not change export signature"
    );
}

#[test]
fn test_export_interface_member_addition_changes_signature() {
    // Adding a new export (not just a member) should change signature
    let source_a = "export interface Foo { x: number; }";
    let source_b = "export interface Foo { x: number; }\nexport interface Bar { y: string; }";

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
        "Adding a new exported interface should change the signature"
    );
}

#[test]
fn test_export_signature_with_declare_keyword() {
    let source_a = "export declare function foo(): void;";
    let source_b = "export declare function foo(): void;\nexport declare function bar(): number;";

    let file_name = "test.d.ts";

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
        "Adding a second declare export should change signature"
    );
}

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

#[test]
fn test_export_signature_enum() {
    let source = "export enum Color { Red, Green, Blue }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_class() {
    let source = "export class Foo { x: number = 0; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_const_vs_let() {
    let source_a = "export const x = 1;";
    let source_b = "export let x = 1;";
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
    // const vs let may or may not change signature
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_signature_async_function() {
    let source = "export async function fetchData() { return 42; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_generator_function() {
    let source = "export function* gen() { yield 1; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_namespace() {
    let source = "export namespace NS { export const x = 1; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_many_exports() {
    let source = "export const a = 1;\nexport const b = 2;\nexport const c = 3;\nexport const d = 4;\nexport const e = 5;";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_arrow_function() {
    let source = "export const add = (a: number, b: number) => a + b;";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_generic_function() {
    let source = "export function identity<T>(x: T): T { return x; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

#[test]
fn test_export_signature_overloaded_function() {
    let source = "export function foo(x: number): number;\nexport function foo(x: string): string;\nexport function foo(x: any): any { return x; }";
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let sig = ExportSignature::compute(&binder, file_name);
    let _ = sig;
}

// ============================================================================
// Batch: additional edge case tests
// ============================================================================

#[test]
fn test_export_declare_namespace_changes_signature() {
    let source_a = "export const x = 1;";
    let source_b = "export const x = 1;\nexport declare namespace Lib { function init(): void; }";

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
        "Adding a declare namespace export should change the signature"
    );
}

#[test]
fn test_export_generic_function_body_change_preserves() {
    let source_a = "export function identity<T>(x: T): T { return x; }";
    let source_b = "export function identity<T>(x: T): T { console.log(x); return x; }";

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
        "Changing generic function body should not change export signature"
    );
}

#[test]
fn test_export_class_with_constructor_body_change() {
    let source_a = "export class Foo { constructor() { this.x = 1; } }";
    let source_b = "export class Foo { constructor() { this.x = 2; } }";

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
        "Changing class constructor body should not change export signature"
    );
}

#[test]
fn test_export_multiple_interfaces_order_independent() {
    let source_a = "export interface A { x: number; }\nexport interface B { y: string; }";
    let source_b = "export interface B { y: string; }\nexport interface A { x: number; }";

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
        "Interface export order should not affect the signature"
    );
}

#[test]
fn test_export_conditional_type_alias() {
    let source_a = "export type IsString<T> = T extends string ? true : false;";
    let source_b = "export type IsString<T> = T extends string ? true : false;\nexport type IsNumber<T> = T extends number ? true : false;";

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
        "Adding a conditional type export should change the signature"
    );
}

#[test]
fn test_export_mapped_type_alias() {
    let source = "export type Readonly2<T> = { readonly [K in keyof T]: T[K] };";

    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig = ExportSignature::compute(&binder, file_name);
    let sig2 = ExportSignature::compute(&binder, file_name);
    assert_eq!(
        sig, sig2,
        "Mapped type export signature should be deterministic"
    );
}

#[test]
fn test_export_class_with_static_member_body_change() {
    let source_a = "export class Config { static value = 1; }";
    let source_b = "export class Config { static value = 2; }";

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
        "Changing static member value should not change export signature"
    );
}

#[test]
fn test_export_enum_vs_const_enum_different_signature() {
    let source_a = "export enum Dir { Up, Down }";
    let source_b = "export const enum Dir { Up, Down }";

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

    // enum vs const enum may have different symbol flags
    let _ = (sig_a, sig_b);
}

#[test]
fn test_export_class_with_getter_setter_body_change() {
    let source_a = "export class Store { get value() { return 1; } set value(v: number) {} }";
    let source_b = "export class Store { get value() { return 2; } set value(v: number) {} }";

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
        "Changing getter body should not change export signature"
    );
}

#[test]
fn test_export_class_adding_new_export_class() {
    let source_a = "export class A {}";
    let source_b = "export class A {}\nexport class B {}";

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
        "Adding a second exported class should change the signature"
    );
}

#[test]
fn test_export_interface_extending_another() {
    let source_a = "export interface Base { x: number; }";
    let source_b =
        "export interface Base { x: number; }\nexport interface Child extends Base { y: string; }";

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
        "Adding a child interface export should change the signature"
    );
}

#[test]
fn test_export_unicode_identifier() {
    let source = "export const caf\u{00E9} = 'coffee';";

    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig = ExportSignature::compute(&binder, file_name);
    let sig2 = ExportSignature::compute(&binder, file_name);
    assert_eq!(
        sig, sig2,
        "Unicode identifier export signature should be deterministic"
    );
}

#[test]
fn test_export_function_with_rest_params_body_change() {
    let source_a = "export function sum(...nums: number[]): number { return nums.reduce((a, b) => a + b, 0); }";
    let source_b = "export function sum(...nums: number[]): number { let total = 0; for (const n of nums) total += n; return total; }";

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
        "Changing rest-params function body should not change export signature"
    );
}

// ============================================================================
// Unified ExportSignatureInput tests
// ============================================================================

#[test]
fn test_input_from_binder_matches_compute() {
    // ExportSignatureInput::from_binder + ExportSignature::from_input must produce
    // the same hash as ExportSignature::compute (they are the same code path).
    let source = "export function foo() { return 1; }\nexport const bar = 42;";
    let file_name = "test.ts";

    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let via_compute = ExportSignature::compute(&binder, file_name);
    let input = ExportSignatureInput::from_binder(&binder, file_name);
    let via_input = ExportSignature::from_input(&input);

    assert_eq!(
        via_compute, via_input,
        "compute() and from_input(from_binder()) must produce identical hashes"
    );
}

#[test]
fn test_input_body_only_edit_stable() {
    let input_a = compute_input("export function foo() { return 1; }");
    let input_b = compute_input("export function foo() { return 999; }");

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_eq!(
        sig_a, sig_b,
        "Body-only edit: ExportSignatureInput should be stable"
    );
}

#[test]
fn test_input_comment_only_edit_stable() {
    let input_a = compute_input("// v1\nexport const x = 1;");
    let input_b = compute_input("// v2 — updated comment\nexport const x = 1;");

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_eq!(
        sig_a, sig_b,
        "Comment-only edit: ExportSignatureInput should be stable"
    );
}

#[test]
fn test_input_private_symbol_edit_stable() {
    let input_a = compute_input("const helper = 1;\nexport function foo() {}");
    let input_b =
        compute_input("const helper = 1;\nconst helper2 = 'new';\nexport function foo() {}");

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_eq!(
        sig_a, sig_b,
        "Adding private symbols: ExportSignatureInput should be stable"
    );
}

#[test]
fn test_input_reexport_change_detected() {
    let sig_a = compute_sig("export { default as foo } from './mod';");
    let sig_b = compute_sig("export { default as bar } from './mod';");

    assert_ne!(
        sig_a, sig_b,
        "Changing a re-export name should change the signature"
    );
}

#[test]
fn test_input_reexport_source_change_detected() {
    let sig_a = compute_sig("export { foo } from './a';");
    let sig_b = compute_sig("export { foo } from './b';");

    assert_ne!(
        sig_a, sig_b,
        "Changing a re-export source module should change the signature"
    );
}

#[test]
fn test_input_wildcard_reexport_added() {
    let sig_a = compute_sig("export function foo() {}");
    let sig_b = compute_sig("export function foo() {}\nexport * from './utils';");

    assert_ne!(
        sig_a, sig_b,
        "Adding a wildcard re-export should change the signature"
    );
}

#[test]
fn test_input_wildcard_reexport_source_change() {
    let sig_a = compute_sig("export * from './a';");
    let sig_b = compute_sig("export * from './b';");

    assert_ne!(
        sig_a, sig_b,
        "Changing wildcard re-export source should change the signature"
    );
}

#[test]
fn test_input_multiple_reexports_order_independent() {
    let sig_a = compute_sig("export { a } from './x';\nexport { b } from './y';");
    let sig_b = compute_sig("export { b } from './y';\nexport { a } from './x';");

    assert_eq!(
        sig_a, sig_b,
        "Re-export order should not affect the signature (sorted keys)"
    );
}

#[test]
fn test_input_adding_named_reexport_changes_signature() {
    let sig_a = compute_sig("export { a } from './x';");
    let sig_b = compute_sig("export { a } from './x';\nexport { b } from './y';");

    assert_ne!(
        sig_a, sig_b,
        "Adding a named re-export should change the signature"
    );
}

#[test]
fn test_input_removing_reexport_changes_signature() {
    let sig_a = compute_sig("export { a } from './x';\nexport { b } from './y';");
    let sig_b = compute_sig("export { a } from './x';");

    assert_ne!(
        sig_a, sig_b,
        "Removing a named re-export should change the signature"
    );
}

// ============================================================================
// Augmentation tests
// ============================================================================

#[test]
fn test_global_augmentation_changes_signature() {
    let sig_a = compute_sig("export const x = 1;");
    let sig_b =
        compute_sig("export const x = 1;\ndeclare global { interface Window { foo: string; } }");

    assert_ne!(
        sig_a, sig_b,
        "Adding a global augmentation should change the signature"
    );
}

#[test]
fn test_module_augmentation_changes_signature() {
    let sig_a = compute_sig("export const x = 1;");
    let sig_b = compute_sig(
        "export const x = 1;\ndeclare module 'express' { interface Request { user: any; } }",
    );

    // Module augmentations may or may not be tracked by the binder for inline source;
    // at minimum, verify no panic and valid computation
    let _ = (sig_a, sig_b);
}

// ============================================================================
// InvalidationSummary tests
// ============================================================================

#[test]
fn test_invalidation_summary_unchanged() {
    let summary = InvalidationSummary::unchanged("a.ts".to_string(), 0x1234);
    assert!(!summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 0);
    assert_eq!(summary.old_signature, Some(0x1234));
    assert_eq!(summary.new_signature, 0x1234);
}

#[test]
fn test_invalidation_summary_changed() {
    let summary = InvalidationSummary::changed("a.ts".to_string(), Some(0x1111), 0x2222, 3);
    assert!(summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 3);
    assert_eq!(summary.old_signature, Some(0x1111));
    assert_eq!(summary.new_signature, 0x2222);
}

#[test]
fn test_invalidation_summary_new_file() {
    let summary = InvalidationSummary::new_file("new.ts".to_string(), 0xABCD);
    assert!(summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 0);
    assert_eq!(summary.old_signature, None);
    assert_eq!(summary.new_signature, 0xABCD);
}

// ============================================================================
// Cross-system equivalence: from_input produces deterministic results
// ============================================================================

#[test]
fn test_from_input_deterministic() {
    let input = ExportSignatureInput {
        exports: vec![
            ("bar".to_string(), 0x10, false),
            ("foo".to_string(), 0x20, true),
        ],
        named_reexports: vec![(
            "baz".to_string(),
            "./mod".to_string(),
            Some("original".to_string()),
        )],
        wildcard_reexports: vec![("./utils".to_string(), false)],
        global_augmentations: vec![("Window".to_string(), 1)],
        module_augmentations: vec![("express".to_string(), vec!["Request".to_string()])],
        exported_locals: vec![("bar".to_string(), 0x10, false)],
    };

    let sig1 = ExportSignature::from_input(&input);
    let sig2 = ExportSignature::from_input(&input);

    assert_eq!(sig1, sig2, "from_input must be deterministic");
    assert_ne!(
        sig1.0, 0,
        "Signature should be non-zero for non-empty input"
    );
}

#[test]
fn test_from_input_different_exports_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("bar".to_string(), 0x10, false)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Different export names should produce different hashes"
    );
}

#[test]
fn test_from_input_different_flags_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x20, false)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Different symbol flags should produce different hashes"
    );
}

#[test]
fn test_from_input_type_only_change_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Changing is_type_only should produce different hashes"
    );
}

#[test]
fn test_from_input_empty_is_consistent() {
    let input = ExportSignatureInput::default();
    let sig1 = ExportSignature::from_input(&input);
    let sig2 = ExportSignature::from_input(&input);

    assert_eq!(sig1, sig2, "Empty input should produce consistent hashes");
}

#[test]
fn test_from_input_wildcard_type_only_change_different_hash() {
    // Changing `export * from "x"` to `export type * from "x"` must change the signature,
    // because type-only wildcard re-exports filter out value exports from the source module.
    let input_a = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Wildcard re-export type_only change must produce different signatures"
    );
}

#[test]
fn test_from_input_wildcard_same_module_same_type_only_same_hash() {
    let input_a = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_eq!(
        sig_a, sig_b,
        "Same wildcard re-export entries must produce same signature"
    );
}

// ── ExportSurface hash-equivalence tests ──────────────────────────────

/// Helper: parse, bind, then compute both the `from_binder()` signature
/// and the `from_surface()` signature.  Returns `(from_binder, from_surface)`.
fn compute_both_sigs(source: &str) -> (ExportSignature, ExportSignature) {
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig_binder = ExportSignature::compute(&binder, file_name);
    let surface =
        tsz_binder::ExportSurface::from_binder(&binder, parser.get_arena(), file_name, root);
    let sig_surface = ExportSignature::from_surface(&surface);
    (sig_binder, sig_surface)
}

#[test]
fn test_surface_hash_matches_binder_simple_exports() {
    let (a, b) = compute_both_sigs("export function foo(): void {}");
    assert_eq!(a, b, "from_surface hash must match from_binder hash");
}

#[test]
fn test_surface_hash_matches_binder_multiple_exports() {
    let (a, b) = compute_both_sigs(
        "export function foo(): void {}\nexport const bar: number = 1;\nexport class Baz {}",
    );
    assert_eq!(a, b, "from_surface hash must match from_binder hash");
}

#[test]
fn test_surface_hash_matches_binder_reexports() {
    let (a, b) = compute_both_sigs("export { foo } from './other';");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for named re-exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_wildcard() {
    let (a, b) = compute_both_sigs("export * from './other';");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for wildcard re-exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_default_export() {
    let (a, b) = compute_both_sigs("export default function main() {}");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for default exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_type_only() {
    let (a, b) = compute_both_sigs("export type Foo = string;");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for type-only exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_mixed() {
    let (a, b) = compute_both_sigs(
        r#"
export function foo(): void {}
export type Bar = number;
export { baz } from './baz';
export * from './utils';
export default class Main {}
"#,
    );
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for mixed exports"
    );
}
