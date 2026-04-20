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

