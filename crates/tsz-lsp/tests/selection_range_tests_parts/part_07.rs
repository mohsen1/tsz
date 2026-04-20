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

