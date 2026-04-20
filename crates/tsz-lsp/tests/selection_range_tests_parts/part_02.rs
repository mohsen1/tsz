#[test]
fn test_selection_range_while_loop() {
    let source = "while (true) {\n  doWork();\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'doWork' inside while body (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in while loop body"
    );

    let mut current = result.as_ref();
    let mut found_while = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_while = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_while, "Selection should expand to include while loop");
}

#[test]
fn test_selection_range_single_line_simple() {
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
fn test_selection_range_do_while() {
    let source = "do {\n  process();\n} while (cond);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'process' inside do body (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in do-while body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in do-while, got {depth}"
    );
}

#[test]
fn test_selection_range_nested_function_calls() {
    let source = "foo(bar(baz(1)));";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '1' deep inside nested calls (column 11)
    let pos = Position::new(0, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in nested function calls"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for nested calls, got {depth}"
    );
}

#[test]
fn test_selection_range_parenthesized_expression() {
    let source = "const result = (a + b) * c;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'a' inside parens (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in parenthesized expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection for parenthesized expr, got {depth}"
    );
}

#[test]
fn test_selection_range_innermost_first() {
    let source = "function outer() {\n  function inner() {\n    return 42;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '42' inside inner function (line 2, column 11)
    let pos = Position::new(2, 11);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection range");

    // The innermost range should be smaller than or equal to the parent
    let sel = result.as_ref().unwrap();
    if let Some(parent) = &sel.parent {
        let inner_size = (sel.range.end.line as i64 - sel.range.start.line as i64).unsigned_abs()
            + (sel.range.end.character as i64 - sel.range.start.character as i64).unsigned_abs();
        let parent_size = (parent.range.end.line as i64 - parent.range.start.line as i64)
            .unsigned_abs()
            + (parent.range.end.character as i64 - parent.range.start.character as i64)
                .unsigned_abs();
        assert!(
            inner_size <= parent_size || sel.range.start.line >= parent.range.start.line,
            "Inner range should be contained within or equal to parent"
        );
    }
}

// =========================================================================
// Additional selection range tests
// =========================================================================

#[test]
fn test_selection_range_generic_type_annotation() {
    let source = "let x: Array<Map<string, number>> = [];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'string' inside the generic (column 18)
    let pos = Position::new(0, 18);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for generic type parameter"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for nested generics, got {depth}"
    );
}

#[test]
fn test_selection_range_for_of_loop() {
    let source = "for (const item of items) {\n  process(item);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'process' inside for-of body (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in for-of loop body"
    );

    let mut current = result.as_ref();
    let mut found_for_of = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_for_of = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_for_of,
        "Selection should expand to include for-of loop"
    );
}

#[test]
fn test_selection_range_type_alias() {
    let source = "type Pair<T> = [T, T];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'T' in the type parameter (column 10)
    let pos = Position::new(0, 10);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for type alias parameter"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in type alias, got {depth}"
    );
}

#[test]
fn test_selection_range_spread_operator() {
    let source = "const merged = { ...a, ...b };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'a' after spread (column 20)
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for spread element"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for spread, got {depth}"
    );
}

