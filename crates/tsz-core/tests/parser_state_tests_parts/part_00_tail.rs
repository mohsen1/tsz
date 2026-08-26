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

#[test]
fn test_parser_generic_arrow_simple() {
    // Basic generic arrow function: <T>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const identity = <T>(x: T) => x;".to_string(),
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
fn test_parser_generic_arrow_tsx_trailing_comma() {
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const id = <T,>(x: T): T => x;".to_string(),
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
fn test_parser_generic_arrow_multiple_params() {
    // Multiple type parameters: <T, U>(x: T, y: U) => [T, U]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const pair = <T, U>(x: T, y: U) => [x, y];".to_string(),
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
fn test_parser_generic_arrow_with_constraint() {
    // Type parameter with constraint: <T extends object>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const clone = <T extends object>(x: T) => x;".to_string(),
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
fn test_parser_generic_arrow_with_default() {
    // Type parameter with default: <T = string>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const wrap = <T = string>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}
