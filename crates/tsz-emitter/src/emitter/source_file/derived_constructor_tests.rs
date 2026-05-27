use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    emit_with_options(
        source,
        PrinterOptions {
            target,
            ..Default::default()
        },
    )
}

fn emit_with_options(source: &str, options: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

#[test]
fn pre_super_nested_class_emits_legacy_decorators() {
    let source = "declare const decorate: any;\nclass Base {}\nclass Derived extends Base {\n    prop = true;\n    constructor() {\n        @decorate(this)\n        class Inner {\n            @decorate(this)\n            method() {}\n            @decorate(this)\n            prop;\n        }\n        super();\n    }\n}\n";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            legacy_decorators: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("Inner = __decorate([")
            && output.contains("decorate(this)")
            && output.contains("], Inner);"),
        "Nested classes lowered directly through ES5 class IR should still emit legacy class decorator calls.\nOutput:\n{output}"
    );
    assert!(
        output.contains("], Inner.prototype, \"method\", null);"),
        "Nested class methods lowered directly through ES5 class IR should still emit legacy method decorator calls.\nOutput:\n{output}"
    );
    assert!(
        output.contains("], Inner.prototype, \"prop\", void 0);"),
        "Nested class fields lowered directly through ES5 class IR should still emit legacy field decorator calls.\nOutput:\n{output}"
    );
}

#[test]
fn pre_super_this_capture_stops_at_ordinary_functions() {
    let source = "class Base {}\nclass FnDecl extends Base {\n    prop = true;\n    constructor() {\n        function declaration(param = this) {\n            return this;\n        }\n        super();\n    }\n}\nclass FnExpr extends Base {\n    prop = true;\n    constructor() {\n        (function () {\n            return this;\n        })();\n        super();\n    }\n}\nclass Arrow extends Base {\n    prop = true;\n    constructor() {\n        (() => this)();\n        super();\n    }\n}\nclass ClassDecl extends Base {\n    memberClass = class {};\n    constructor() {\n        class Inner extends this.memberClass {\n            method() {\n                return this;\n            }\n        }\n        super();\n    }\n}\nclass ClassExpr extends Base {\n    memberClass = class {};\n    constructor() {\n        console.log(class extends this.memberClass {});\n        super();\n    }\n}\n";

    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains(
            "function declaration(param) {\n            if (param === void 0) { param = this; }\n            return this;\n        }"
        ),
        "Ordinary nested function declarations should keep their own `this`, including default parameters.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(function () {\n            return this;\n        })();"),
        "Ordinary nested function expressions should keep their own `this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(function () { return _this; })();"),
        "Nested arrows should still capture the pre-super constructor receiver.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}(_this.memberClass));"),
        "Nested class declarations should evaluate their heritage expression with the pre-super receiver capture.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "Inner.prototype.method = function () {\n                return this;\n            };"
        ),
        "Nested class method bodies should still keep their own `this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}(_this.memberClass)))"),
        "Nested class expressions should also evaluate their heritage expression with the pre-super receiver capture.\nOutput:\n{output}"
    );
}
