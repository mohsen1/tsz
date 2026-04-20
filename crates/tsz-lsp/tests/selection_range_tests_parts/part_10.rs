#[test]
fn test_selection_range_throw_statement() {
    let source = "function fail() {\n  throw new Error('oops');\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'Error' (line 1, column 12)
    let pos = Position::new(1, 12);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in throw statement");
}

#[test]
fn test_selection_range_regex_literal() {
    let source = "const pattern = /hello\\s+world/gi;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_multiple_variable_declarations() {
    let source = "let a = 1, b = 2, c = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'b' (column 11)
    let pos = Position::new(0, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for middle variable declaration"
    );
}
