use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_tc39_decorator_source(source: &str) -> String {
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
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_object_property_name() {
    let source = "declare var dec: any;\n({ C: @dec class {} });\n";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("__esDecorate"),
        "Class decorator transform should run for object-property class expressions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"C\")"),
        "Anonymous decorated class expression should use the object property name.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_member_decorated_class_expression_uses_object_property_name() {
    let source = "declare var dec: any;\n({ C: class { @dec y: any; } });\n";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("__esDecorate"),
        "Member decorator transform should run for object-property class expressions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(this, \"C\")"),
        "Anonymous member-decorated class expression should use the object property name.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_literal_computed_object_names() {
    let source = "\
declare var dec: any;
({ [\"C\"]: @dec class {} });
({ [0]: class { @dec y: any; } });
({ __proto__: @dec class {} });
({ [\"__proto__\"]: @dec class {} });
";
    let output = emit_tc39_decorator_source(source);

    assert!(
        !output.contains("__propKey(\"C\")") && !output.contains("__propKey(0)"),
        "Literal computed property names should not allocate __propKey temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"C\")")
            && output.contains("__setFunctionName(this, \"0\")"),
        "Literal computed object properties should pass literal names to __setFunctionName.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"\")")
            && output.contains("__setFunctionName(_classThis, \"__proto__\")"),
        "Noncomputed __proto__ should not perform named evaluation, but computed __proto__ should.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_shorthand_default_name() {
    let source = "declare var dec: any;\nvar C;\n({ C = @dec class {} } = {});\n";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("C = (() => {"),
        "Cover-initialized shorthand assignment should keep the property default expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"C\")"),
        "Anonymous decorated class expression should use the shorthand property name.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expressions_use_file_unique_runtime_names() {
    let source = "\
declare var dec: any;
{ let x = @dec class {}; }
{ let y = @dec class {}; }
{ let z = @dec class {}; }
";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("var class_1 = class")
            && output.contains("var class_2 = class")
            && output.contains("var class_3 = class"),
        "Repeated decorated anonymous class expressions should allocate file-unique runtime names.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return class_1 = _classThis;")
            && output.contains("return class_2 = _classThis;")
            && output.contains("return class_3 = _classThis;"),
        "Each decorated anonymous class expression should return through its own runtime name.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_computed_object_key_temp() {
    let source = "declare var dec: any;\ndeclare var x: any;\n({ [x]: @dec class {} });\n";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("var _a;"),
        "Computed property named evaluation should reserve a temp before the object literal.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[_a = tslib_1.__propKey(x)]"),
        "Computed object property should reuse the planned __propKey temp as the property key.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, _a)"),
        "Computed object property should pass the raw temp to __setFunctionName.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_computed_class_field_temp() {
    let source = "declare var dec: any;\ndeclare var x: any;\nclass C { [x] = @dec class {} }\n";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("var _a;"),
        "Computed class field named evaluation should reserve a temp before the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[_a = tslib_1.__propKey(x)] = (() => {"),
        "Computed class field should reuse the planned __propKey temp as the field key.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, _a)"),
        "Computed class field should pass the raw temp to __setFunctionName.\nOutput:\n{output}"
    );
}

#[test]
fn anonymous_tc39_decorated_class_expression_uses_literal_computed_class_field_names() {
    let source = "\
declare var dec: any;
class C { static [\"x\"] = @dec class {}; }
class D { static [0] = class { @dec y: any; }; }
";
    let output = emit_tc39_decorator_source(source);

    assert!(
        !output.contains("__propKey(\"x\")") && !output.contains("__propKey(0)"),
        "Literal computed class field names should not allocate __propKey temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__setFunctionName(_classThis, \"x\")")
            && output.contains("__setFunctionName(this, \"0\")"),
        "Literal computed class fields should pass literal names to __setFunctionName.\nOutput:\n{output}"
    );
}

#[test]
fn decorated_fields_render_nested_tc39_decorated_class_initializers() {
    let source = "\
declare var dec: any;
class C { @dec x = @dec class {}; }
class D { @dec static [\"y\"] = @dec class {}; }
";
    let output = emit_tc39_decorator_source(source);

    assert!(
        !output.contains("@dec class"),
        "Decorated field initializers should render nested transformed class expressions, not raw source text.\nOutput:\n{output}"
    );
    assert!(
        output.contains("x = tslib_1.__runInitializers(this, _x_initializers, (() => {")
            && output.contains(
                "static [\"y\"] = tslib_1.__runInitializers(this, _static_member_initializers, (() => {"
            ),
        "Decorated fields should pass transformed nested class expressions into __runInitializers.\nOutput:\n{output}"
    );
    assert!(
        output.contains("tslib_1.__setFunctionName(_classThis, \"x\")")
            && output.contains("tslib_1.__setFunctionName(_classThis, \"y\")"),
        "Nested anonymous decorated class expressions should use the containing field name.\nOutput:\n{output}"
    );
}

#[test]
fn decorated_class_static_blocks_rewrite_super_member_calls() {
    let source = "\
declare var dec: any;
declare class Base { static method(...args: any[]): void; }
const method = \"method\";

@dec
class C extends Base {
    static {
        super.method();
        super[method]();
        super.method``;
        super[method]``;
    }
}
";
    let output = emit_tc39_decorator_source(source);

    assert!(
        output.contains("Reflect.get(_classSuper, \"method\", _classThis).call(_classThis);")
            && output.contains("Reflect.get(_classSuper, method, _classThis).call(_classThis);"),
        "Decorated class static blocks should rewrite static super calls through Reflect.get.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Reflect.get(_classSuper, \"method\", _classThis).bind(_classThis) ``;")
            && output.contains("Reflect.get(_classSuper, method, _classThis).bind(_classThis) ``;"),
        "Decorated class static blocks should bind static super tagged-template calls.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("super.method()") && !output.contains("super[method]()"),
        "Decorated class static blocks should not copy raw static super member access.\nOutput:\n{output}"
    );
}
