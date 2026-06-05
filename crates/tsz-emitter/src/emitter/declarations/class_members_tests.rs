use crate::emitter::{Printer as EmitPrinter, PrinterOptions};
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_ts(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

fn emit_ts_with_options(source: &str, options: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitPrinter::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn es_decorator_on_method_emitted_at_esnext() {
    let source = "class C {\n    @dec\n    method() {}\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("@dec"),
        "ES decorator on method should be emitted at ESNext target.\nOutput: {output}"
    );
    assert!(
        output.contains("method()"),
        "Decorated method should be emitted.\nOutput: {output}"
    );
}

#[test]
fn es_decorator_on_static_method_emitted() {
    let source = "class C {\n    @dec\n    static foo() {}\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("@dec"),
        "ES decorator on static method should be emitted.\nOutput: {output}"
    );
    assert!(
        output.contains("static foo()"),
        "Static modifier and method name should be emitted.\nOutput: {output}"
    );
}

#[test]
fn namespace_export_does_not_qualify_static_method_name() {
    let source = "namespace A {\n    export class Point {\n        static Origin() { return { x: 0, y: 0 }; }\n    }\n\n    export namespace Point {\n        export function Origin() { return \"\"; }\n    }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("static Origin()"),
        "Class method declarations should keep bare member names inside namespace IIFEs.\nOutput: {output}"
    );
    assert!(
        !output.contains("static A.Origin()"),
        "Namespace export qualification must not apply to class method names.\nOutput: {output}"
    );
}

#[test]
fn esnext_define_class_fields_preserve_type_only_ts_fields() {
    let output = emit_ts_with_options(
        "class A {\n    foo?: string;\n    bar: number;\n    declare baz: boolean;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ESNext,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("class A {\n    foo;\n    bar;\n}"),
        "ESNext define-field emit should preserve TS typed fields without initializers.\nOutput: {output}"
    );
    assert!(
        !output.contains("baz"),
        "`declare` fields should remain erased even when native fields are preserved.\nOutput: {output}"
    );
}

#[test]
fn es_decorator_on_getter_emitted() {
    let source = "class C {\n    @dec\n    get value() { return 1; }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("@dec"),
        "ES decorator on getter should be emitted.\nOutput: {output}"
    );
    assert!(
        output.contains("get value()"),
        "Getter should be emitted.\nOutput: {output}"
    );
}

#[test]
fn multiple_es_decorators_on_method() {
    let source = "class C {\n    @first\n    @second\n    method() {}\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("@first"),
        "First decorator should be emitted.\nOutput: {output}"
    );
    assert!(
        output.contains("@second"),
        "Second decorator should be emitted.\nOutput: {output}"
    );
}

#[test]
fn es_decorator_with_arguments_on_method() {
    let source = "class C {\n    @dec(1, 2)\n    method() {}\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("@dec(1, 2)"),
        "Decorator with arguments should be emitted verbatim.\nOutput: {output}"
    );
}

#[test]
fn single_line_constructor_body_preserved() {
    let source = "class B {\n    constructor(x: number) { this.x = x; }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("constructor(x) { this.x = x; }"),
        "Single-line constructor body should stay on one line.\nOutput: {output}"
    );
}

#[test]
fn quoted_constructor_method_names_emit_as_constructors() {
    let source = "class C {\n    \"constructor\"() {}\n}\nclass D {\n    \"\\x63onstructor\"() {}\n}\nclass E {\n    ['constructor']() {}\n}\nvar o = { \"constructor\"() {} };";
    let output = emit_ts(source);

    assert_eq!(
        output.matches("constructor() { }").count(),
        2,
        "Quoted constructor method names should emit as constructors.\nOutput: {output}"
    );
    assert!(
        output.contains("['constructor']() { }"),
        "Computed constructor property names should remain computed methods.\nOutput: {output}"
    );
    assert!(
        output.contains("var o = { \"constructor\"() { } };"),
        "Object-literal quoted constructor methods should remain quoted methods.\nOutput: {output}"
    );
}

#[test]
fn es2015_async_method_forwards_destructured_and_defaulted_params_to_generator() {
    let source = r#"class X {
    async destructured({ reason, code }) { }
    async nested({ suberr: { reason } }) { }
    async defaulted(value = 1) { }
    async plain(value) { }
}"#;
    let output = emit_ts_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains(
            "destructured(_a) {\n        return __awaiter(this, arguments, void 0, function* ({ reason, code }) { });"
        ),
        "Async methods with binding-pattern parameters should forward outer arguments into the generator.\nOutput: {output}"
    );
    assert!(
        output.contains(
            "nested(_a) {\n        return __awaiter(this, arguments, void 0, function* ({ suberr: { reason } }) { });"
        ),
        "Forwarding should apply structurally to nested binding patterns too.\nOutput: {output}"
    );
    assert!(
        output.contains(
            "defaulted() {\n        return __awaiter(this, arguments, void 0, function* (value = 1) { });"
        ),
        "Async methods with default parameters should move the default into the generator.\nOutput: {output}"
    );
    assert!(
        output.contains(
            "plain(value) {\n        return __awaiter(this, void 0, void 0, function* () { });"
        ),
        "Async methods with simple parameters should keep the outer parameter list and avoid forwarding arguments.\nOutput: {output}"
    );
}

