use super::*;

// =============================================================================
// TC39 Decorator Emitter - Basic Smoke Tests
// =============================================================================

fn emit_decorator(source: &str) -> String {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).expect("expected root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("expected source file data");
    let class_idx = source_file.statements.nodes[0];

    let mut emitter = TC39DecoratorEmitter::new(&parser.arena);
    emitter.set_source_text(source);
    emitter.emit_class(class_idx)
}

#[test]
fn test_decorator_emitter_creation() {
    let arena = NodeArena::new();
    let emitter = TC39DecoratorEmitter::new(&arena);
    // Should produce empty string for NONE index
    let result = emitter.emit_class(NodeIndex::NONE);
    assert!(
        result.is_empty(),
        "Expected empty string for NONE class index"
    );
}

#[test]
fn test_decorator_emitter_indent_level() {
    let arena = NodeArena::new();
    let mut emitter = TC39DecoratorEmitter::new(&arena);
    emitter.set_indent_level(2);
    // Just verify it compiles and doesn't panic
    let result = emitter.emit_class(NodeIndex::NONE);
    assert!(result.is_empty());
}

// =============================================================================
// Class Decorator Application
// =============================================================================

#[test]
fn test_class_decorator_produces_iife() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("let Foo = (() => {"),
        "Expected IIFE wrapper for decorated class.\nOutput:\n{output}"
    );
    // In ES2015 decorator mode, the class is anonymous (name set via __setFunctionName)
    assert!(
        output.contains("var Foo = _classThis = class"),
        "Expected class variable assignment in output.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_decorator_has_class_decorators_array() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("_classDecorators = [sealed]"),
        "Expected _classDecorators array.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_decorator_has_metadata() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("Symbol.metadata"),
        "Expected Symbol.metadata check.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_decorator_has_es_decorate_call() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("__esDecorate"),
        "Expected __esDecorate call.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_decorator_has_class_extra_initializers() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("_classExtraInitializers"),
        "Expected _classExtraInitializers.\nOutput:\n{output}"
    );
}

// =============================================================================
// Method Decorator
// =============================================================================

#[test]
fn test_method_decorator_produces_decorator_var() {
    let source = "class Foo {\n    @log\n    greet() { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("let Foo = (() =>"),
        "Expected IIFE for class with method decorators.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_instanceExtraInitializers"),
        "Expected instance extra initializers for instance method.\nOutput:\n{output}"
    );
}

#[test]
fn test_method_decorator_has_es_decorate_with_method_kind() {
    let source = "class Foo {\n    @log\n    greet() { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("__esDecorate"),
        "Expected __esDecorate call for method decorator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"method\""),
        "Expected method kind in decorator context.\nOutput:\n{output}"
    );
}

#[test]
fn test_static_method_decorator() {
    let source = "class Foo {\n    @log\n    static greet() { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("_staticExtraInitializers"),
        "Expected static extra initializers for static method.\nOutput:\n{output}"
    );
}

// =============================================================================
// Property Decorator
// =============================================================================

#[test]
fn test_property_decorator() {
    let source = "class Foo {\n    @validate\n    name = 'bar';\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("let Foo = (() =>"),
        "Expected IIFE for class with property decorators.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__esDecorate"),
        "Expected __esDecorate call for property decorator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"field\""),
        "Expected field kind in decorator context.\nOutput:\n{output}"
    );
}

// =============================================================================
// Getter/Setter Decorator
// =============================================================================

#[test]
fn test_getter_decorator() {
    let source = "class Foo {\n    @log\n    get value() { return 1; }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("__esDecorate"),
        "Expected __esDecorate call for getter decorator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"getter\""),
        "Expected getter kind in decorator context.\nOutput:\n{output}"
    );
}

#[test]
fn test_setter_decorator() {
    let source = "class Foo {\n    @log\n    set value(v: number) { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("__esDecorate"),
        "Expected __esDecorate call for setter decorator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"setter\""),
        "Expected setter kind in decorator context.\nOutput:\n{output}"
    );
}

// =============================================================================
// Constructor Handling
// =============================================================================

#[test]
fn test_decorated_class_emits_constructor() {
    let source = "@sealed class Foo {\n    constructor(x: number) { this.x = x; }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("constructor("),
        "Expected constructor in decorated class output.\nOutput:\n{output}"
    );
}

#[test]
fn test_decorated_class_without_constructor_omits_constructor() {
    // tsc does NOT inject a default constructor for class-only decorators.
    // A constructor is only emitted when there are instance member decorators
    // that need __runInitializers in the constructor body.
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        !output.contains("constructor()"),
        "Class-only decorators should not inject a default constructor.\nOutput:\n{output}"
    );
}

// =============================================================================
// Instance Extra Initializers in Constructor
// =============================================================================

#[test]
fn test_instance_method_decorator_adds_run_initializers_in_constructor() {
    let source = "class Foo {\n    @log\n    greet() { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("__runInitializers(this, _instanceExtraInitializers)"),
        "Expected __runInitializers call in constructor for instance decorators.\nOutput:\n{output}"
    );
}

// =============================================================================
// Multiple Decorators
// =============================================================================

#[test]
fn test_multiple_class_decorators() {
    let source = "@first @second class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("first") && output.contains("second"),
        "Expected both decorator names in output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_classDecorators"),
        "Expected class decorators array for multiple decorators.\nOutput:\n{output}"
    );
}

// =============================================================================
// Extends Clause
// =============================================================================

#[test]
fn test_decorated_class_with_extends() {
    let source = "@sealed class Dog extends Animal { }";
    let output = emit_decorator(source);

    // With class decorators, tsc captures the super class in _classSuper
    assert!(
        output.contains("let _classSuper = Animal;"),
        "Expected _classSuper variable declaration.\nOutput:\n{output}"
    );
    assert!(
        output.contains("extends _classSuper"),
        "Expected extends clause to use _classSuper alias.\nOutput:\n{output}"
    );
}

// =============================================================================
// Return Statement
// =============================================================================

#[test]
fn test_iife_has_return() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("return"),
        "Expected return statement in IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn test_iife_closes_properly() {
    let source = "@sealed class Foo { }";
    let output = emit_decorator(source);

    assert!(
        output.contains("})()"),
        "Expected IIFE closing pattern.\nOutput:\n{output}"
    );
}
