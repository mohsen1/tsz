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
        "Should have _super parameter: {output}"
    );
    assert!(
        output.contains("__extends(Dog, _super)"),
        "Should have extends helper: {output}"
    );
    assert!(
        output.contains("_super.call(this"),
        "Should have super.call pattern: {output}"
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
fn test_class_with_auto_accessor_field() {
    let source = r#"class RegularClass {
            accessor shouldError: string;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("var _RegularClass_shouldError_accessor_storage"));
    assert!(output.contains("_RegularClass_shouldError_accessor_storage.set(this, void 0)"));
    assert!(output.contains("Object.defineProperty(RegularClass.prototype, \"shouldError\", {"));
    assert!(output.contains(
        "__classPrivateFieldGet(this, _RegularClass_shouldError_accessor_storage, \"f\")"
    ));
    assert!(output.contains(
        "__classPrivateFieldSet(this, _RegularClass_shouldError_accessor_storage, value, \"f\")"
    ));
}

#[test]
fn test_auto_accessor_without_initializer_does_not_emit_set_undefined() {
    let source = r#"class RegularClass {
            accessor shouldError: string;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(output.contains("_RegularClass_shouldError_accessor_storage.set(this, void 0)"));
    assert!(!output.contains(
        "__classPrivateFieldSet(this, _RegularClass_shouldError_accessor_storage, undefined, \"f\")"
    ));
}

#[test]
fn test_auto_accessor_comment_and_function_bodies() {
    let source = r#"class RegularClass {
            accessor shouldError: string; // Should still error
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(
        output.contains("// Should still error"),
        "Trailing property comment should be preserved for auto accessors.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "get: function () { return __classPrivateFieldGet(this, _RegularClass_shouldError_accessor_storage, \"f\"); } // Should still error",
        ),
        "Auto accessor trailing comment should attach to getter descriptor.\nOutput:\n{output}"
    );
    assert!(output.contains("set: function (value) { __classPrivateFieldSet(this, _RegularClass_shouldError_accessor_storage, value, \"f\"); }"));
    assert!(
        !output
            .contains("var RegularClass = /** @class */ (function () {\n    // Should still error")
    );
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
fn test_class_property_jsdoc_moves_with_initializer_into_constructor() {
    // When a class property's initializer is lifted into the synthesized
    // ES5 constructor body, the JSDoc that decorated the property in source
    // must move with it so user-authored documentation isn't silently
    // dropped during the lowering.
    let source = r#"class C {
    constructor() {
    }

    /** property comment */
    public b = 10;
}"#;

    let output = transform_class(source).expect("transform should succeed");

    let comment_pos = output
        .find("/** property comment */")
        .expect("property JSDoc must survive into the lowered output");
    let init_pos = output
        .find("this.b = 10")
        .expect("property initializer must be lifted into the constructor");
    assert!(
        comment_pos < init_pos,
        "JSDoc must precede the lifted initializer.\nOutput:\n{output}"
    );
}

#[test]
fn test_constructor_body_preserves_multiline_jsdoc_before_statement() {
    // Inside the constructor body, a multi-line JSDoc preceding a real
    // statement (e.g. a `this.field = value` initializer in a JS-style
    // constructor) must be carried through into the lowered output. The
    // line-based comment scanner used to reject it because the opening
    // `/**` line did not also end with `*/`.
    let source = r#"class Aleph {
    constructor(a, b) {
        /**
         * Field is always null
         */
        this.field = b;
    }
}"#;

    let output = transform_class(source).expect("transform should succeed");

    let comment_pos = output
        .find("Field is always null")
        .expect("multi-line JSDoc body must survive into the lowered output");
    let init_pos = output
        .find("this.field = b")
        .expect("constructor initializer must be emitted");
    assert!(
        comment_pos < init_pos,
        "Multi-line JSDoc must precede the statement it documents.\nOutput:\n{output}"
    );
    assert!(
        output.contains("/**"),
        "Opening `/**` must be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("*/"),
        "Closing `*/` must be preserved.\nOutput:\n{output}"
    );
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
fn type_only_computed_field_side_effect_emits_inside_iife() {
    let source = r#"class C {
            [Symbol.isRegExp]: string;
        }"#;

    let output = transform_class(source).expect("transform should succeed in test");

    assert!(
        output.contains("Symbol.isRegExp;\n    return C;"),
        "type-only computed field side effect should emit inside the class IIFE.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return C;\n}());\nSymbol.isRegExp;"),
        "type-only computed field side effect should not be deferred after the class IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn computed_field_temp_assignment_emits_inside_iife() {
    let source = r#"class C {
            [Symbol.toStringTag]: string = "";
        }"#;

    let output = transform_class(source).expect("transform should succeed in test");

    assert!(
        output.contains("function C() {\n        this[_a] = \"\";\n    }\n    var _a;\n    _a = Symbol.toStringTag;\n    return C;"),
        "computed field temp should be declared and assigned inside the class IIFE.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this[_a] = \"\";"),
        "constructor should reference the computed field temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}());\n_a = Symbol.toStringTag;"),
        "computed field temp assignment should not be deferred after the class IIFE.\nOutput:\n{output}"
    );
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

