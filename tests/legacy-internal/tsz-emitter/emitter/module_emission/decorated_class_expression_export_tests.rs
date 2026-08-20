use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_commonjs_es2022_import_helpers(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2022,
        import_helpers: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn commonjs_exported_anonymous_tc39_decorated_class_expression_inlines_export() {
    let output = emit_commonjs_es2022_import_helpers(
        r#"declare var dec: any;
export const C = @dec class {};
"#,
    );

    assert!(
        output.contains("exports.C = (() => {"),
        "Decorated class expression exports should assign the transformed IIFE directly.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const C = (() => {") && !output.contains("exports.C = C;"),
        "Decorated class expression exports should not keep a redundant local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"C\")"),
        "Anonymous class expression should still receive named-evaluation metadata.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_exported_named_tc39_decorated_class_expression_inlines_export() {
    let output = emit_commonjs_es2022_import_helpers(
        r#"declare var dec: any;
export const C = @dec class C {};
"#,
    );

    assert!(
        output.contains("exports.C = (() => {"),
        "Named decorated class expression exports should assign the transformed IIFE directly.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const C = (() => {") && !output.contains("exports.C = C;"),
        "Named decorated class expression exports should not keep a redundant local binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return C = _classThis;"),
        "The named class expression should keep its inner class name assignment.\nOutput:\n{output}"
    );
}
