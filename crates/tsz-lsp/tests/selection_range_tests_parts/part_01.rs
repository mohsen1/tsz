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

