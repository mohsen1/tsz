#[test]
fn test_parser_unterminated_template_literal_reports_ts1160() {
    let source = "`";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|diag| diag.code == diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL),
        "Expected unterminated template literal diagnostic, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_template_literal_property_name_no_ts1160() {
    let source = "var x = { `abc${ 123 }def${ 456 }ghi`: 321 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "Expected property assignment expected diagnostic, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL),
        "Did not expect unterminated template literal diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn test_parser_double_comma_emits_ts1136() {
    let source = "Boolean({ x: 0,, });";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "Expected TS1136 property assignment expected diagnostic for double comma, got: {diagnostics:?}"
    );
}

#[test]
fn test_parser_call_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "foo(1, 2, 3);".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_property_access() {
    let mut parser = ParserState::new("test.ts".to_string(), "obj.foo.bar;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_new_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "new Foo(1, 2);".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_class_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { x = 1; bar() { return this.x; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_with_constructor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Point { constructor(x, y) { this.x = x; this.y = y; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_member_named_var() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { var() { return 1; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_extends() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Parent { constructor() { super(); } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // May have some diagnostics for super() but should parse successfully
}

