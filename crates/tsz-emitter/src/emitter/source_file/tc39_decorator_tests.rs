use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn default_tc39_decorated_private_method_body_uses_js_emitter() {
    let source = "\
declare var dec: any;
export default @dec class {
    @dec
    #foo(value: number) {
        const label: string = String(value);
        return label;
    }
}
";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        import_helpers: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("const label = String(value);"),
        "Default decorated private method body should be rendered through the JS emitter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("value: number") && !output.contains("label: string"),
        "Default decorated private method body must not copy TypeScript-only syntax.\nOutput:\n{output}"
    );
}
