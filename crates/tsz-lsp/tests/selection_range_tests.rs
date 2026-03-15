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
        "Should have nested selection ranges, got {depth}"
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
    assert!(depth >= 4, "Should have deep nesting in class, got {depth}");
}

#[test]
fn test_selection_range_arrow_function_body() {
    let source = "const add = (a: number, b: number) => a + b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '+' operator in arrow body (column 40)
    let pos = Position::new(0, 40);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in arrow function body"
    );

    // Should have multiple levels: operator -> binary expr -> arrow body -> arrow -> variable decl -> file
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in arrow function, got {depth}"
    );
}

#[test]
fn test_selection_range_object_literal_property() {
    let source = "const obj = {\n  name: \"hello\",\n  age: 42\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'name' property (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for object property"
    );

    // Should eventually expand to include the whole object literal
    let mut current = result.as_ref();
    let mut found_object = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 3 {
            found_object = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_object,
        "Selection should expand to include entire object literal"
    );
}

#[test]
fn test_selection_range_template_literal() {
    let source = "const msg = `hello ${name} world`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position inside template expression at 'name' (column 21)
    let pos = Position::new(0, 21);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in template literal"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested ranges in template literal, got {depth}"
    );
}

#[test]
fn test_selection_range_ternary_expression() {
    let source = "const val = x > 0 ? \"positive\" : \"negative\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at "positive" string (column 20)
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in ternary expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested ranges for ternary, got {depth}"
    );
}

#[test]
fn test_selection_range_switch_case() {
    let source = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'break' inside case 1 (line 2, column 4)
    let pos = Position::new(2, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in switch case"
    );

    // Should eventually expand to include the whole switch statement
    let mut current = result.as_ref();
    let mut found_switch = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_switch = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_switch,
        "Selection should expand to include switch statement"
    );
}

#[test]
fn test_selection_range_try_catch_finally() {
    let source = "try {\n  foo();\n} catch (e) {\n  bar();\n} finally {\n  baz();\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'bar' inside catch block (line 3, column 2)
    let pos = Position::new(3, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in catch block"
    );

    // Should eventually expand to include the whole try/catch/finally
    let mut current = result.as_ref();
    let mut found_try = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 6 {
            found_try = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_try,
        "Selection should expand to include try/catch/finally"
    );
}

#[test]
fn test_selection_range_array_with_nested_objects() {
    let source = "const arr = [\n  { x: 1 },\n  { x: 2 }\n];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' inside first object (line 1, column 4)
    let pos = Position::new(1, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in nested object within array"
    );

    // Should have deep nesting: property name -> property -> object -> array element -> array -> decl -> file
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting for array with nested objects, got {depth}"
    );
}

#[test]
fn test_selection_range_destructuring_assignment() {
    let source = "const { a, b: c } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' in destructuring (column 11)
    let pos = Position::new(0, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in destructuring"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in destructuring, got {depth}"
    );
}

#[test]
fn test_selection_range_type_assertion() {
    let source = "const val = expr as string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'string' keyword (column 20)
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for type assertion"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for type assertion, got {depth}"
    );
}

#[test]
fn test_selection_range_for_loop() {
    let source = "for (let i = 0; i < 10; i++) {\n  console.log(i);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'console' inside for body (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in for loop body"
    );

    // Should eventually expand to include the whole for loop
    let mut current = result.as_ref();
    let mut found_for = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_for = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_for, "Selection should expand to include for loop");
}

#[test]
fn test_selection_range_multiline_string_concatenation() {
    let source = "const s = \"hello\" +\n  \"world\" +\n  \"foo\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at "world" string (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in multiline expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in multiline expression, got {depth}"
    );
}
