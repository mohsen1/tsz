use super::*;

// =============================================================================
// TC39 Decorator Emitter - Basic Smoke Tests
// =============================================================================

fn emit_decorator(source: &str) -> String {
    emit_decorator_with(source, false, false)
}

fn emit_decorator_with(
    source: &str,
    use_static_blocks: bool,
    use_define_for_class_fields: bool,
) -> String {
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
    emitter.set_use_static_blocks(use_static_blocks);
    emitter.set_use_define_for_class_fields(use_define_for_class_fields);
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
// Decorator Temp Hygiene (#3091)
// =============================================================================

/// When the source class body references an identifier with the same name as
/// a generated decorator temporary (e.g. `_classDescriptor`), the transform
/// must rename its temp so the user's reference still resolves to the outer
/// binding. tsc emits `_classDescriptor_1` in this case.
#[test]
fn test_class_descriptor_temp_renamed_when_user_binding_collides() {
    let source = "\
@dec
class C {
    static value = _classDescriptor;
}";
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("root");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("source file");
    let class_idx = source_file.statements.nodes[0];

    let mut emitter = TC39DecoratorEmitter::new(&parser.arena);
    emitter.set_source_text(source);
    let output = emitter.emit_class(class_idx);

    assert!(
        output.contains("let _classDescriptor_1;"),
        "Expected the generated temp to be renamed to _classDescriptor_1 to avoid \
         shadowing the user reference. Output:\n{output}"
    );
    assert!(
        !output.contains("let _classDescriptor;"),
        "Generated temp must not keep the colliding name. Output:\n{output}"
    );
    // ES2015 class-decorator lowering moves static fields after the decorator
    // IIFE; the user reference must still point at the original binding.
    assert!(
        output.contains("_classThis.value = _classDescriptor;"),
        "User binding reference must be preserved unchanged. Output:\n{output}"
    );
}

/// Without a collision, the temp name stays at the default. This locks the
/// hygiene policy as collision-driven, not unconditional rename.
#[test]
fn test_class_descriptor_temp_unchanged_when_no_collision() {
    let source = "@dec class C { }";
    let output = emit_decorator(source);
    assert!(
        output.contains("let _classDescriptor;"),
        "Expected default temp name when no collision exists.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_classDescriptor_1"),
        "Should not suffix when there's no collision.\nOutput:\n{output}"
    );
}

/// `_classExtraInitializers` collides with a user binding referenced inside
/// the class body — the transform must rename its temp to `_1`. (#3091)
#[test]
fn test_class_extra_initializers_temp_renamed_when_user_binding_collides() {
    let source = "\
@dec
class C {
    static value = _classExtraInitializers;
}";
    let output = emit_decorator(source);
    assert!(
        output.contains("let _classExtraInitializers_1 = [];"),
        "Expected _classExtraInitializers temp to be renamed. Output:\n{output}"
    );
    assert!(
        !output.contains("let _classExtraInitializers = [];"),
        "Generated temp must not keep the colliding name. Output:\n{output}"
    );
    assert!(
        output.contains("_classThis.value = _classExtraInitializers;"),
        "User binding reference must be preserved unchanged. Output:\n{output}"
    );
}

/// `_classThis` collides with a user binding — rename. (#3091)
#[test]
fn test_class_this_temp_renamed_when_user_binding_collides() {
    let source = "\
@dec
class C {
    static value = _classThis;
}";
    let output = emit_decorator(source);
    assert!(
        output.contains("let _classThis_1;"),
        "Expected _classThis temp to be renamed. Output:\n{output}"
    );
    assert!(
        output.contains("_classThis_1.value = _classThis;"),
        "User binding reference must be preserved unchanged. Output:\n{output}"
    );
}

/// All three of the issue's listed colliding helpers (#3091) must be renamed
/// together when the class body references each one.
#[test]
fn test_all_three_decorator_temps_renamed_together() {
    let source = "\
@dec
class C {
    static a = _classDescriptor;
    static b = _classExtraInitializers;
    static c = _classThis;
}";
    let output = emit_decorator(source);
    assert!(
        output.contains("let _classDescriptor_1;"),
        "Expected _classDescriptor renamed.\n{output}"
    );
    assert!(
        output.contains("let _classExtraInitializers_1 = [];"),
        "Expected _classExtraInitializers renamed.\n{output}"
    );
    assert!(
        output.contains("let _classThis_1;"),
        "Expected _classThis renamed.\n{output}"
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

#[test]
fn test_instance_method_decorator_initializes_parameter_property_assignment() {
    let source =
        "class Foo {\n    constructor(private message: string) { }\n    @log\n    greet() { }\n}";
    let output = emit_decorator(source);

    assert!(
        output.contains("constructor(message)"),
        "Expected parameter property type/modifier to be stripped from constructor.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "this.message = (__runInitializers(this, _instanceExtraInitializers), message);"
        ),
        "Expected parameter property assignment to run instance initializers first.\nOutput:\n{output}"
    );
}

#[test]
fn test_instance_method_decorator_initializes_parameter_property_class_field() {
    let source =
        "class Foo {\n    constructor(private message: string) { }\n    @log\n    greet() { }\n}";
    let output = emit_decorator_with(source, true, true);

    assert!(
        output.contains("message = __runInitializers(this, _instanceExtraInitializers);"),
        "Expected native field emit to host instance initializers.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this.message = message;"),
        "Expected constructor to assign the parameter property value.\nOutput:\n{output}"
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
