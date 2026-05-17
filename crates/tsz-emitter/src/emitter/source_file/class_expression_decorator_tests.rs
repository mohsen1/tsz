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
