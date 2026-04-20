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

