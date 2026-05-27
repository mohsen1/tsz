use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn legacy_decorated_computed_members_reuse_planned_key_temps() {
    let source = "declare function decorator(target: any, key: any): any;\nconst fieldName = \"field\";\nfunction key() { return \"method\"; }\nclass Foo {\n    @decorator [fieldName]: number;\n    @decorator [key()]: number = 1;\n}\nclass Bar {\n    [key()]: number;\n    @decorator [key()]() {}\n}\n";

    let output = emit(source);

    assert!(
        output.contains("var _a, _b, _c;"),
        "Legacy decorated computed names should reserve reusable temps in source order.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this[_b] = 1;"),
        "Decorated computed field initializers should use the planned key temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = fieldName, _b = key();"),
        "Decorated computed fields should evaluate key temps before __decorate calls.\nOutput:\n{output}"
    );
    assert!(
        output.contains("], Foo.prototype, _a, void 0);")
            && output.contains("], Foo.prototype, _b, void 0);"),
        "__decorate calls should reuse field key temps instead of re-emitting expressions.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[(key(), _c = key())]() { }"),
        "A decorated computed method should fold pending erased-field side effects into the class member key.\nOutput:\n{output}"
    );
    assert!(
        output.contains("], Bar.prototype, _c, null);"),
        "Decorated computed methods should pass the method key temp to __decorate.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_decorated_computed_class_expressions_match_tsc_temp_order() {
    let source = "function decorator(target: any, key: any) { }\nconst fieldName = \"field\";\nclass A {\n    [Symbol.match]: any = null;\n    @decorator [Symbol.iterator]: any = null;\n}\nvoid class B {\n    [Symbol.match]: any = null;\n    @decorator [fieldName]: any = null;\n};\nvoid class C {\n    [Symbol.match]: any = null;\n    @decorator [fieldName]: any = null;\n    [\"some\" + \"method\"]() { }\n};\n";

    let output = emit(source);

    assert!(
        output.contains("var _a, _b, _c, _d;\nvar _e, _f, _g;"),
        "Class field and class-expression wrapper temps should be reserved before legacy decorator key temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = Symbol.match, _e = Symbol.iterator;"),
        "A class declaration should evaluate the reserved field temp before the decorated key temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("void (_c = class B {")
            && output.contains("_b = Symbol.match,\n    _f = fieldName,\n    _c);"),
        "A class expression with trailing computed entries should use the reserved wrapper temp after key evaluation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("B.prototype"),
        "Legacy member decorators on class expressions are invalid in tsc and should not emit __decorate calls.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "void class C {\n    constructor() {\n        this[_d] = null;\n        this[_g] = null;"
        ) && output.contains("[(_d = Symbol.match, _g = fieldName, \"some\" + \"method\")]() { }\n};"),
        "A computed method should consume pending computed field entries without adding a class-expression wrapper.\nOutput:\n{output}"
    );
}