#[test]
fn test_leading_comment_not_duplicated_in_iife() {
    // The ES5 class IIFE must NOT include a leading_comment, because the
    // statement-level comment handler in the main emitter already emits
    // comments that precede the class declaration. Including the comment
    // in the IR would produce duplicate output.
    let source = r#"// No errors
class C {
    foo() {}
}"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // The IR printer should NOT emit "// No errors" — that is the
    // statement-level emitter's responsibility.
    assert!(
        !output.contains("// No errors"),
        "ES5 class IIFE should not include leading comment (handled by statement loop).\nOutput:\n{output}"
    );
    // The class body should still be correct
    assert!(output.contains("var C = /** @class */ (function ()"));
    assert!(output.contains("C.prototype.foo = function ()"));
}

#[test]
fn test_multiple_classes_no_comment_duplication() {
    // Verify that leading comments before multiple classes are not included
    // in the IR output (they're handled by the statement-level emitter).
    let source = r#"// First class comment
class A {
    methodA() {}
}"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    assert!(
        !output.contains("// First class comment"),
        "Leading comment should not appear in ES5 class IR output.\nOutput:\n{output}"
    );
    assert!(output.contains("var A = /** @class */ (function ()"));
}

#[test]
fn test_static_this_class_alias_in_property_initializer() {
    let source = r#"class CC {
            static a = 1;
            static b = this.a + 1;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have var _a; and _a = CC;
    assert!(
        output.contains("var _a;"),
        "Should declare class alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = CC;"),
        "Should assign class to alias.\nOutput:\n{output}"
    );
    // this.a should become _a.a
    assert!(
        output.contains("_a.a + 1"),
        "this should be replaced with _a in static property initializer.\nOutput:\n{output}"
    );
}

#[test]
fn test_static_this_class_alias_in_static_block() {
    let source = r#"class Foo {
            static b = 1;
            static {
                this.b;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Should have var _a; and _a = Foo;
    assert!(
        output.contains("var _a;"),
        "Should declare class alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = Foo;"),
        "Should assign class to alias.\nOutput:\n{output}"
    );
    // this.b inside static block should become _a.b
    assert!(
        output.contains("_a.b"),
        "this should be replaced with _a in static block.\nOutput:\n{output}"
    );
}

#[test]
fn test_static_this_not_replaced_in_static_method() {
    // `this` in static methods should stay as `this` because regular
    // functions have their own `this` binding.
    let source = r#"class DD {
            static c = 2;
            static d = this.c + 1;
            static ff = function () { this.c + 1 }
            static foo () {
                return this.c + 1;
            }
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // Static property initializer: this → _a
    assert!(
        output.contains("_a.c + 1"),
        "this should be replaced with _a in static property initializer.\nOutput:\n{output}"
    );
    // Static method body: this stays as this
    assert!(
        output.contains("return this.c + 1"),
        "this should stay as this in static method body.\nOutput:\n{output}"
    );
    // Function expression in property initializer: this stays as this
    assert!(
        output.contains("function () { this.c + 1; }"),
        "this should stay as this inside function expression.\nOutput:\n{output}"
    );
}

#[test]
fn test_no_class_alias_when_no_this_in_static_members() {
    // If there's no `this` in static members, no class alias should be generated
    let source = r#"class Simple {
            static a = 1;
            static b = 2;
        }"#;

    let output = transform_class(source);
    assert!(output.is_some());
    let output = output.expect("transform should succeed in test");

    // No class alias needed
    assert!(
        !output.contains("var _a"),
        "Should not declare class alias when this is not used in static members.\nOutput:\n{output}"
    );
}

// Issue #3967: a class with only a static block (no static properties) that
// references `this` must declare/assign the class alias outside the IIFE so
// the deferred static block can reference it. Previously only the
// has_static_props path emitted the alias preamble, leaving classes like
// `class C { static { this.name; } }` with an undeclared `_a` reference at
// runtime.
#[test]
fn test_static_block_only_class_alias_preamble() {
    let source = r#"class C {
            static { console.log("block", this.name); }
        }"#;

    let output = transform_class(source).expect("transform should succeed");

    assert!(
        output.contains("var _a;"),
        "static-block-only class with this should declare alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = C;"),
        "static-block-only class with this should assign alias to class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a.name"),
        "this in static block should be replaced with _a.\nOutput:\n{output}"
    );

    // The alias must be assigned BEFORE the static-block IIFE runs, so the
    // block does not read undefined `_a`. The class IIFE wrapper also begins
    // with `(function () {`, so we anchor the search past the closing
    // `}());` of the class IIFE.
    let class_iife_end = output
        .find("}());")
        .expect("class IIFE should close before the static-block IIFE")
        + "}());".len();
    let assign_idx = output.find("_a = C;").expect("assignment should exist");
    let block_idx = output[class_iife_end..]
        .find("(function () {")
        .map(|i| i + class_iife_end)
        .expect("static-block IIFE should exist after the class IIFE");
    assert!(
        assign_idx < block_idx,
        "alias must be assigned before the static-block IIFE.\nOutput:\n{output}"
    );
}
