//! Integration tests for malformed class member emit recovery.

use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;
use tsz_emitter::{context::emit::EmitContext, lowering::LoweringPass};
use tsz_parser::ParserState;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

fn print_with_printer_options(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn print_with_cli_style_pipeline(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.set_source_map_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn public_empty_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {};\n}\n");
    assert_eq!(output, "class C {\n}\n{ }\n;\n");
}

#[test]
fn public_index_signature_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {[name:string]:VariableDeclaration};\n}\n");
    assert_eq!(
        output,
        "class C {\n}\n{\n    [name, string];\n    VariableDeclaration;\n}\n;\n"
    );
}

#[test]
fn es2015_type_only_class_property_is_erased() {
    let output = print_es2015("class C {\n    foo: string;\n}\n");
    assert_eq!(output, "class C {\n}\n");
}

#[test]
fn computed_string_field_preserves_source_quotes_with_constructor() {
    let output = print_with_printer_options(
        "class C {\n    ['this'] = '';\n    constructor() {}\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("this['this'] = '';"),
        "Computed string field should preserve its source quote style.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Computed string field should not be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn cli_style_computed_string_field_preserves_source_quotes_with_crlf() {
    let source = "class C {\r\n    data = { foo: '' };\r\n    ['this'] = '';\r\n    constructor() {\r\n        var copy: typeof this.data = { foo: '' };\r\n    }\r\n}\r\n";
    let output = print_with_cli_style_pipeline(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("this['this'] = '';"),
        "Computed string field should preserve its source quote style.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Computed string field should not be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn downlevel_define_type_only_computed_property_does_not_allocate_temp() {
    let output = print_with_printer_options(
        "class C {\n    [side.effect]: string;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("_a = side.effect"),
        "Type-only computed property should not allocate an unused temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}\nside.effect;"),
        "Side-effectful computed property expression should still be emitted.\nOutput:\n{output}"
    );
}
