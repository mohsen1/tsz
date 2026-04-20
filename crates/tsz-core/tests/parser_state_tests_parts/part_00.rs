#[test]
fn test_parser_simple_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "1 + 2".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(!parser.arena.is_empty());

    // Should have: SourceFile, ExpressionStatement, BinaryExpression, 2 NumericLiterals
    assert!(
        parser.arena.len() >= 5,
        "Expected at least 5 nodes, got {}",
        parser.arena.len()
    );
}

#[test]
fn test_parser_reset_clears_arena() {
    let mut parser = ParserState::new("test.ts".to_string(), "const a = 1;".to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    assert!(
        arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "a"),
        "Expected identifier 'a' after first parse"
    );

    parser.reset("test.ts".to_string(), "const b = 2;".to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    assert!(
        arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "b"),
        "Expected identifier 'b' after reset parse"
    );
    assert!(
        !arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "a"),
        "Did not expect identifier 'a' after reset parse"
    );
}

#[test]
fn test_parser_numeric_separator_invalid_diagnostic() {
    let source = "let x = 1_;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE)
        .unwrap_or_else(|| panic!("Expected numeric separator diagnostic, got: {diagnostics:?}"));
    let underscore_pos = source.find('_').expect("underscore not found") as u32;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_parser_numeric_separator_consecutive_diagnostic() {
    let source = "let x = 1__0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED
        })
        .unwrap_or_else(|| {
            panic!("Expected consecutive separator diagnostic, got: {diagnostics:?}")
        });
    let underscore_pos = source.find("__").expect("double underscore not found") as u32 + 1;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_parser_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function add(a, b) { return a + b; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_variable_declaration() {
    let mut parser = ParserState::new("test.ts".to_string(), "let x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_if_statement() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "if (x > 0) { return x; } else { return -x; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_while_loop() {
    let mut parser = ParserState::new("test.ts".to_string(), "while (x < 10) { x++; }".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_for_loop() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "for (let i = 0; i < 10; i++) { console.log(i); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_object_literal() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let obj = { a: 1, b: 2 };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

