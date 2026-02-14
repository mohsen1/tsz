use super::*;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;

fn emit_class(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
    {
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(node) = parser.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::CLASS_DECLARATION
            {
                let mut emitter = ClassES5Emitter::new(&parser.arena);
                emitter.set_source_text(source);
                return emitter.emit_class(stmt_idx);
            }
        }
    }
    String::new()
}

#[test]
fn test_simple_class() {
    let output = emit_class("class Point { }");
    assert!(
        output.contains("var Point = /** @class */ (function ()"),
        "Should have class IIFE: {}",
        output
    );
    assert!(
        output.contains("function Point()"),
        "Should have constructor: {}",
        output
    );
    assert!(
        output.contains("return Point;"),
        "Should return class name: {}",
        output
    );
}

#[test]
fn test_class_with_constructor() {
    let output = emit_class(
        r#"class Point {
            constructor(x, y) {
                this.x = x;
                this.y = y;
            }
        }"#,
    );
    assert!(
        output.contains("function Point(x, y)"),
        "Should have constructor with params: {}",
        output
    );
}

#[test]
fn test_class_with_extends() {
    let output = emit_class(
        r#"class Dog extends Animal {
            constructor(name) {
                super(name);
            }
        }"#,
    );
    assert!(
        output.contains("(function (_super)"),
        "Should have _super parameter: {}",
        output
    );
    assert!(
        output.contains("__extends(Dog, _super)"),
        "Should have extends helper: {}",
        output
    );
    assert!(
        output.contains("_super.call(this"),
        "Should have super.call pattern: {}",
        output
    );
}

#[test]
fn test_class_with_method() {
    let output = emit_class(
        r#"class Greeter {
            greet() {
                console.log("Hello");
            }
        }"#,
    );
    assert!(
        output.contains("Greeter.prototype.greet = function ()"),
        "Should have prototype method: {}",
        output
    );
}

#[test]
fn test_class_with_static_method() {
    let output = emit_class(
        r#"class Counter {
            static count() {
                return 0;
            }
        }"#,
    );
    assert!(
        output.contains("Counter.count = function ()"),
        "Should have static method: {}",
        output
    );
}

#[test]
fn test_class_with_private_field() {
    let output = emit_class(
        r#"class Container {
            #value = 42;
        }"#,
    );
    assert!(
        output.contains("var _Container_value"),
        "Should have WeakMap declaration: {}",
        output
    );
    assert!(
        output.contains("_Container_value.set("),
        "Should have WeakMap.set call: {}",
        output
    );
}

#[test]
fn test_class_with_getter_setter() {
    let output = emit_class(
        r#"class Person {
            _name: string = "";
            get name() { return this._name; }
            set name(value: string) { this._name = value; }
        }"#,
    );
    assert!(
        output.contains("Object.defineProperty"),
        "Should have Object.defineProperty: {}",
        output
    );
    assert!(output.contains("get:"), "Should have getter: {}", output);
    assert!(output.contains("set:"), "Should have setter: {}", output);
}

#[test]
fn test_declare_class_ignored() {
    let output = emit_class(
        r#"declare class Foo {
            bar(): void;
        }"#,
    );
    assert!(output.is_empty(), "Declare class should produce no output");
}

#[test]
fn test_constructor_trailing_comment_preserved() {
    let output = emit_class(
        r#"class C1 {
            constructor(p3) {
                this.p3 = p3;
            } // OK
        }"#,
    );
    assert!(
        output.contains("} // OK"),
        "Constructor trailing comment should be preserved: {}",
        output
    );
}

#[test]
fn test_var_function_recovery_supports_dollar_identifier() {
    let output = emit_class(
        r#"class C {
            var $constructor() { }
        }"#,
    );
    assert!(
        output.contains("var $constructor;"),
        "Recovery emit should keep `$` in identifier: {}",
        output
    );
}