#[test]
fn multiline_constructor_body_stays_multiline() {
    let source = "class B {\n    constructor(x: number) {\n        this.x = x;\n    }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("constructor(x) {\n"),
        "Multi-line constructor body should stay multiline.\nOutput: {output}"
    );
    assert!(
        !output.contains("constructor(x) { this.x = x; }"),
        "Multi-line constructor body should not be collapsed to one line.\nOutput: {output}"
    );
}

#[test]
fn single_line_constructor_body_with_return() {
    let source = "class C {\n    constructor(x: number) { return null; }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("constructor(x) { return null; }"),
        "Single-line constructor body with return should stay on one line.\nOutput: {output}"
    );
}

#[test]
fn bodyless_optional_class_methods_emit_empty_bodies_for_recovery() {
    let output = emit_ts_with_options(
        "class C {\n    x()?: number;\n}\nclass C2<T> {\n    x()?: T;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("class C {\n    x() { }\n}"),
        "Recovered optional class methods should keep an empty runtime body.\nOutput: {output}"
    );
    assert!(
        output.contains("class C2 {\n    x() { }\n}"),
        "Recovered generic optional class methods should erase types and keep an empty runtime body.\nOutput: {output}"
    );
}

#[test]
fn object_literal_accessor_empty_body_has_space_braces() {
    let source = "export const t = {\n    set setter(v) {},\n};";
    let output = emit_ts(source);

    assert!(
        !output.contains("set setter(v) {},"),
        "Object-literal setter should not use compact empty-body formatting.\nOutput: {output}"
    );
    assert!(
        output.contains("set setter(v) { },"),
        "Object-literal setter should preserve trailing comma when present.\nOutput: {output}"
    );
}

#[test]
fn object_literal_accessor_empty_body_compact_in_js_file() {
    let source = "export const t = {\n    set setter(v) {},\n};";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("set setter(v) {}"),
        "JS input object-literal accessor should use compact empty-body formatting.\nOutput: {output}"
    );
    assert!(
        !output.contains("set setter(v) { },"),
        "JS input object-literal accessor should prefer compact braces.\nOutput: {output}"
    );
}

#[test]
fn es5_object_literal_accessor_numeric_name_preserves_source_text() {
    let output = emit_ts_with_options(
        "var f = { 0: 0, get 0o0() { return 0; } };",
        PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("get 0o0()"),
        "Object-literal accessor numeric names are property keys and should keep source token text.\nOutput: {output}"
    );
    assert!(
        !output.contains("get 0()"),
        "Object-literal accessor numeric names should not use numeric expression downleveling.\nOutput: {output}"
    );
}

#[test]
fn generator_method_overloads_preserve_asterisk() {
    // When overloaded generator methods are emitted, the implementation
    // method should retain the * (generator asterisk).
    let source = "class C {\n    *f(s: string): Iterable<any>;\n    *f(s: number): Iterable<any>;\n    *f(s: any): Iterable<any> { }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("*f(s)"),
        "Generator method implementation should retain * after overload erasure.\nOutput: {output}"
    );
}

#[test]
fn static_constructor_preserves_static_modifier() {
    // `static constructor()` is a parse error but tsc preserves `static`
    // in emit for error-recovery fidelity.
    let source = "class C {\n    static constructor() { }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("static constructor()"),
        "Invalid `static` modifier on constructor should be preserved in emit.\nOutput: {output}"
    );
}

#[test]
fn export_constructor_preserves_export_modifier() {
    // `export constructor()` is a parse error but tsc preserves `export`
    // in emit for error-recovery fidelity.
    let source = "class C {\n    export constructor() { }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("export constructor()"),
        "Invalid `export` modifier on constructor should be preserved in emit.\nOutput: {output}"
    );
}

#[test]
fn normal_constructor_emits_without_spurious_modifiers() {
    // A regular constructor without modifiers should emit only `constructor`.
    let source = "class C {\n    constructor(x: number) { this.x = x; }\n}";
    let output = emit_ts(source);
    assert!(
        output.contains("constructor(x)"),
        "Normal constructor should emit without extra modifiers.\nOutput: {output}"
    );
    assert!(
        !output.contains("static constructor"),
        "Normal constructor should not gain a `static` modifier.\nOutput: {output}"
    );
}
