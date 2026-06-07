use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget) -> String {
    emit_with_define(source, target, false)
}

fn emit_with_define(
    source: &str,
    target: ScriptTarget,
    use_define_for_class_fields: bool,
) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields,
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
fn es2015_nested_class_computed_names_use_enclosing_static_this_alias() {
    let source = "class C {\n    static c = \"foo\";\n    static bar = class Inner {\n        static [this.c] = 123;\n        [this.c] = 456;\n    };\n}\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("class Inner {\n        constructor() {\n            this[_d] = 456;"),
        "Instance computed field should use the captured computed-name temp in the constructor.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_c = _a.c,\n    _d = _a.c,\n") && output.contains("_b[_c] = 123,"),
        "Computed names should evaluate against the enclosing static alias before static assignment.\nOutput:\n{output}"
    );
}

#[test]
fn es5_nested_class_computed_names_use_enclosing_static_this_alias() {
    let source = "class C {\n    static c = \"foo\";\n    static bar = class Inner {\n        static [this.c] = 123;\n        [this.c] = 456;\n    };\n}\n";
    let output = emit(source, ScriptTarget::ES5);

    assert!(
        output.contains("var _a, _b, _c, _d;"),
        "ES5 static-initializer class-expression temps should share the IIFE hoist group in tsc order.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "C.bar = (_b = /** @class */ (function () {\n            function Inner() {\n                this[_d] = 456;\n            }\n            return Inner;\n        }()),\n        _c = _a.c,\n        _d = _a.c,\n        _b[_c] = 123,\n        _b);"
        ),
        "ES5 computed names should evaluate against the enclosing static alias before static assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Inner[_b] = 123")
            && !output.contains("_b = this.c")
            && !output.contains("_c = this.c"),
        "ES5 nested class computed names must not fall back to unbound `this` or run the static assignment before key evaluation.\nOutput:\n{output}"
    );
}

