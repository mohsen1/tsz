#[test]
fn test_selection_range_object_destructuring() {
    let source = "const { a, b, c } = obj;";
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
        "Should find selection range in object destructuring"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for destructuring, got {depth}"
    );
}

#[test]
fn test_selection_range_as_expression() {
    let source = "const x = someValue as string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'someValue' (column 10)
    let pos = Position::new(0, 10);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for as expression"
    );
}

// =========================================================================
// Additional selection range tests (batch 4 — edge cases)
// =========================================================================

#[test]
fn test_selection_range_single_identifier_file() {
    let source = "x";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 0);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for single identifier"
    );
}

#[test]
fn test_selection_range_whitespace_before_code() {
    let source = "  \n  \nconst x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(2, 6);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range after leading whitespace"
    );
}

#[test]
fn test_selection_range_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 6);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_nested_arrow_functions() {
    let source = "const f = (x: number) => (y: number) => x + y;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'y' in x + y (column 45)
    let pos = Position::new(0, 45);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in nested arrow");
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(
        depth >= 3,
        "Nested arrows should produce deep chain, got {depth}"
    );
}

#[test]
fn test_selection_range_async_arrow_function() {
    let source = "const f = async () => { return 1; };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 31);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in async arrow body"
    );
}

#[test]
fn test_selection_range_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'yield' (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in generator body");
}

#[test]
fn test_selection_range_class_private_field() {
    let source = "class Foo {\n  #bar = 42;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for private field");
}

#[test]
fn test_selection_range_class_accessor() {
    let source = "class Foo {\n  get val() { return 1; }\n  set val(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'val' in getter (line 1, column 6)
    let pos = Position::new(1, 6);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for getter");
}

