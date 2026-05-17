use super::*;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;

fn emit_class(source: &str) -> String {
    emit_class_with(source, false, false)
}

fn emit_class_with(
    source: &str,
    tc39_decorators: bool,
    use_define_for_class_fields: bool,
) -> String {
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
                emitter.set_tc39_decorators(tc39_decorators);
                emitter.set_use_define_for_class_fields(use_define_for_class_fields);
                return emitter.emit_class(stmt_idx);
            }
        }
    }
    String::new()
}

#[test]
fn test_async_resource_methods_use_disposable_context() {
    let source = r#"
class C {
    a = async () => {
        await using d = { async [Symbol.asyncDispose]() {} };
    };

    async am() {
        await using d = { async [Symbol.asyncDispose]() {} };
        await null;
    }

    async *ag() {
        await using d = { async [Symbol.asyncDispose]() {} };
        yield;
        await null;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_file = parser
        .arena
        .get_source_file(parser.arena.get(root).expect("root node"))
        .expect("source file");
    let class_idx = source_file.statements.nodes[0];

    let mut emitter = ClassES5Emitter::new(&parser.arena);
    emitter.set_source_text(source);
    emitter.set_disposable_env_context(21, Vec::<String>::new());
    let mut inner_name_counts = rustc_hash::FxHashMap::default();
    inner_name_counts.insert("ag".to_string(), 1);
    emitter.set_async_generator_inner_name_counts(inner_name_counts);
    let output = emitter.emit_class(class_idx);

    assert!(
        output.contains("var env_21, d, e_21, result_21;"),
        "Async resource field initializer should consume env_21.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _this = this;"),
        "Async arrow field initializers should capture lexical `this` once in the constructor.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__awaiter(_this, void 0, void 0"),
        "Async arrow field initializers should pass the captured class instance to __awaiter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var env_22, d, e_22, result_22;"),
        "First async resource method should consume env_22.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var env_23, d, e_23, result_23;"),
        "Second async resource method should consume env_23.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function ag_2()"),
        "Class async generator inner names should continue the outer printer's suffix sequence.\nOutput:\n{output}"
    );
    assert_eq!(
        emitter.disposable_env_counter(),
        24,
        "Class emitter should publish the next disposable env id"
    );
    assert_eq!(
        emitter
            .take_async_generator_inner_name_counts()
            .get("ag")
            .copied(),
        Some(2),
        "Class emitter should publish async generator inner-name counters"
    );
}

#[test]
fn test_simple_class() {
    let output = emit_class("class Point { }");
    assert!(
        output.contains("var Point = /** @class */ (function ()"),
        "Should have class IIFE: {output}"
    );
    assert!(
        output.contains("function Point()"),
        "Should have constructor: {output}"
    );
    assert!(
        output.contains("return Point;"),
        "Should return class name: {output}"
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
        "Should have constructor with params: {output}"
    );
}

#[test]
fn test_tc39_method_decorator_wraps_es5_class_and_initializes_parameter_property() {
    let output = emit_class_with(
        r#"class C {
            constructor(private message: string) {}
            @bound speak() {}
        }"#,
        true,
        false,
    );

    assert!(
        output.contains("var C = function () {"),
        "Expected TC39 decorator wrapper around ES5 class.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _instanceExtraInitializers = [];"),
        "Expected instance extra initializers.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "this.message = (__runInitializers(this, _instanceExtraInitializers), message);"
        ),
        "Expected parameter property assignment to consume instance initializers.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__esDecorate(_a, null, _speak_decorators, { kind: \"method\", name: \"speak\", static: false, private: false"),
        "Expected TC39 method decorator application.\nOutput:\n{output}"
    );
}

#[test]
fn test_tc39_method_decorator_define_parameter_property_uses_initializer_value() {
    let output = emit_class_with(
        r#"class C {
            constructor(private message: string) {}
            @bound speak() {}
        }"#,
        true,
        true,
    );

    assert!(
        output.contains("Object.defineProperty(this, \"message\""),
        "Expected parameter property to use defineProperty mode.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value: (__runInitializers(this, _instanceExtraInitializers), message)"),
        "Expected defineProperty value to consume instance initializers.\nOutput:\n{output}"
    );
}