#[test]
fn es5_define_nested_class_computed_names_use_enclosing_static_this_alias() {
    let source = "class C {\n    static c = \"foo\";\n    static bar = class Inner {\n        static [this.c] = 123;\n        [this.c] = 456;\n    };\n}\n";
    let output = emit_with_define(source, ScriptTarget::ES5, true);

    assert!(
        output.contains("var _a, _b, _c, _d;"),
        "ES5 define-mode static-initializer class-expression temps should share the IIFE hoist group in tsc order.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "value: (_b = /** @class */ (function () {\n                function Inner() {\n                    Object.defineProperty(this, _d, {\n                        enumerable: true,\n                        configurable: true,\n                        writable: true,\n                        value: 456\n                    });\n                }\n                return Inner;\n            }()),\n            _c = _a.c,\n            _d = _a.c,\n            Object.defineProperty(_b, _c, {"
        ),
        "ES5 define-mode static computed field should use the enclosing static alias for key evaluation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Object.defineProperty(Inner, _b")
            && !output.contains("_b = this.c")
            && !output.contains("_c = this.c"),
        "ES5 define-mode computed names must not use unbound `this`.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_static_initializer_class_expr_result_temp_precedes_computed_key_temps() {
    let source = "class C {\n    static c = \"foo\";\n    static bar = class Inner {\n        static [this.c] = 123;\n        [this.c] = 456;\n    };\n}\n";
    let output = emit(source, ScriptTarget::ES2015);

    assert!(
        output.contains("var _a, _b, _c, _d;"),
        "Static-initializer class-expression temps should share the hoist group in tsc order.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "C.bar = (_b = class Inner {\n        constructor() {\n            this[_d] = 456;\n        }\n    },\n    _c = _a.c,\n    _d = _a.c,\n    _b[_c] = 123,\n    _b);"
        ),
        "Class-expression result temp should be reserved before computed key temps while \
         preserving enclosing static `this` alias evaluation.\nOutput:\n{output}"
    );
}

#[test]
fn es2022_static_block_field_initializer_captures_nested_class_computed_names() {
    let source = "class C {\n    static c = \"foo\";\n    static bar = class Inner {\n        static [this.c] = 123;\n        [this.c] = 456;\n    };\n}\n";
    let output = emit(source, ScriptTarget::ES2022);

    assert!(
        output.contains("static { this.bar = (_c = () => { _a = this.c, _b = this.c; },"),
        "Native static field block should capture nested class computed names against the enclosing `this`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("constructor() {")
            && output.contains("this[_b] = 456;")
            && output.contains("static { _c(); }")
            && output.contains("static { this[_a] = 123; }"),
        "Nested class fields should consume the captured keys in instance and static initializers.\nOutput:\n{output}"
    );
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

#[test]
fn es2015_define_static_super_writes_use_hoisted_comma_temps() {
    let source = "\
declare class Base { static a: any; }
class C extends Base {
    static assign = super.a = 0;
    static add = super.a += 1;
    static rest = { ...super.a } = { x: 0 };
    static pre = ++super.a;
    static elem = ++super[(\"a\")];
    static post = super.a++;

    // keep instance comment
    x = 1;
}
";
    let output = emit_with_define(source, ScriptTarget::ES2015, true);

    assert!(
        output.contains("var _a;\nvar _b, _c, _d, _e, _f, _g, _h, _j, _k, _l, _m;"),
        "Object-rest assignment temp should be declared before class/static-super temps in tsc order.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class C extends (_c = Base)") && output.contains("_b = C;"),
        "Static field initializers should use separate class and base aliases.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: (Reflect.set(_c, \"a\", _d = 0, _b), _d)")
            && output.contains("value: (Reflect.set(_c, \"a\", _e = Reflect.get(_c, \"a\", _b) + 1, _b), _e)")
            && output.contains("value: (Reflect.set(_c, \"a\", (_g = Reflect.get(_c, \"a\", _b), _f = ++_g), _b), _f)")
            && output.contains("value: (Reflect.set(_c, _h = (\"a\"), (_k = Reflect.get(_c, _h, _b), _j = ++_k), _b), _j)")
            && output.contains("value: (Reflect.set(_c, \"a\", (_m = Reflect.get(_c, \"a\", _b), _l = _m++, _m), _b), _l)"),
        "Value-producing static `super` writes should use hoisted comma expressions, not IIFEs.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Object.defineProperty(C, \"rest\", Object.assign({ enumerable: true, configurable: true, writable: true, value: (_a = { x: 0 },"),
        "Define-mode object-rest static initializer should use tsc's compact `Object.assign` descriptor.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "        // keep instance comment\n        Object.defineProperty(this, \"x\""
        ),
        "Leading comments before lowered instance fields should remain in the synthesized constructor.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_assign_static_super_writes_use_hoisted_comma_temps() {
    let source = "\
declare class Base { static a: any; }
class C extends Base {
    static assign = super.a = 0;
    static add = super.a += 1;
    static pre = ++super.a;

    // keep instance comment
    x = 1;
}
";
    let output = emit_with_define(source, ScriptTarget::ES2015, false);

    assert!(
        output.contains("class C extends (_b = Base)") && output.contains("_a = C;"),
        "Assignment-mode static field initializers should keep class/base aliases stable.\nOutput:\n{output}"
    );
    assert!(
        output.contains("C.assign = (Reflect.set(_b, \"a\", _c = 0, _a), _c);")
            && output.contains("C.add = (Reflect.set(_b, \"a\", _d = Reflect.get(_b, \"a\", _a) + 1, _a), _d);")
            && output.contains("C.pre = (Reflect.set(_b, \"a\", (_f = Reflect.get(_b, \"a\", _a), _e = ++_f), _a), _e);"),
        "Assignment-mode static `super` writes should also use hoisted comma expressions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("        // keep instance comment\n        this.x = 1;"),
        "Leading comments before lowered assignment-mode instance fields should be preserved.\nOutput:\n{output}"
    );
}
