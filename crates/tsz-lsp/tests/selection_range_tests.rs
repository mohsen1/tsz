use super::*;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_selection_range_simple_identifier() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' (column 4)
    let pos = Position::new(0, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for identifier"
    );
    let selection = result.unwrap();

    // Should have at least one parent (the identifier should expand to larger constructs)
    // The innermost range should cover 'x'
    assert!(selection.range.start.character <= 4);
    assert!(selection.range.end.character >= 5);
}

#[test]
fn test_selection_range_nested_expression() {
    let source = "foo.bar().baz";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'bar' (column 4)
    let pos = Position::new(0, 4);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection range");

    // Count the depth of the selection chain
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    // Should have multiple levels for nested member access
    assert!(
        depth >= 2,
        "Should have nested selection ranges, got {}",
        depth
    );
}

#[test]
fn test_selection_range_function_body() {
    let source = "function foo() {\n  return 1;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'return' (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in function body"
    );

    // Should eventually expand to include the whole function
    let mut current = result.as_ref();
    let mut found_function = false;
    while let Some(sel) = current {
        // Check if this range covers the whole function
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_function = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_function,
        "Selection should expand to include function"
    );
}

#[test]
fn test_selection_range_multiple_positions() {
    let source = "let a = 1;\nlet b = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    let positions = vec![Position::new(0, 4), Position::new(1, 4)];
    let results = provider.get_selection_ranges(&positions);

    assert_eq!(results.len(), 2);
    assert!(results[0].is_some(), "First position should have selection");
    assert!(
        results[1].is_some(),
        "Second position should have selection"
    );
}

#[test]
fn test_selection_range_no_node() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position in empty file
    let pos = Position::new(0, 0);
    let result = provider.get_selection_range(pos);

    // Should handle gracefully - may return None or a source file range
    // Just verify it doesn't panic
    let _ = result;
}

#[test]
fn test_selection_range_block_statement() {
    let source = "if (x) {\n  y = 1;\n  z = 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'y' inside the block
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in block");

    // Verify we can find a range that covers the block
    let mut current = result.as_ref();
    let mut found_block = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 3 {
            found_block = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_block, "Should expand to if statement block");
}

#[test]
fn test_selection_range_class_member() {
    let source = "class Foo {\n  bar() {\n    return 1;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'return' inside method
    let pos = Position::new(2, 4);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in class method");

    // Count depth
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    // Should have several levels: return -> statement -> block -> method -> class -> file
    assert!(
        depth >= 4,
        "Should have deep nesting in class, got {}",
        depth
    );
}
