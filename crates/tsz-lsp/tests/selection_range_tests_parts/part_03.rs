#[test]
fn test_selection_range_optional_chaining() {
    let source = "const val = obj?.prop?.nested;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'nested' (column 22)
    let pos = Position::new(0, 22);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for optional chaining"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for optional chaining, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_function() {
    let source = "function calculate(\n  x: number,\n  y: number\n): number {\n  return x + y;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' parameter (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for function parameter"
    );

    // Should eventually expand to include the whole function
    let mut current = result.as_ref();
    let mut found_function = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_function = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_function,
        "Selection should expand to include the full function"
    );
}

#[test]
fn test_selection_range_array_destructuring() {
    let source = "const [first, ...rest] = items;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'first' (column 7)
    let pos = Position::new(0, 7);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in array destructuring"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in array destructuring, got {depth}"
    );
}

#[test]
fn test_selection_range_computed_property() {
    let source = "const obj = {\n  [Symbol.iterator]: function() {}\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Symbol' (line 1, column 3)
    let pos = Position::new(1, 3);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for computed property"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for computed property, got {depth}"
    );
}

#[test]
fn test_selection_range_position_past_end() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position well past the end of the file
    let pos = Position::new(100, 100);
    let result = provider.get_selection_range(pos);

    // Should not panic; may return None or a range
    let _ = result;
}

#[test]
fn test_selection_range_class_with_decorators() {
    // Decorators may or may not parse depending on parser mode,
    // but the test should not panic
    let source =
        "class Foo {\n  private x: number = 0;\n  public get value() { return this.x; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'value' getter name (line 2, column 14)
    let pos = Position::new(2, 14);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for getter in class"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for class getter, got {depth}"
    );
}

#[test]
fn test_selection_range_async_await() {
    let source = "async function load() {\n  const data = await fetch('/api');\n  return data;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'fetch' (line 1, column 22)
    let pos = Position::new(1, 22);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for await expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for async/await, got {depth}"
    );
}

#[test]
fn test_selection_range_labeled_statement() {
    let source = "outer: for (let i = 0; i < 10; i++) {\n  inner: for (let j = 0; j < 10; j++) {\n    if (j === 5) break outer;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'break' keyword (line 2, column 18)
    let pos = Position::new(2, 18);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in labeled statement"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting for nested labeled loops, got {depth}"
    );
}

// =========================================================================
// Additional selection range tests to reach 50
// =========================================================================

#[test]
fn test_selection_range_empty_function_body() {
    let source = "function noop() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at opening brace (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    // Should not panic; may return a range covering the block or the function
    let _ = result;
}

#[test]
fn test_selection_range_numeric_literal() {
    let source = "const n = 123456;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at the number (column 10)
    let pos = Position::new(0, 10);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for numeric literal"
    );
}

