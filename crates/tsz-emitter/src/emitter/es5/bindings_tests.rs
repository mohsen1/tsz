use crate::output::printer::{PrintOptions, Printer};
use tsz_parser::ParserState;

#[test]
fn emit_using_declaration_es5() {
    let source = "using d = { [Symbol.dispose]() {} };\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var env_1"),
        "Expected disposable env temp allocation.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__addDisposableResource"),
        "Expected __addDisposableResource helper call for using declarations.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__disposeResources"),
        "Expected __disposeResources helper call for using declarations.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("using d"),
        "Raw using syntax should be downleveled on ES5.\nOutput:\n{output}"
    );
}

#[test]
fn destructuring_new_expr_gets_parens_for_property_access() {
    // var { x } = <any>new Foo; -> var x = (new Foo).x;
    let source = "var { x } = <any>new Foo;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("(new Foo).x"),
        "Destructured new expression needs parens for property access.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new Foo.x"),
        "Should NOT produce `new Foo.x` (different semantics).\nOutput:\n{output}"
    );
}

#[test]
fn destructuring_new_with_args_no_extra_parens() {
    // var { x } = <any>new Foo(); -> var x = new Foo().x; (no extra parens needed)
    let source = "var { x } = <any>new Foo();\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("new Foo().x"),
        "new Foo() with args should NOT have extra parens.\nOutput:\n{output}"
    );
}

#[test]
fn empty_binding_patterns_with_identifier_rhs_emit_temp() {
    let source = "let {} = undefined;\nlet {} = maybe;\nlet [] = xs;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    assert!(
        output.contains("var _a = undefined;"),
        "Empty object binding with `undefined` RHS should still evaluate through a temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _b = maybe;"),
        "Empty object binding with identifier RHS should still evaluate through a temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _c = xs;"),
        "Empty array binding with identifier RHS should still evaluate through a temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var ;"),
        "Empty binding patterns must not emit an empty variable declaration.\nOutput:\n{output}"
    );
}
