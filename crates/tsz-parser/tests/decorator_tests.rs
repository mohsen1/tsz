//! Tests for decorator parsing, particularly ES decorator support (TC39 Stage 3).
use crate::parser::{NodeIndex, ParserState};

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

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
