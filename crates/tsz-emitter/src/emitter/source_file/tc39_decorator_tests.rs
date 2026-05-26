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
fn tc39_decorator_member_access_preserves_receiver_this() {
    let source = "\
declare const instance: any;
declare class Base {
    decorate(value: any, context: any): any;
}

class C {
    @instance.decorate
    method1() {}

    @(instance[\"decorate\"])
    method2() {}

    @((instance.decorate))
    method3() {}
}

class D extends Base {
    m() {
        class Nested {
            @(super.decorate)
            method1() {}

            @(super[\"decorate\"])
            method2() {}

            @((super.decorate))
            method3() {}
        }
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
        output.contains("var _a, _b, _c;")
            && output.contains("_method1_decorators = [(_a = instance).decorate.bind(_a)];")
            && output.contains("_method2_decorators = [((_b = instance)[\"decorate\"].bind(_b))];")
            && output.contains("_method3_decorators = [(((_c = instance).decorate.bind(_c)))];")
            && output.contains("let _outerThis = this;")
            && output.contains("_method1_decorators = [(super.decorate.bind(_outerThis))];")
            && output.contains("_method2_decorators = [(super[\"decorate\"].bind(_outerThis))];")
            && output.contains("_method3_decorators = [((super.decorate.bind(_outerThis)))];"),
        "TC39 decorator member-access expressions should bind evaluated receivers and capture outer this for super.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_class_decorated_member_decorator_captures_outer_this() {
    let source = "\
declare let dec: any;

@dec(this)
class C {
    @dec(this)
    value = 1;
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
        output.contains("let _outerThis = this;")
            && output.contains("let _classDecorators = [dec(this)];")
            && output.contains("_value_decorators = [dec(_outerThis)];"),
        "TC39 member decorator expressions emitted from static blocks should preserve lexical outer this.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_parenthesized_decorated_class_expression_breaks_after_open_paren() {
    let source = "\
declare const dec: any;

(@dec class C {
    @dec
    y: number;
});
";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("(\n@dec\nclass C") && !output.contains("(@dec\nclass C"),
        "Parenthesized native decorated class expressions should put the decorator on a fresh line after the open paren.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_native_parameter_decorators_preserve_syntax_without_types() {
    let source = "\
declare const dec: any;

class C {
    constructor(@dec x: any) {}
    method(@dec x: any) {}
    set value(@dec x: any) {}
}

(class C {
    constructor(@dec x: any) {}
    method(@dec x: any) {}
    static set value(@dec x: any) {}
});
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );

    assert!(
        output.contains("constructor(\n    @dec\n    x) { }")
            && output.contains("method(\n    @dec\n    x) { }")
            && output.contains("set value(\n    @dec\n    x) { }")
            && output.contains("static set value(\n    @dec\n    x) { }"),
        "Native ESNext parameter decorators should be preserved on their own lines.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("x: any"),
        "Native parameter-decorator emit should still erase TypeScript-only parameter types.\nOutput:\n{output}"
    );
}

#[test]
fn esnext_native_decorators_preserve_between_decorator_comments() {
    let source = "\
declare const dec: any;

/*1*/
(
/*2*/
@dec
/*3*/
@dec
/*4*/
class C {
    /*5*/
    @dec
    /*6*/
    @dec
    /*7*/
    method() {}

    /*8*/
    @dec
    /*9*/
    @dec
    /*10*/
    get value() { return 1; }

    /*11*/
    @dec
    /*12*/
    @dec
    /*13*/
    field = 1;
}
);
";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ESNext,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("/*2*/\n@dec\n/*3*/\n@dec\n/*4*/\nclass C"),
        "Class decorator comments should stay between native decorators and the class keyword.\nOutput:\n{output}"
    );
    assert!(
        output.contains("/*5*/\n    @dec\n    /*6*/\n    @dec\n    /*7*/\n    method() { }")
            && output.contains(
                "/*8*/\n    @dec\n    /*9*/\n    @dec\n    /*10*/\n    get value() { return 1; }"
            )
            && output
                .contains("/*11*/\n    @dec\n    /*12*/\n    @dec\n    /*13*/\n    field = 1;"),
        "Member decorator comments should stay between native decorators and member tokens.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_transformed_decorated_members_preserve_outer_comments() {
    let source = "\
declare const dec: any;

@dec
class C {
    /*method0*/
    @dec
    /*method1*/
    method() {}

    /*get0*/
    @dec
    /*get1*/
    get value() { return 1; }

    /*field0*/
    @dec
    /*field1*/
    field = 1;

    /*accessor0*/
    @dec
    /*accessor1*/
    accessor z = 1;

    /*static0*/
    @dec
    /*static1*/
    static #s = 1;
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
        output.contains("/*method0*/\n        method() { }")
            && output.contains("/*get0*/\n        get value() { return 1; }")
            && output.contains("/*field0*/\n        field = (__runInitializers")
            && output.contains("/*accessor0*/\n        get z()")
            && output.contains("static {\n            /*static0*/\n            _C_s ="),
        "Transformed TC39 member output should keep the outer source-leading member comments.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("/*method1*/")
            && !output.contains("/*get1*/")
            && !output.contains("/*field1*/")
            && !output.contains("/*accessor1*/")
            && !output.contains("/*static1*/"),
        "Transformed TC39 member output should not promote comments between decorator lines.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_moved_decorated_fields_preserve_outer_comments() {
    let source = "\
declare const dec: any;

@dec
class C {
    /*field0*/
    @dec
    /*field1*/
    field = 1;

    /*accessor0*/
    @dec
    /*accessor1*/
    accessor z = 1;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "constructor() {\n            /*field0*/\n            this.field = __runInitializers"
        ) && output.contains("/*accessor0*/\n        get z()"),
        "Lower-target TC39 member replacement sites should keep source-leading comments for moved fields and accessors.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("/*field1*/") && !output.contains("/*accessor1*/"),
        "Lower-target TC39 member replacement sites should not use comments between decorator lines.\nOutput:\n{output}"
    );
}

#[test]
fn transformed_parenthesized_decorated_class_expression_uses_iife_wrapper_layout() {
    let source = "\
declare const dec: any;

/*1*/
(
/*2*/
@dec
class C {
    @dec
    static #m() {}
}
);
";

    let es2015_output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        es2015_output.contains("/*1*/\n((() => {") && !es2015_output.contains("/*2*/"),
        "Lower-target transformed decorated class expressions should let the IIFE own the class expression while keeping the outer parens.\nOutput:\n{es2015_output}"
    );

    let es2022_output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );
    assert!(
        es2022_output.contains("/*1*/\n((() => {")
            && es2022_output.contains("static { __setFunctionName(this, \"C\"); }")
            && !es2022_output.contains("/*2*/"),
        "ES2022 transformed parenthesized class expressions should preserve wrapper layout and name the generated class.\nOutput:\n{es2022_output}"
    );
}

#[test]
fn class_decorated_static_private_members_request_access_helpers_in_tsc_order() {
    let source = "\
declare const dec: any;

@dec
class C {
    @dec
    static #m() {}

    @dec
    static get #x() { return 1; }

    @dec
    static set #x(value) {}

    @dec
    static #y = 1;
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

    let run_initializers = output
        .find("var __runInitializers")
        .expect("expected __runInitializers helper");
    let es_decorate = output
        .find("var __esDecorate")
        .expect("expected __esDecorate helper");
    let private_in = output
        .find("var __classPrivateFieldIn")
        .expect("expected __classPrivateFieldIn helper");
    let private_get = output
        .find("var __classPrivateFieldGet")
        .expect("expected __classPrivateFieldGet helper");
    let private_set = output
        .find("var __classPrivateFieldSet")
        .expect("expected __classPrivateFieldSet helper");

    assert!(
        run_initializers < es_decorate && private_in < private_get && private_get < private_set,
        "Class-decorated static private member access helpers should follow tsc helper order.\nOutput:\n{output}"
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
fn tc39_es5_class_decorated_static_this_members_use_class_capture() {
    let source = "\
declare var dec: any;

@dec
class C {
    static { this; }
    static x: any = this;
    static m() { this; }
    static get g() { return this; }
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    let decorate_pos = output
        .find("__esDecorate(null, _classDescriptor = { value: _classThis }")
        .expect("expected class decorator application");
    let static_block_pos = output
        .find("_classThis;\n    })();")
        .expect("expected static block to run after decoration");
    let static_field_pos = output
        .find("_classThis.x = _classThis;")
        .expect("expected static field initializer to use class capture");
    let class_init_pos = output
        .find("__runInitializers(_classThis, _classExtraInitializers);")
        .expect("expected class extra initializers");

    assert!(
        output.contains("var C = function () {")
            && output.contains("var C = _classThis = /** @class */ (function () {")
            && output.contains("function C_1()")
            && output.contains("C_1.m = function () { this; };")
            && output.contains("Object.defineProperty(C_1, \"g\""),
        "ES5 class-decorated classes should wrap the inner class while leaving methods/accessors on the inner constructor.\nOutput:\n{output}"
    );
    assert!(
        decorate_pos < static_block_pos
            && static_block_pos < static_field_pos
            && static_field_pos < class_init_pos,
        "Static initializers should run against the decorated class capture before class extra initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("C.x = this;") && !output.contains("_a = C;"),
        "Static `this` should not use the undecorated constructor path.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es5_static_public_field_decorator_reserves_outer_this_capture() {
    let source = "\
declare var dec: any;

class C {
    @dec
    static field = 1;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    let this_capture_pos = output
        .find("var _this = this;")
        .expect("expected outer this capture");
    let class_pos = output
        .find("var C = function ()")
        .expect("expected ES5 class");
    assert!(
        this_capture_pos < class_pos,
        "Static decorated public fields should reserve the ES5 outer this capture before the class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.field = __runInitializers(_a, _static_field_initializers, 1),"),
        "Static decorated public field initialization should run after decoration in the wrapper.\nOutput:\n{output}"
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

#[test]
fn tc39_class_and_member_decorated_static_private_members_use_helper_temps() {
    let source = "\
declare var dec: any;

@dec
class C {
    @dec
    static #method() {}
    @dec
    static get #value() { return 0; }
    @dec
    static set #value(value) {}
    @dec
    static #field = 1;
    @dec
    static accessor #accessor = 2;
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
        output.contains("var _C_method_get, _C_value_get, _C_value_set, _C_field, _C_accessor_accessor_storage, _C_accessor_get, _C_accessor_set;")
            && output.contains("_C_method_get = function _C_method_get() { return _static_private_method_descriptor.value; }")
            && output.contains("_C_value_get = function _C_value_get() { return _static_private_get_value_descriptor.get.call(this); }")
            && output.contains("_C_value_set = function _C_value_set(value) { return _static_private_set_value_descriptor.set.call(this, value); }")
            && output.contains("access: { has: obj => __classPrivateFieldIn(_classThis, obj), get: obj => __classPrivateFieldGet(obj, _classThis, \"a\", _C_method_get) }")
            && output.contains("access: { has: obj => __classPrivateFieldIn(_classThis, obj), get: obj => __classPrivateFieldGet(obj, _classThis, \"f\", _C_field), set: (obj, value) => { __classPrivateFieldSet(obj, _classThis, value, \"f\", _C_field); } }")
            && output.contains("access: { has: obj => __classPrivateFieldIn(_classThis, obj), get: obj => __classPrivateFieldGet(obj, _classThis, \"a\", _C_accessor_get), set: (obj, value) => { __classPrivateFieldSet(obj, _classThis, value, \"a\", _C_accessor_set); } }"),
        "Class and member decorated static private elements should expose class-replacement-safe helper temps.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("obj.#method")
            && !output.contains("obj.#value")
            && !output.contains("obj.#field")
            && !output.contains("obj.#accessor"),
        "Class and member decorated static private access records must not keep native private syntax.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_static_private_fields_use_storage_temps() {
    let source = "\
declare var dec: any;

class C {
    @dec
    static #value = 0;
}

@dec
class D {
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
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a, _C_value;")
            && output.contains("has: obj => __classPrivateFieldIn(_a, obj), get: obj => __classPrivateFieldGet(obj, _a, \"f\", _C_value), set: (obj, value) => { __classPrivateFieldSet(obj, _a, value, \"f\", _C_value); }")
            && output.contains("_C_value = { value: __runInitializers(_a, _static_private_value_initializers, 0) }")
            && output.contains("var _D_value;")
            && output.contains("_D_value = { value: 0 };")
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"f\", _D_value);")
            && output.contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"f\", _D_value);"),
        "ES2015 decorated static private fields should use generated storage descriptors for decorator access and class-decorator static-block capture.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(".#value"),
        "ES2015 decorated static private field output must not keep native private-field access.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_static_private_methods_and_accessors_use_descriptor_temps() {
    let source = "\
declare var dec: any;

class C {
    @dec
    static #method() {}
    @dec
    static get #value() { return 0; }
    @dec
    static set #value(value) {}
}

@dec
class D {
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
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a, _C_method_get, _C_value_get, _C_value_set;")
            && output.contains("_C_method_get = function _C_method_get() { return _static_private_method_descriptor.value; }")
            && output.contains("get: obj => __classPrivateFieldGet(obj, _a, \"a\", _C_method_get)")
            && output.contains("_C_value_get = function _C_value_get() { return _static_private_get_value_descriptor.get.call(this); }")
            && output.contains("_C_value_set = function _C_value_set(value) { return _static_private_set_value_descriptor.set.call(this, value); }")
            && output.contains("_D_value_get = function _D_value_get() { return 0; };")
            && output.contains("_D_value_set = function _D_value_set(value) { };")
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"a\", _D_value_get);")
            && output.contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"a\", _D_value_set);"),
        "ES2015 decorated static private methods/accessors should use descriptor temps for decorator access and class-decorator static-block capture.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(".#method") && !output.contains(".#value"),
        "ES2015 decorated static private method/accessor output must not keep native private access.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_static_private_auto_accessors_use_descriptor_temps() {
    let source = "\
declare var dec: any;

class C {
    @dec
    static accessor #value = 0;
}

@dec
class D {
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
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a, _C_value_accessor_storage, _C_value_get, _C_value_set;")
            && output.contains("__classPrivateFieldGet(_a, _a, \"f\", _C_value_accessor_storage)")
            && output.contains("__classPrivateFieldSet(_a, _a, value, \"f\", _C_value_accessor_storage)")
            && output.contains("_C_value_get = function _C_value_get() { return _static_private_value_descriptor.get.call(this); }")
            && output.contains("_C_value_set = function _C_value_set(value) { return _static_private_value_descriptor.set.call(this, value); }")
            && output.contains("get: obj => __classPrivateFieldGet(obj, _a, \"a\", _C_value_get)")
            && output.contains("set: (obj, value) => { __classPrivateFieldSet(obj, _a, value, \"a\", _C_value_set); }")
            && output.contains("_C_value_accessor_storage = { value: __runInitializers(_a, _static_private_value_initializers, 0) }")
            && output.contains("_D_value_get = function _D_value_get() { return __classPrivateFieldGet(_classThis, _classThis, \"f\", _D_value_accessor_storage); }")
            && output.contains("_D_value_set = function _D_value_set(value) { __classPrivateFieldSet(_classThis, _classThis, value, \"f\", _D_value_accessor_storage); };")
            && output.contains("__classPrivateFieldGet(_classThis, _classThis, \"a\", _D_value_get);")
            && output.contains("__classPrivateFieldSet(_classThis, _classThis, 1, \"a\", _D_value_set);"),
        "ES2015 decorated static private auto-accessors should use descriptor temps and generated storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(".#value"),
        "ES2015 decorated static private auto-accessor output must not keep native private access.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_nonstatic_private_members_use_storage_and_descriptor_temps() {
    let source = "\
declare var dec: any;

class C {
    @dec
    #field = 0;
    @dec
    #method() {}
    @dec
    get #value() { return 1; }
    @dec
    set #value(value) {}
    @dec
    accessor #acc = 2;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("_C_instances")
            && output.contains("_C_field = new WeakMap()")
            && output.contains("_C_acc_accessor_storage = new WeakMap()")
            && output.contains("_C_instances = new WeakSet()")
            && output.contains("_C_instances.add(this);")
            && output.contains("_C_field.set(this, __runInitializers(this, _private_field_initializers, 0));")
            && output.contains("_C_acc_accessor_storage.set(this, __runInitializers(this, _private_acc_initializers, 2));")
            && output.contains("has: obj => __classPrivateFieldIn(_C_field, obj), get: obj => __classPrivateFieldGet(obj, _C_field, \"f\"), set: (obj, value) => { __classPrivateFieldSet(obj, _C_field, value, \"f\"); }")
            && output.contains("_C_method_get = function _C_method_get() { return _private_method_descriptor.value; }")
            && output.contains("get: obj => __classPrivateFieldGet(obj, _C_instances, \"a\", _C_method_get)")
            && output.contains("_C_value_get = function _C_value_get() { return _private_get_value_descriptor.get.call(this); }")
            && output.contains("_C_value_set = function _C_value_set(value) { return _private_set_value_descriptor.set.call(this, value); }")
            && output.contains("_C_acc_get = function _C_acc_get() { return _private_acc_descriptor.get.call(this); }")
            && output.contains("_C_acc_set = function _C_acc_set(value) { return _private_acc_descriptor.set.call(this, value); }")
            && output.contains("__classPrivateFieldGet(this, _C_acc_accessor_storage, \"f\")")
            && output.contains("__classPrivateFieldSet(this, _C_acc_accessor_storage, value, \"f\")"),
        "ES2015 decorated non-static private members should lower through storage/brand descriptor temps.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(".#field")
            && !output.contains(".#method")
            && !output.contains(".#value")
            && !output.contains(".#acc")
            && !output.contains("get #")
            && !output.contains("set #"),
        "ES2015 decorated non-static private member output must not keep native private syntax.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_nonstatic_auto_accessors_initialize_storage_before_computed_names() {
    let source = "\
declare var dec: any;
declare var field3: any;

class C {
    @dec
    accessor field1 = 1;
    @dec
    accessor [\"field2\"] = 2;
    @dec
    accessor [field3] = 3;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "get [(_C_field1_accessor_storage = new WeakMap(), _C__a_accessor_storage = new WeakMap(), _C__b_accessor_storage = new WeakMap(), \"field2\")]()"
        )
            && output.contains("_C_field1_accessor_storage.set(this, __runInitializers(this, _field1_initializers, 1));")
            && output.contains("_C__a_accessor_storage.set(this, (__runInitializers(this, _field1_extraInitializers), __runInitializers(this, _member_initializers, 2)));")
            && output.contains("_C__b_accessor_storage.set(this, (__runInitializers(this, _member_extraInitializers), __runInitializers(this, _member_initializers_1, 3)));")
            && output.contains("_member_decorators = [dec]")
            && output.contains("_member_decorators_1 = [dec]")
            && output.contains("_b = __propKey(field3)"),
        "ES2015 public instance auto-accessors should initialize generated storage before computed names and chain extra initializers.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_class_decorated_auto_accessors_chain_field_extras() {
    let source = "\
declare var dec: any;

@dec
class C {
    @dec
    y = 1;
    @dec
    accessor z = 1;
    @dec
    static #y = 1;
    @dec
    static accessor #z = 1;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("_C_z_1_accessor_storage = new WeakMap();")
            && output.contains("_C_z_1_accessor_storage.set(this, (__runInitializers(this, _y_extraInitializers), __runInitializers(this, _z_initializers, 1)));")
            && output.contains("_C_z_accessor_storage = { value: (__runInitializers(_classThis, _static_private_y_extraInitializers), __runInitializers(_classThis, _static_private_z_initializers, 1)) };"),
        "ES2015 class-decorated auto-accessors should create generated storage and chain previous field extra initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_C_y = { value: (__runInitializers(_classThis, _staticExtraInitializers), __runInitializers(_classThis, _static_private_y_initializers, 1)) };\n    (() => {\n        __runInitializers(_classThis, _static_private_y_extraInitializers);"),
        "Static private field extra initializers should be consumed by the following static private auto-accessor.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2022_class_decorated_auto_accessors_chain_field_extras() {
    let source = "\
declare var dec: any;

@dec
class C {
    @dec
    y = 1;
    @dec
    accessor z = 1;
    @dec
    static #y = 1;
    @dec
    static accessor #z = 1;
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
        output.contains("#z_1_accessor_storage = (__runInitializers(this, _y_extraInitializers), __runInitializers(this, _z_initializers, 1));")
            && output.contains("_C_z_accessor_storage = { value: (__runInitializers(_classThis, _static_private_y_extraInitializers), __runInitializers(_classThis, _static_private_z_initializers, 1)) };"),
        "ES2022 class-decorated auto-accessors should chain previous field extra initializers in class body storage.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("constructor() {\n            __runInitializers(this, _y_extraInitializers);\n            __runInitializers(this, _z_extraInitializers);"),
        "Instance field extra initializers should not be emitted separately when the following auto-accessor consumes them.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es5_public_methods_schedule_computed_key_decorators() {
    let source = "\
declare var dec: any;
declare var method3: any;

class C {
    @dec(1) method1() {}
    @dec(2) [\"method2\"]() {}
    @dec(3) [method3]() {}
}

class D {
    @dec(1) static method1() {}
    @dec(2) static [\"method2\"]() {}
    @dec(3) static [method3]() {}
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("__runInitializers(this, _instanceExtraInitializers);")
            && output.contains(
            "C.prototype[(_method1_decorators = [dec(1)], _member_decorators = [dec(2)], _member_decorators_1 = [dec(3)], _b = __propKey(method3))] = function () { };"
        )
            && output.contains(
                "D[(_static_method1_decorators = [dec(1)], _static_member_decorators = [dec(2)], _static_member_decorators_1 = [dec(3)], _b = __propKey(method3))] = function () { };"
            )
            && output.contains(
                "__esDecorate(_a, null, _member_decorators, { kind: \"method\", name: \"method2\", static: false, private: false, access: { has: function (obj) { return \"method2\" in obj; }, get: function (obj) { return obj[\"method2\"]; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _member_decorators_1, { kind: \"method\", name: _b, static: false, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_member_decorators, { kind: \"method\", name: \"method2\", static: true, private: false, access: { has: function (obj) { return \"method2\" in obj; }, get: function (obj) { return obj[\"method2\"]; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_member_decorators_1, { kind: \"method\", name: _b, static: true, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            ),
        "ES5 TC39 public methods should sink decorator/proKey assignments into the computed method key and use bracket access for string/computed names.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es5_abstract_decorated_accessors_have_no_runtime_decorator_output() {
    let source = "\
declare var dec: any;
declare var method3: any;

abstract class C {
    @dec(1) abstract get method1(): number;
    @dec(2) abstract set [\"method2\"](value);
    @dec(3) abstract get [method3](): number;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("__esDecorate(_a")
            && !output.contains("_method1_decorators")
            && !output.contains("_member_decorators"),
        "Abstract decorated accessors should not enter the ES5 TC39 runtime decorator wrapper.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es5_public_accessors_schedule_computed_key_decorators() {
    let source = "\
declare var dec: any;
declare var method3: any;

class C {
    @dec(11) get method1() { return 0; }
    @dec(12) set method1(value) {}
    @dec(21) get [\"method2\"]() { return 0; }
    @dec(22) set [\"method2\"](value) {}
    @dec(31) get [method3]() { return 0; }
    @dec(32) set [method3](value) {}
}

class D {
    @dec(11) static get method1() { return 0; }
    @dec(12) static set method1(value) {}
    @dec(21) static get [\"method2\"]() { return 0; }
    @dec(22) static set [\"method2\"](value) {}
    @dec(31) static get [method3]() { return 0; }
    @dec(32) static set [method3](value) {}
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "Object.defineProperty(C.prototype, (_get_method1_decorators = [dec(11)], _set_method1_decorators = [dec(12)], _get_member_decorators = [dec(21)], _set_member_decorators = [dec(22)], _get_member_decorators_1 = [dec(31)], _b = __propKey(method3)), {"
        )
            && output.contains(
                "Object.defineProperty(C.prototype, (_set_member_decorators_1 = [dec(32)], _c = __propKey(method3)), {"
            )
            && output.contains(
                "Object.defineProperty(D, (_static_get_method1_decorators = [dec(11)], _static_set_method1_decorators = [dec(12)], _static_get_member_decorators = [dec(21)], _static_set_member_decorators = [dec(22)], _static_get_member_decorators_1 = [dec(31)], _b = __propKey(method3)), {"
            )
            && output.contains(
                "Object.defineProperty(D, (_static_set_member_decorators_1 = [dec(32)], _c = __propKey(method3)), {"
            )
            && output.contains(
                "__esDecorate(_a, null, _get_member_decorators_1, { kind: \"getter\", name: _b, static: false, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _set_member_decorators_1, { kind: \"setter\", name: _c, static: false, private: false, access: { has: function (obj) { return _c in obj; }, set: function (obj, value) { obj[_c] = value; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_get_member_decorators_1, { kind: \"getter\", name: _b, static: true, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_set_member_decorators_1, { kind: \"setter\", name: _c, static: true, private: false, access: { has: function (obj) { return _c in obj; }, set: function (obj, value) { obj[_c] = value; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            ),
        "ES5 TC39 public accessors should sink decorator/proKey assignments into computed Object.defineProperty keys and use bracket access for computed names.\nOutput:\n{output}"
    );
}
