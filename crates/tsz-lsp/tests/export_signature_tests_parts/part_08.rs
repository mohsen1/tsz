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

