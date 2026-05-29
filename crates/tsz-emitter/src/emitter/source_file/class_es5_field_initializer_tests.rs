use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
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
fn lifted_es5_class_fields_preserve_object_shape_comments_and_nested_class_name() {
    let source = "\
class Test {
    value = {
        hello: this.hello,
    };

    withGetter = {
        get [this.key]() {
            return true;
        }
    };

    withSetter = {
        set [this.key](_: any) {}
    };

    nested = (class extends this.Base { });

    // lifted field comment
    afterComment = (() => this.value)();
}

";

    let output = emit_es5(source);

    assert!(
        output.contains("this.value = {\n            hello: this.hello,\n        };"),
        "Lifted multiline object literals should preserve source trailing commas.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "Object.defineProperty(_a, this.key, {\n                get: function () {\n                    return true;\n                },\n                enumerable: false,\n                configurable: true\n            })"
        ),
        "Computed getter descriptors lowered from multiline object literals should stay multiline.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "Object.defineProperty(_b, this.key, {\n                set: function (_) { },\n                enumerable: false,\n                configurable: true\n            })"
        ),
        "Computed setter descriptors lowered from multiline object literals should stay multiline.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__extends(class_1, _super);") && output.contains("function class_1() {"),
        "Anonymous nested class expressions should use the class-name namespace, not object temps.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__extends(_"),
        "Anonymous nested class expressions must not consume `_a`-style object temps as class names.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "// lifted field comment\n        this.afterComment = (function () { return _this.value; })();"
        ),
        "Line comments attached to lifted class fields should move with the field initializer.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_class_expression_uses_contextual_object_property_names() {
    let source = "\
var foo: any = {};
foo.alpha = class {};
foo.break = class {};
({ beta: class {}, case: class {} });
";

    let output = emit_es5(source);

    assert!(
        output.contains("function alpha() {\n    }\n    return alpha;"),
        "Property assignment class expressions should use the property name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function break_1() {\n    }\n    return break_1;"),
        "Keyword property assignment class expressions should use a safe derived name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function beta() {") && output.contains("return beta;"),
        "Object literal class expressions should use the property name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function case_1() {") && output.contains("return case_1;"),
        "Keyword object literal class expressions should use a safe derived name.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function class_"),
        "Contextual object property names should avoid generic class temp names.\nOutput:\n{output}"
    );
}
