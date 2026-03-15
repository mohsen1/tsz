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

#[test]
fn test_selection_range_interface_member() {
    let source = "interface Props {\n  name: string;\n  age: number;\n}";
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
        "Should find selection range for interface member"
    );

    // Should eventually expand to include the whole interface
    let mut current = result.as_ref();
    let mut found_interface = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 3 {
            found_interface = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_interface,
        "Selection should expand to include the interface"
    );
}

#[test]
fn test_selection_range_enum_member() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Green' member (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for enum member"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in enum, got {depth}"
    );
}

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

#[test]
fn test_selection_range_void_expression() {
    let source = "void doSomething();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    let pos = Position::new(0, 5);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for void expression"
    );
}

#[test]
fn test_selection_range_delete_expression() {
    let source = "delete obj.prop;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    let pos = Position::new(0, 7);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for delete expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for delete expression, got {depth}"
    );
}

#[test]
fn test_selection_range_new_expression() {
    let source = "const obj = new MyClass(1, 2);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'MyClass' (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for new expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for new expression, got {depth}"
    );
}

#[test]
fn test_selection_range_tagged_template() {
    let source = "const result = html`<div>${value}</div>`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for tagged template"
    );
}

#[test]
fn test_selection_range_class_method_with_body() {
    let source = "class Foo {\n  bar(x: number): string {\n    return x.toString();\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'toString' (line 2, column 13)
    let pos = Position::new(2, 13);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in class method body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting inside class method body, got {depth}"
    );
}

#[test]
fn test_selection_range_conditional_type() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'true' (column 38)
    let pos = Position::new(0, 38);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in conditional type"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for conditional type, got {depth}"
    );
}

#[test]
fn test_selection_range_intersection_type() {
    let source = "type Both = A & B & C;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'B' (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for intersection type member"
    );
}

#[test]
fn test_selection_range_union_type() {
    let source = "type Mixed = string | number | boolean;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'number' (column 22)
    let pos = Position::new(0, 22);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for union type member"
    );
}

#[test]
fn test_selection_range_mapped_type() {
    let source = "type Readonly<T> = { readonly [K in keyof T]: T[K] };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'keyof' (column 36)
    let pos = Position::new(0, 36);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in mapped type"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have deep nesting for mapped type, got {depth}"
    );
}

#[test]
fn test_selection_range_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    let pos = Position::new(0, 0);
    let result = provider.get_selection_range(pos);

    // Should not panic on empty source
    let _ = result;
}

#[test]
fn test_selection_range_assignment_expression() {
    let source = "let x: number;\nx = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at '42' (line 1, column 4)
    let pos = Position::new(1, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in assignment expression"
    );
}

#[test]
fn test_selection_range_nested_if_else() {
    let source = "if (a) {\n  if (b) {\n    doStuff();\n  } else {\n    doOther();\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'doOther' in inner else (line 4, column 4)
    let pos = Position::new(4, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in nested if-else"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting for nested if-else, got {depth}"
    );
}

#[test]
fn test_selection_range_arrow_with_expression_body() {
    let source = "const double = (x: number) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' in expression body (column 30)
    let pos = Position::new(0, 30);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in arrow function expression body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for arrow body, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_object_literal() {
    let source = "const config = {\n  host: 'localhost',\n  port: 8080,\n  debug: true\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'port' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for property in multiline object"
    );

    // Should eventually expand to include the whole object
    let mut current = result.as_ref();
    let mut found_object = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 4 {
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
fn test_selection_range_tuple_type() {
    let source = "let pair: [string, number] = ['a', 1];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'number' in tuple type (column 19)
    let pos = Position::new(0, 19);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for tuple type element"
    );
}

#[test]
fn test_selection_range_satisfies_expression() {
    let source = "const x = { a: 1 } satisfies Record<string, number>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Record' (column 30)
    let pos = Position::new(0, 30);
    let result = provider.get_selection_range(pos);

    // Should not panic; parser may or may not support `satisfies`
    let _ = result;
}

// =========================================================================
// Additional selection range tests to reach 80+ (batch 3)
// =========================================================================

#[test]
fn test_selection_range_enum_member_expand() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Green' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for enum member"
    );

    let mut current = result.as_ref();
    let mut found_enum = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 4 {
            found_enum = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_enum, "Selection should expand to include entire enum");
}

