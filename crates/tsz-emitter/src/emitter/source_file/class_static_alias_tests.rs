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

// A concise-body arrow returning an anonymous `class extends <base>` with a
// static field (the mixin pattern) is lowered to a single-line block body
// `(...) => { var _a; return _a = class extends <base> {...}, _a.<field> = ...,
// _a; }`. tsc keeps the synthesized `{ var _a; return` on the arrow's `=>`
// line and does *not* parenthesize the comma wrapper (it is the direct operand
// of the synthesized `return`). These assertions are keyed on the structural
// shape, not on the chosen identifier spellings, so they hold for any base /
// parameter / field names.

#[test]
fn es2015_mixin_arrow_concise_class_expr_is_single_line_block_without_paren() {
    let source = "const Mixin = (Sup) =>\n    class extends Sup {\n        static label = \"x\";\n        go() {}\n    }\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("(Sup) => { var _a; return _a = class extends Sup {"),
        "Mixin arrow body should be a single-line `{{ var _a; return _a = class ...`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return (_a = "),
        "Comma wrapper that is the direct `return` operand must not be parenthesized.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.label = \"x\","),
        "Static field initializer should be a comma item on the wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a; };"),
        "Single-line block should close with `_a; }};` on one line.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_mixin_arrow_renamed_param_and_base_keeps_single_line_block() {
    // Same structural shape, different parameter / base / field spellings: the
    // fix must not depend on the chosen identifiers.
    let source = "const make = (B) =>\n    class extends B {\n        static tag = 1;\n        run() {}\n    }\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("(B) => { var _a; return _a = class extends B {"),
        "Renamed mixin should still emit a single-line block body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return (_a = "),
        "Renamed mixin comma wrapper must not be parenthesized after `return`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.tag = 1,") && output.contains("_a; };"),
        "Renamed mixin static field should be a comma item closing with `_a; }};`.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_mixin_arrow_typed_param_and_return_keeps_single_line_block() {
    // Annotated mixin (type parameter + parameter type + return type) lowers to
    // the same runtime shape once types are erased.
    let source = concat!(
        "type Ctor<T> = new (...a: any[]) => T;\n",
        "const Printable = <T extends Ctor<object>>(superClass: T): Ctor<object> & { message: string } & T =>\n",
        "    class extends superClass {\n",
        "        static message = \"hello\";\n",
        "        print() {}\n",
        "    }\n",
    );
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("(superClass) => { var _a; return _a = class extends superClass {"),
        "Annotated mixin should erase types and emit a single-line block body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return (_a = "),
        "Annotated mixin comma wrapper must not be parenthesized after `return`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.message = \"hello\","),
        "Annotated mixin static field should be a comma item.\nOutput:\n{output}"
    );
}
