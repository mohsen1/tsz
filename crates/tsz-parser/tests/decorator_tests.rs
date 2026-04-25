//! Tests for decorator parsing, particularly ES decorator support (TC39 Stage 3).
use crate::parser::ParserState;
use crate::parser::test_fixture::parse_source;

fn has_error_code(parser: &ParserState, code: u32) -> bool {
    parser.get_diagnostics().iter().any(|d| d.code == code)
}

/// ES decorators on class expressions are valid (TC39 Stage 3).
/// The parser should NOT emit TS1206 for `(@dec class {})`.
#[test]
fn es_decorator_on_class_expression_no_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\nconst C = @dec class {};");
    assert!(
        !has_error_code(&parser, 1206),
        "ES decorators on class expressions should not produce TS1206"
    );
}

/// Multiple ES decorators on a class expression should not produce TS1206.
#[test]
fn multiple_es_decorators_on_class_expression_no_ts1206() {
    let (parser, _root) =
        parse_source("declare var d1: any;\ndeclare var d2: any;\nconst C = @d1 @d2 class {};");
    assert!(
        !has_error_code(&parser, 1206),
        "multiple ES decorators on class expression should not produce TS1206"
    );
}

/// Parenthesized decorated class expression: `(@dec class {})`.
#[test]
fn parenthesized_decorated_class_expression_no_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\n(@dec class C {});");
    assert!(
        !has_error_code(&parser, 1206),
        "parenthesized decorated class expression should not produce TS1206"
    );
}

/// `export default @dec class {}` should not produce TS1206.
#[test]
fn export_default_decorated_class_no_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\nexport default @dec class {};");
    assert!(
        !has_error_code(&parser, 1206),
        "export default with decorated class should not produce TS1206"
    );
}

/// `export default @dec class {}` with a name should not produce TS1206.
#[test]
fn export_default_decorated_named_class_no_ts1206() {
    let (parser, _root) =
        parse_source("declare var dec: any;\nexport default @dec class MyClass {};");
    assert!(
        !has_error_code(&parser, 1206),
        "export default with decorated named class should not produce TS1206"
    );
}

/// Decorators on class declarations remain valid (baseline sanity check).
#[test]
fn decorator_on_class_declaration_no_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\n@dec class C {}");
    assert!(
        !has_error_code(&parser, 1206),
        "decorator on class declaration should not produce TS1206"
    );
}

/// Decorators on non-class constructs (e.g., functions) should still produce TS1206.
#[test]
fn decorator_on_function_produces_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\n@dec function foo() {}");
    assert!(
        has_error_code(&parser, 1206),
        "decorator on function should produce TS1206"
    );
}

/// Decorators on enum declarations should still produce TS1206.
#[test]
fn decorator_on_enum_produces_ts1206() {
    let (parser, _root) = parse_source("declare var dec: any;\n@dec enum E { A }");
    assert!(
        has_error_code(&parser, 1206),
        "decorator on enum should produce TS1206"
    );
}

/// TS1436: Decorator placed after `public` modifier instead of before it.
#[test]
fn decorator_after_public_modifier_produces_ts1436() {
    let (parser, _root) = parse_source(
        "declare function dec(t: any, k: string): void;\nclass C { public @dec prop; }",
    );
    assert!(
        has_error_code(&parser, 1436),
        "decorator after 'public' modifier should produce TS1436"
    );
    // Should NOT produce generic TS1146 (Declaration expected)
    assert!(
        !has_error_code(&parser, 1146),
        "should not produce generic TS1146 when TS1436 applies"
    );
}

/// TS1436: Decorator placed after `static` modifier on a method.
#[test]
fn decorator_after_static_modifier_produces_ts1436() {
    let (parser, _root) =
        parse_source("declare var dec: any;\nclass C { static @dec method() {} }");
    assert!(
        has_error_code(&parser, 1436),
        "decorator after 'static' modifier should produce TS1436"
    );
}

/// TS1436: Decorator placed after `private` modifier on a get accessor.
#[test]
fn decorator_after_private_modifier_on_accessor_produces_ts1436() {
    let (parser, _root) = parse_source(
        "declare var dec: any;\nclass C { private @dec get accessor() { return 1; } }",
    );
    assert!(
        has_error_code(&parser, 1436),
        "decorator after 'private' on get accessor should produce TS1436"
    );
}

/// TS1436: Decorator placed after `protected` modifier on a set accessor.
#[test]
fn decorator_after_protected_modifier_on_setter_produces_ts1436() {
    let (parser, _root) = parse_source(
        "declare var dec: any;\nclass C { protected @dec set accessor(v: number) {} }",
    );
    assert!(
        has_error_code(&parser, 1436),
        "decorator after 'protected' on set accessor should produce TS1436"
    );
}

/// Normal decorator position (before modifiers) should NOT produce TS1436.
#[test]
fn decorator_before_modifiers_no_ts1436() {
    let (parser, _root) =
        parse_source("declare var dec: any;\nclass C { @dec public prop: number = 1; }");
    assert!(
        !has_error_code(&parser, 1436),
        "decorator before modifier is valid and should not produce TS1436"
    );
}

/// TS1436: Decorator placed after property name (e.g., `private prop @decorator`).
#[test]
fn decorator_after_property_name_produces_ts1436() {
    let (parser, _root) = parse_source(
        "declare var decorator: any;\nclass Foo {\n  private prop @decorator\n  foo() { return 0; }\n}",
    );
    assert!(
        has_error_code(&parser, 1436),
        "decorator after property name should produce TS1436"
    );
    // Should NOT produce generic TS1146 (Declaration expected)
    assert!(
        !has_error_code(&parser, 1146),
        "should not produce generic TS1146 when TS1436 applies"
    );
}
