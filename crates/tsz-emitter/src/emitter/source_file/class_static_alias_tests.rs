use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields: false,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn es2015_static_async_arrow_declares_class_alias_once() {
    let source = "class Test {\n    static member = async (x: string) => { };\n}\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert_eq!(
        output.matches("var _a;").count(),
        1,
        "ES2015 static async arrow class alias should be declared once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = Test;\nTest.member = (x) => __awaiter(void 0"),
        "Static async arrow should keep `void 0` as the awaiter receiver.\nOutput:\n{output}"
    );
}

#[test]
fn es5_static_async_arrow_uses_local_class_alias_as_generator_this() {
    let source = "class Test {\n    static member = async (x: string) => { };\n}\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        !output.starts_with("var _a;\n"),
        "ES5 class declarations should not emit an outer class-alias temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a;\n    _a = Test;"),
        "ES5 class IIFE should own the static initializer class alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__awaiter(void 0, void 0, void 0, function () { return __generator(_a"),
        "Static async arrow should pass the class alias to `__generator`, not `__awaiter`.\nOutput:\n{output}"
    );
}

#[test]
fn es5_static_block_reuses_surrounding_class_alias() {
    let source = "class Test {\n    static value = this.name;\n    static { this.value; }\n}\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("var _a;\n    _a = Test;"),
        "ES5 class IIFE should establish one static class alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _a = this;"),
        "Lowered static blocks should not shadow the class alias with a local receiver capture.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(function () {\n        _a.value;\n    })();"),
        "Static block `this` references should use the surrounding class alias.\nOutput:\n{output}"
    );
}