#[test]
fn test_legacy_accessor_decorator_metadata_uses_setter_parameter_type() {
    let source = r#"class A {
        @dec get x() { return 0; }
        set x(value: number) {}
    }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut output = String::new();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
    {
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(node) = parser.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::CLASS_DECLARATION
            {
                let mut emitter = ClassES5Emitter::new(&parser.arena);
                emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators: Vec::new(),
                    has_member_decorators: true,
                    emit_decorator_metadata: true,
                });
                output = emitter.emit_class(stmt_idx);
                break;
            }
        }
    }

    assert!(
        output.contains("__metadata(\"design:type\", Number),"),
        "Expected accessor metadata design:type to come from the setter parameter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:paramtypes\", [Number])"),
        "Expected accessor metadata paramtypes to include the setter parameter type.\nOutput:\n{output}"
    );
}

#[test]
fn test_legacy_async_method_decorator_metadata_without_annotation_uses_promise() {
    let source = r#"class A {
        @dec async inferred() {}
        @dec async explicitAny(): any { return 1; }
    }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut output = String::new();

    if let Some(root_node) = parser.arena.get(root)
        && let Some(source_file) = parser.arena.get_source_file(root_node)
    {
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(node) = parser.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::CLASS_DECLARATION
            {
                let mut emitter = ClassES5Emitter::new(&parser.arena);
                emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators: Vec::new(),
                    has_member_decorators: true,
                    emit_decorator_metadata: true,
                });
                output = emitter.emit_class(stmt_idx);
                break;
            }
        }
    }

    assert!(
        output.contains("__metadata(\"design:returntype\", Promise)"),
        "Inferred async ES5 method metadata should use Promise.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:returntype\", Object)"),
        "Explicit async `any` ES5 method metadata should serialize normally.\nOutput:\n{output}"
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
fn test_arrow_body_computed_object_temp_is_function_scoped() {
    let output = emit_class(
        r#"class C extends Base {
            constructor() {
                super();
                () => {
                    var obj = {
                        // computed key comment
                        [(super(), "prop")]() { }
                    };
                };
            }
        }"#,
    );

    assert!(
        output.contains("function () {\n            var _a;"),
        "Computed object temp should be scoped to the lowered arrow body: {output}"
    );
    assert!(
        !output.contains("function C() {\n        var _a;"),
        "Computed object temp should not be hoisted to the constructor: {output}"
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
        "Should have prototype method: {output}"
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
        "Should have static method: {output}"
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
        "Should have WeakMap declaration: {output}"
    );
    assert!(
        output.contains("_Container_value.set("),
        "Should have WeakMap.set call: {output}"
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
        "Should have Object.defineProperty: {output}"
    );
    assert!(output.contains("get:"), "Should have getter: {output}");
    assert!(output.contains("set:"), "Should have setter: {output}");
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
        "Constructor trailing comment should be preserved: {output}"
    );
}

#[test]
fn test_empty_constructor_inner_comments_preserved() {
    let output = emit_class(
        r#"class C {
            /** constructor comment
            */
            constructor() {
                /** constructor comment2
                */
            }
        }"#,
    );

    assert!(
        output.contains("    /** constructor comment\n    */\n    function C() {"),
        "Constructor leading block comment should keep TypeScript's ES5 indentation.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "    function C() {\n        /** constructor comment2\n                */\n    }"
        ),
        "Detached block comment inside an empty constructor should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn test_empty_constructor_detached_line_comments_preserved() {
    let output = emit_class(
        r#"class C {
            constructor() {
                /// detached

                // before close
            }
        }"#,
    );

    assert!(
        output.contains("    function C() {\n        /// detached\n        // before close\n    }"),
        "Detached line comments inside an empty constructor should be preserved.\nOutput:\n{output}"
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
        "Recovery emit should keep `$` in identifier: {output}"
    );
}
