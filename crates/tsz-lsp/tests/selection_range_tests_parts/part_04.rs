#[test]
fn test_selection_range_string_literal() {
    let source = "const s = \"hello world\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position inside the string (column 12)
    let pos = Position::new(0, 12);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for string literal"
    );
}

#[test]
fn test_selection_range_for_in_loop() {
    let source = "for (const key in obj) {\n  use(key);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'use' inside for-in body (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in for-in loop body"
    );

    let mut current = result.as_ref();
    let mut found_for_in = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_for_in = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_for_in,
        "Selection should expand to include for-in loop"
    );
}

#[test]
fn test_selection_range_null_coalescing() {
    let source = "const val = a ?? b ?? c;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' (column 17)
    let pos = Position::new(0, 17);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for nullish coalescing"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for nullish coalescing, got {depth}"
    );
}

#[test]
fn test_selection_range_import_statement() {
    let source = "import { foo, bar } from './module';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'foo' (column 9)
    let pos = Position::new(0, 9);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in import statement"
    );
}

#[test]
fn test_selection_range_export_statement() {
    let source = "export { foo, bar };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'bar' (column 14)
    let pos = Position::new(0, 14);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in export statement"
    );
}

#[test]
fn test_selection_range_nested_ternary() {
    let source = "const x = a ? b ? 1 : 2 : 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '1' inside nested ternary (column 18)
    let pos = Position::new(0, 18);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in nested ternary"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for nested ternary, got {depth}"
    );
}

#[test]
fn test_selection_range_class_constructor() {
    let source = "class Foo {\n  constructor(private x: number) {\n    this.x = x;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'this' inside constructor (line 2, column 4)
    let pos = Position::new(2, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in constructor body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting inside constructor, got {depth}"
    );
}

#[test]
fn test_selection_range_comma_separated_params() {
    let source = "function f(a: number, b: string, c: boolean) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' parameter (column 22)
    let pos = Position::new(0, 22);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for middle parameter"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested ranges for parameter, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_array_literal() {
    let source = "const arr = [\n  1,\n  2,\n  3,\n  4\n];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '3' (line 3, column 2)
    let pos = Position::new(3, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for element in multiline array"
    );

    // Should eventually expand to include the whole array
    let mut current = result.as_ref();
    let mut found_array = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_array = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_array,
        "Selection should expand to include entire array literal"
    );
}

#[test]
fn test_selection_range_typeof_expression() {
    let source = "const t = typeof someVar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'someVar' (column 17)
    let pos = Position::new(0, 17);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for typeof expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for typeof, got {depth}"
    );
}

// =========================================================================
// Additional selection range tests (batch 2)
// =========================================================================

