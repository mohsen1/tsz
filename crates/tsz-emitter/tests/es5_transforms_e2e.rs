//! End-to-end ES5 transform tests using the full `lower_and_print` pipeline.
//!
//! These tests verify that the complete chain (parse -> lower -> print) produces
//! correct ES5 output for destructuring, class, and async transforms.

use crate::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions::es5();
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es5_with_comments(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, PrintOptions::es5()).code
}

// =============================================================================
// Array Destructuring
// =============================================================================

#[test]
fn test_array_destructuring_basic() {
    let output = emit_es5("const [a, b] = arr;\n");
    assert!(
        !output.contains("[a, b]"),
        "ES5 output should not contain array destructuring syntax.\nOutput:\n{output}"
    );
    // Should have temp variable
    assert!(
        output.contains("var") || output.contains("_a"),
        "Expected ES5 variable declaration.\nOutput:\n{output}"
    );
}

#[test]
fn test_object_destructuring_basic() {
    let output = emit_es5("const {x, y} = obj;\n");
    assert!(
        !output.contains("{x, y}"),
        "ES5 output should not contain object destructuring syntax.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_with_default_value() {
    let output = emit_es5("const {x = 5} = obj;\n");
    assert!(
        output.contains("void 0") || output.contains("undefined") || output.contains("5"),
        "Expected default value handling.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rest_array() {
    let output = emit_es5("const [first, ...rest] = arr;\n");
    assert!(
        output.contains("slice"),
        "Expected Array.prototype.slice for rest elements.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_nested_object() {
    let output = emit_es5("const {a: {b}} = obj;\n");
    assert!(
        !output.contains("{a:"),
        "ES5 should not contain nested destructuring syntax.\nOutput:\n{output}"
    );
    // Should reference .a and .b through temp variables
    assert!(
        output.contains(".a") || output.contains("[\"a\"]"),
        "Expected property access for nested destructuring.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rename() {
    let output = emit_es5("const {x: renamed} = obj;\n");
    assert!(
        output.contains("renamed"),
        "Expected renamed binding.\nOutput:\n{output}"
    );
}

// =============================================================================
// Class ES5 Transform
// =============================================================================

#[test]
fn test_class_to_iife() {
    let output = emit_es5_with_comments(
        "class Point {\n    constructor(x, y) {\n        this.x = x;\n        this.y = y;\n    }\n}\n",
    );
    assert!(
        output.contains("/** @class */"),
        "Expected @class annotation.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function Point("),
        "Expected constructor function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return Point;"),
        "Expected return statement.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_extends_to_iife() {
    let output = emit_es5("class Dog extends Animal {\n    bark() { return 'woof'; }\n}\n");
    assert!(
        output.contains("__extends"),
        "Expected __extends helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super"),
        "Expected _super parameter.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_static_method() {
    let output = emit_es5("class Counter {\n    static count() { return 0; }\n}\n");
    assert!(
        output.contains("Counter.count = function"),
        "Expected static method on class directly.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_prototype_method() {
    let output = emit_es5("class Greeter {\n    greet() { return 'hello'; }\n}\n");
    assert!(
        output.contains("Greeter.prototype.greet = function"),
        "Expected prototype method.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_private_field_weakmap() {
    let output = emit_es5("class Container {\n    #value = 42;\n}\n");
    assert!(
        output.contains("WeakMap"),
        "Expected WeakMap for private field.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_property_initializer() {
    let output = emit_es5("class Counter {\n    count = 0;\n}\n");
    assert!(
        output.contains("this.count ="),
        "Expected property initializer in constructor.\nOutput:\n{output}"
    );
}

#[test]
fn test_computed_string_field_preserves_source_quotes() {
    let output = emit_es5("class C {\n    ['this'] = '';\n}\n");
    assert!(
        output.contains("this['this'] = '';"),
        "Expected computed string field to preserve source quotes.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Expected computed string field not to be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_getter_setter_define_property() {
    let output = emit_es5("class Foo {\n    get bar() { return 1; }\n    set bar(v) {}\n}\n");
    assert!(
        output.contains("Object.defineProperty"),
        "Expected Object.defineProperty for accessors.\nOutput:\n{output}"
    );
}

// =============================================================================
// Arrow Function ES5 Transform
// =============================================================================

#[test]
fn test_arrow_function_to_function() {
    let output = emit_es5("const f = (x) => x * 2;\n");
    assert!(
        !output.contains("=>"),
        "ES5 should not contain arrow function syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function"),
        "Expected function keyword.\nOutput:\n{output}"
    );
}

#[test]
fn test_arrow_function_this_capture() {
    let output = emit_es5("class Foo {\n    bar() {\n        const f = () => this;\n    }\n}\n");
    assert!(
        output.contains("_this"),
        "Expected _this capture for arrow function using this.\nOutput:\n{output}"
    );
}

// =============================================================================
// Let/Const -> Var
// =============================================================================

#[test]
fn test_let_becomes_var() {
    let output = emit_es5("let x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected let to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let x"),
        "let should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn test_const_becomes_var() {
    let output = emit_es5("const x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected const to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const x"),
        "const should not appear in ES5.\nOutput:\n{output}"
    );
}

// =============================================================================
// Async Function Transform
// =============================================================================

#[test]
fn test_async_function_awaiter() {
    let output = emit_es5("async function fetchData() {\n    await fetch('/api');\n}\n");
    assert!(
        output.contains("__awaiter"),
        "Expected __awaiter helper for async function.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("async "),
        "async keyword should not appear in ES5.\nOutput:\n{output}"
    );
}

// =============================================================================
// Template Literals
// =============================================================================

#[test]
fn test_template_literal_to_concatenation() {
    let output = emit_es5("const msg = `Hello ${name}!`;\n");
    // ES5 should convert template literals to string concatenation
    assert!(
        !output.contains('`'),
        "ES5 should not contain template literal syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("+") || output.contains("concat"),
        "Expected string concatenation.\nOutput:\n{output}"
    );
}

// =============================================================================
// Spread Transform
// =============================================================================

#[test]
fn test_spread_in_call() {
    let output = emit_es5("foo(...args);\n");
    assert!(
        !output.contains("...args"),
        "ES5 should not contain spread syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("apply") || output.contains("__spreadArray"),
        "Expected apply or __spreadArray for spread.\nOutput:\n{output}"
    );
}

// =============================================================================
// Exponentiation Transform (ES2016)
// =============================================================================

#[test]
fn test_exponentiation_to_math_pow() {
    let output = emit_es5("const x = 2 ** 3;\n");
    assert!(
        output.contains("Math.pow"),
        "Expected Math.pow for exponentiation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("**"),
        "ES5 should not contain ** operator.\nOutput:\n{output}"
    );
}

// =============================================================================
// Enum ES5 Transform
// =============================================================================

#[test]
fn test_enum_to_iife() {
    let output = emit_es5_with_comments("enum Color {\n    Red,\n    Green,\n    Blue\n}\n");
    // Enums become IIFEs in ES5
    assert!(
        output.contains("Color[Color[") || output.contains("Color[\"Red\"]"),
        "Expected enum IIFE pattern.\nOutput:\n{output}"
    );
}

// =============================================================================
// Type Stripping
// =============================================================================

#[test]
fn test_type_annotations_stripped() {
    let output = emit_es5("const x: number = 42;\n");
    assert!(
        !output.contains(": number"),
        "Type annotations should be stripped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("42"),
        "Value should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn test_interface_stripped() {
    let output = emit_es5("interface Point { x: number; y: number; }\n");
    assert!(
        !output.contains("interface"),
        "Interface should be stripped from JS output.\nOutput:\n{output}"
    );
}

#[test]
fn test_type_alias_stripped() {
    let output = emit_es5("type ID = string | number;\n");
    assert!(
        !output.contains("type ID"),
        "Type alias should be stripped from JS output.\nOutput:\n{output}"
    );
}
