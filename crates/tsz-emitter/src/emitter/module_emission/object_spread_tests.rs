use crate::context::emit::EmitContext;
use crate::emitter::{Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn es5_object_spread_assign_operands_keep_simple_property_groups_compact() {
    let source = r#"function f(t, obj, a) {
    let x05 = { a: 5, b: "hi", ...t };
    let x06 = { ...t, a: 5, b: "hi" };
    let x07 = { a: 5, b: "hi", ...t, c: true, ...obj };
    let x08 = { ...t, a, b: "hi" };
}
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    for expected in [
        "var x05 = __assign({ a: 5, b: \"hi\" }, t);",
        "var x06 = __assign(__assign({}, t), { a: 5, b: \"hi\" });",
        "var x07 = __assign(__assign(__assign({ a: 5, b: \"hi\" }, t), { c: true }), obj);",
        "var x08 = __assign(__assign({}, t), { a: a, b: \"hi\" });",
    ] {
        assert!(
            output.contains(expected),
            "ES5 object-spread lowering should emit compact simple property groups in __assign operands.\nExpected: {expected}\nOutput:\n{output}"
        );
    }
}

#[test]
fn es5_object_spread_assign_operands_preserve_boundary_comments() {
    let source = r#"function f(t) {
    let x09 = { a: 1 /* keep trailing */, ...t };
    let x10 = { /* keep leading */ a: 1, ...t };
    let x11 = { ...t, a: 1 /* keep final */ };
}
"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    for expected in [
        "/* keep trailing */",
        "/* keep leading */",
        "/* keep final */",
    ] {
        assert!(
            output.contains(expected),
            "ES5 object-spread lowering should preserve boundary comments instead of compacting them away.\nExpected comment: {expected}\nOutput:\n{output}"
        );
    }
}
