
use super::*;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::ParserState;

fn transform_class(source: &str) -> Option<String> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root)?;
    let source_file = parser.arena.get_source_file(root_node)?;

    // Find the class declaration
    for &stmt_idx in &source_file.statements.nodes {
        if let Some(node) = parser.arena.get(stmt_idx)
            && node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            let mut transformer = ES5ClassTransformer::new(&parser.arena);
            transformer.set_source_text(source);
            if let Some(ir) = transformer.transform_class_to_ir(stmt_idx) {
                let mut printer = IRPrinter::with_arena(&parser.arena);
                printer.set_source_text(source);
                return Some(printer.emit(&ir).to_string());
            }
        }
    }

    None
}

#[test]
fn test_simple_class() {
    let source = r#"class Point {
            x: number;
            y: number;
            constructor(x: number, y: number) {
                this.x = x;
                this.y = y;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("var Point = /** @class */ (function ()"));
    assert!(output.contains("function Point(x, y)"));
    assert!(output.contains("return Point;"));
}

#[test]
fn test_class_with_extends() {
    let source = r#"class Dog extends Animal {
            constructor(name: string) {
                super(name);
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some(), "Transform should produce output");
    let output = output.expect("transform should succeed in test");

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
    let source = r#"class Greeter {
            greet() {
                console.log("Hello");
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("Greeter.prototype.greet = function ()"));
}

#[test]
fn test_class_with_static_method() {
    let source = r#"class Counter {
            static count() {
                return 0;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("Counter.count = function ()"));
}

#[test]
fn test_class_with_private_field() {
    let source = r#"class Container {
            #value = 42;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("var _Container_value"));
    assert!(output.contains("_Container_value.set(this, void 0)"));
    assert!(output.contains("_Container_value = new WeakMap()"));
}

#[test]
fn test_class_with_parameter_property() {
    let source = r#"class Point {
            constructor(public x: number, public y: number) {}
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("this.x = x"));
    assert!(output.contains("this.y = y"));
}

#[test]
fn test_derived_class_default_constructor() {
    let source = r#"class Child extends Parent {
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("__extends(Child, _super)"));
    assert!(
        output.contains("_super !== null && _super.apply(this, arguments) || this")
            || output.contains("_super.apply(this, arguments)")
    );
}

#[test]
fn test_class_with_instance_property() {
    let source = r#"class Counter {
            count = 0;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("this.count ="));
}

#[test]
fn test_declare_class_ignored() {
    let source = r#"declare class Foo {
            bar(): void;
        }"#;

    let output = transform_class(source);
    assert!(output.is_none());
}

#[test]
fn test_accessor_pair_combined() {
    let source = r#"class Person {
            _name: string = "";
            get name() { return this._name; }
            set name(value: string) { this._name = value; }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have single Object.defineProperty call with both get and set
    assert!(output.contains("Object.defineProperty"));
    assert!(output.contains("get:"));
    assert!(output.contains("set:"));
    assert!(output.contains("enumerable: false"));
    assert!(output.contains("configurable: true"));
}

#[test]
fn test_static_accessor_combined() {
    let source = r#"class Config {
            static _instance: Config | null = null;
            static get instance() { return Config._instance; }
            static set instance(value: Config) { Config._instance = value; }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have Object.defineProperty on class directly (not prototype)
    assert!(output.contains("Object.defineProperty(Config,"));
    assert!(output.contains("get:"));
    assert!(output.contains("set:"));
}

#[test]
fn test_async_method() {
    let source = r#"class Fetcher {
            async fetch() {
                return await Promise.resolve(42);
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Async method should have __awaiter wrapper
    assert!(output.contains("__awaiter"));
}

#[test]
fn test_static_async_method() {
    let source = r#"class API {
            static async request() {
                return await fetch("/api");
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Static async method should have __awaiter wrapper
    assert!(output.contains("API.request = function ()"));
    assert!(output.contains("__awaiter"));
}

#[test]
fn test_computed_method_name() {
    let source = r#"class Container {
            [Symbol.iterator]() {
                return this;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Computed method name should use bracket notation
    assert!(output.contains("Container.prototype[Symbol.iterator]"));
}

#[test]
fn test_getter_only() {
    let source = r#"class ReadOnly {
            get value() { return 42; }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have DefineProperty with only get
    assert!(output.contains("Object.defineProperty"));
    assert!(output.contains("get:"));
    // Should still have enumerable and configurable
    assert!(output.contains("enumerable: false"));
    assert!(output.contains("configurable: true"));
}

#[test]
fn test_setter_only() {
    let source = r#"class WriteOnly {
            set value(v: number) { console.log(v); }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have DefineProperty with only set
    assert!(output.contains("Object.defineProperty"));
    assert!(output.contains("set:"));
}

#[test]
fn test_static_block() {
    let source = r#"class Initializer {
            static value: number;
            static {
                Initializer.value = 42;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Static block content should be emitted
    assert!(output.contains("Initializer.value = 42"));
}

#[test]
fn test_string_method_name() {
    let source = r#"class StringMethods {
            "my-method"() {
                return 1;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // String literal method name should use bracket notation
    assert!(output.contains("StringMethods.prototype[\"my-method\"]"));
}

#[test]
fn test_numeric_method_name() {
    let source = r#"class NumericMethods {
            42() {
                return "answer";
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Numeric literal method name should use bracket notation
    assert!(output.contains("NumericMethods.prototype[42]"));
}
