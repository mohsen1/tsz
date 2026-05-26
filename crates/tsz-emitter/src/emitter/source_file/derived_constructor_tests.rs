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
fn derived_constructor_fields_follow_parenthesized_super_call() {
    let source = "class Base {}\nclass Plain extends Base {\n    prop = true;\n    constructor() {\n        (super());\n    }\n}\nclass AfterStatement extends Base {\n    prop = true;\n    constructor() {\n        this.touch;\n        (super());\n    }\n}\nclass BeforeStatement extends Base {\n    prop = true;\n    constructor() {\n        (super());\n        this.touch;\n    }\n}\nclass SuperArgument extends Base {\n    prop = true;\n    constructor() {\n        super(this);\n    }\n}\n";

    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("constructor() {\n        (super());\n        this.prop = true;\n    }"),
        "A parenthesized root super call should receive instance field initializers after the call.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "constructor() {\n        this.touch;\n        (super());\n        this.prop = true;\n    }"
        ),
        "Prefix statements before a parenthesized super call should remain before the initializer prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "constructor() {\n        (super());\n        this.prop = true;\n        this.touch;\n    }"
        ),
        "Statements after a parenthesized super call should remain after the initializer prologue.\nOutput:\n{output}"
    );

    let es5_output = emit(source, ScriptTarget::ES5);
    assert!(
        es5_output.contains(
            "function Plain() {\n        var _this = (_this = _super.call(this) || this);\n        _this.prop = true;\n        return _this;\n    }"
        ),
        "ES5 derived constructor lowering should preserve the parenthesized super assignment before field initializers.\nOutput:\n{es5_output}"
    );
    assert!(
        es5_output.contains(
            "function AfterStatement() {\n        var _this = this;\n        _this.touch;\n        (_this = _super.call(this) || this);\n        _this.prop = true;\n        return _this;\n    }"
        ),
        "ES5 derived constructor lowering should keep prefix statements before a parenthesized super call.\nOutput:\n{es5_output}"
    );
    assert!(
        es5_output.contains(
            "function BeforeStatement() {\n        var _this = (_this = _super.call(this) || this);\n        _this.prop = true;\n        _this.touch;\n        return _this;\n    }"
        ),
        "ES5 derived constructor lowering should place following statements after field initializers.\nOutput:\n{es5_output}"
    );
    assert!(
        es5_output.contains(
            "function SuperArgument() {\n        var _this = _super.call(this, _this) || this;\n        _this.prop = true;\n        return _this;\n    }"
        ),
        "ES5 derived constructor lowering should route `this` in super-call arguments through the constructor result temp.\nOutput:\n{es5_output}"
    );
}
