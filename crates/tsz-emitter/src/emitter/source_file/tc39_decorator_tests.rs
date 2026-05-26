use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn emit_with_options(source: &str, options: PrinterOptions) -> String {
    let (parser, root) = parse_test_source(source);
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
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

    let output = emit_with_options(
        source,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2022,
            import_helpers: true,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const label = String(value);"),
        "Default decorated private method body should be rendered through the JS emitter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("value: number") && !output.contains("label: string"),
        "Default decorated private method body must not copy TypeScript-only syntax.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_decorated_public_members_strip_return_type_annotations() {
    let source = "\
declare var dec: any;
class C {
    @dec
    m(): void {}
    @dec
    objectResult(): { x: number } { return { x: 1 }; }
    @dec
    get value(): number { return 1; }
    @dec
    get objectValue(): { x: number } { return { x: 1 }; }
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("m() { }")
            && output.contains("objectResult() { return { x: 1 }; }")
            && output.contains("get value() { return 1; }")
            && output.contains("get objectValue() { return { x: 1 }; }"),
        "Decorated public method/accessor emit should keep JS member syntax.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("m(): void")
            && !output.contains("objectResult(): { x: number }")
            && !output.contains("value(): number")
            && !output.contains("objectValue(): { x: number }"),
        "Decorated public method/accessor emit must not copy return type annotations.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_class_decorated_static_this_members_use_class_capture() {
    let source = "\
declare var dec: any;

@dec
class C {
    static { this; }
    static x: any = this;
    static accessor a: any = this;
    static m() { this; }
}
";

    let es2022_output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        es2022_output.contains("static { _classThis; }")
            && es2022_output.contains("static x = _classThis;")
            && es2022_output.contains("_C_a_accessor_storage = { value: _classThis };")
            && es2022_output.contains("static get a() { return __classPrivateFieldGet(_classThis, _classThis, \"f\", _C_a_accessor_storage); }")
            && es2022_output.contains("static set a(value) { __classPrivateFieldSet(_classThis, _classThis, value, \"f\", _C_a_accessor_storage); }")
            && es2022_output.contains("static m() { this; }"),
        "Class-decorated static blocks, fields, and auto-accessors should use the class capture without changing method `this`.\nOutput:\n{es2022_output}"
    );
    assert!(
        !es2022_output.contains("static x: any = this")
            && !es2022_output.contains("static accessor a: any = this"),
        "Class-decorated static field emit must not copy TypeScript-only syntax.\nOutput:\n{es2022_output}"
    );

    let es2015_output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        es2015_output.contains("_classThis;\n    })();")
            && es2015_output.contains("_classThis.x = _classThis;")
            && es2015_output.contains("_C_a_accessor_storage = { value: _classThis };")
            && es2015_output.contains("static get a() { return __classPrivateFieldGet(_classThis, _classThis, \"f\", _C_a_accessor_storage); }")
            && es2015_output.contains("static m() { this; }"),
        "Lowered class-decorated static blocks, fields, and auto-accessors should use the class capture.\nOutput:\n{es2015_output}"
    );
    assert!(
        !es2015_output.contains("static { this; }")
            && !es2015_output.contains("x: any = this")
            && !es2015_output.contains("accessor a: any = this"),
        "Lowered class-decorated static members must not keep raw TypeScript syntax.\nOutput:\n{es2015_output}"
    );
}

#[test]
fn tc39_class_decorated_static_private_accessors_use_helper_temps() {
    let source = "\
declare var dec: any;

@dec
class C {
    static get #value() { return 0; }
    static set #value(value) {}
    static {
        this.#value;
        this.#value = 1;
    }
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _C_value_get, _C_value_set;")
            && output.contains(
                "static { _C_value_get = function _C_value_get() { return 0; }, _C_value_set = function _C_value_set(value) { }; }"
            )
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"a\", _C_value_get);")
            && output.contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"a\", _C_value_set);"),
        "Class-decorated static private accessors should be extracted into helper temps used by static blocks.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_classThis.#value"),
        "Class-decorated static blocks should not keep direct static private accessor syntax after capture.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_class_decorated_static_private_auto_accessors_use_helper_temps() {
    let source = "\
declare var dec: any;

@dec
class C {
    static accessor #value = 0;
    static {
        this.#value;
        this.#value = 1;
    }
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _C_value_get, _C_value_set, _C_value_accessor_storage;")
            && output.contains(
                "static { _C_value_get = function _C_value_get() { return __classPrivateFieldGet(_classThis, _classThis, \"f\", _C_value_accessor_storage); }, _C_value_set = function _C_value_set(value) { __classPrivateFieldSet(_classThis, _classThis, value, \"f\", _C_value_accessor_storage); }; }"
            )
            && output.contains("_C_value_accessor_storage = { value: 0 };")
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"a\", _C_value_get);")
            && output.contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"a\", _C_value_set);"),
        "Class-decorated static private auto-accessors should use helper temps over generated storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_classThis.#value"),
        "Class-decorated static blocks should not keep direct private auto-accessor syntax after capture.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_class_decorated_static_private_fields_use_storage_temps() {
    let source = "\
declare var dec: any;

@dec
class C {
    static #value = 0;
    static {
        this.#value;
        this.#value = 1;
    }
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _C_value;")
            && output.contains("static {\n            _C_value = { value: 0 };\n        }")
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"f\", _C_value);")
            && output
                .contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"f\", _C_value);"),
        "Class-decorated static private fields should use storage temps in captured static blocks.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_classThis.#value"),
        "Class-decorated static blocks should not keep direct static private field syntax after capture.\nOutput:\n{output}"
    );
}
