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

