#[test]
fn test_parser_function_type_no_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let factory: () => Widget;".to_string(),
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
fn test_parser_function_type_multiple_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let handler: (a: string, b: number, c: boolean) => void;".to_string(),
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
fn test_parser_function_type_optional_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let fn: (x: number, y?: string) => void;".to_string(),
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
fn test_parser_function_type_rest_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let fn: (...args: number[]) => void;".to_string(),
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
fn test_parser_parenthesized_type_still_works() {
    // Ensure parenthesized types still work after adding function type support
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x: (string | number);".to_string(),
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
fn test_parser_literal_type_string() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"let status: "success" | "error";"#.to_string(),
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
fn test_parser_literal_type_number() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let port: 80 | 443 | 8080;".to_string(),
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
fn test_parser_literal_type_boolean() {
    let mut parser = ParserState::new("test.ts".to_string(), "let flag: true;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_typeof_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let copy: typeof original;".to_string(),
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
fn test_parser_typeof_type_qualified() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let t: typeof console.log;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Generic Arrow Function Tests
// =========================================================================