#[test]
fn test_selection_range_namespace_body() {
    let source = "namespace App {\n  export const x = 1;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' (line 1, column 16)
    let pos = Position::new(1, 16);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range inside namespace"
    );

    let mut current = result.as_ref();
    let mut found_ns = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_ns = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_ns, "Selection should expand to include namespace");
}

#[test]
fn test_selection_range_interface_body() {
    let source = "interface Config {\n  host: string;\n  port: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'port' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for interface property"
    );

    let mut current = result.as_ref();
    let mut found_iface = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 3 {
            found_iface = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_iface, "Selection should expand to include interface");
}

#[test]
fn test_selection_range_class_static_method() {
    let source = "class Util {\n  static create() {\n    return new Util();\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Util' in new expression (line 2, column 15)
    let pos = Position::new(2, 15);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for static method body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting inside class static method, got {depth}"
    );
}

#[test]
fn test_selection_range_template_literal_expression() {
    let source = "const name = 'world';\nconst msg = `hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position inside the template literal (line 1, column 20)
    let pos = Position::new(1, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in template literal"
    );
}

#[test]
fn test_selection_range_class_extends() {
    let source = "class Base {}\nclass Child extends Base {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'method' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in child class"
    );

    let mut current = result.as_ref();
    let mut found_child = false;
    while let Some(sel) = current {
        if sel.range.start.line == 1 && sel.range.end.line == 3 {
            found_child = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_child,
        "Selection should expand to include Child class"
    );
}

#[test]
fn test_selection_range_deeply_nested_blocks() {
    let source = "function f() {\n  if (a) {\n    if (b) {\n      if (c) {\n        doIt();\n      }\n    }\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'doIt' (line 4, column 8)
    let pos = Position::new(4, 8);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in deeply nested blocks"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 5,
        "Should have deep nesting for nested if blocks, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_interface() {
    let source = "interface API {\n  get(url: string): void;\n  post(url: string, body: any): void;\n  put(url: string, body: any): void;\n  del(url: string): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'post' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for interface method"
    );

    let mut current = result.as_ref();
    let mut found_iface = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_iface = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_iface,
        "Selection should expand to include entire interface"
    );
}

#[test]
fn test_selection_range_switch_case_body() {
    let source = "switch (x) {\n  case 1:\n    console.log('one');\n    break;\n  default:\n    console.log('other');\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'one' in case body (line 2, column 17)
    let pos = Position::new(2, 17);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection range in case body");

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in switch case, got {depth}"
    );
}

#[test]
fn test_selection_range_class_with_multiple_members() {
    let source = "class Foo {\n  a: number;\n  b: string;\n  c(): void {}\n  d(): boolean { return true; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'c' method (line 3, column 2)
    let pos = Position::new(3, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for class method"
    );

    let mut current = result.as_ref();
    let mut found_class = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_class = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_class,
        "Selection should expand to include entire class"
    );
}

#[test]
fn test_selection_range_object_method_shorthand() {
    let source = "const obj = {\n  greet() {\n    return 'hi';\n  }\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'hi' (line 2, column 12)
    let pos = Position::new(2, 12);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in object method"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in object method, got {depth}"
    );
}

#[test]
fn test_selection_range_arrow_with_block_body() {
    let source = "const fn = (x: number) => {\n  const y = x * 2;\n  return y;\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'y' in return statement (line 2, column 9)
    let pos = Position::new(2, 9);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in arrow block body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in arrow block body, got {depth}"
    );
}

#[test]
fn test_selection_range_boolean_expression() {
    let source = "const result = a && b || c && d;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' (column 20)
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in boolean expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in boolean expression, got {depth}"
    );
}

#[test]
fn test_selection_range_type_assertion_angle_bracket() {
    let source = "const x = <string>'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'hello' (column 19)
    let pos = Position::new(0, 19);
    let result = provider.get_selection_range(pos);

    // Should not panic
    let _ = result;
}

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
