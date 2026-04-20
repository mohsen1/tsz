#[test]
fn test_selection_range_const_enum() {
    let source = "const enum Dir {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for const enum member"
    );
    let mut current = result.as_ref();
    let mut found_enum = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_enum = true;
            break;
        }
        current = sel.parent.as_deref();
    }
    assert!(found_enum, "Selection should expand to entire const enum");
}

#[test]
fn test_selection_range_index_type_query() {
    let source = "type Keys = keyof { a: 1; b: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 12);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for keyof expression"
    );
}

#[test]
fn test_selection_range_infer_type() {
    let source = "type Ret<T> = T extends (...args: any) => infer R ? R : never;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'infer' (column 42)
    let pos = Position::new(0, 42);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_nested_ternary_deep() {
    let source = "const x = a ? b : c ? d : e ? f : g;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'f' (column 34 area)
    let pos = Position::new(0, 34);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in deeply nested ternary"
    );
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(depth >= 2, "Should have nested selections, got {depth}");
}

#[test]
fn test_selection_range_rest_parameter() {
    let source = "function sum(...nums: number[]) { return 0; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'nums' (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for rest parameter");
}

#[test]
fn test_selection_range_optional_parameter() {
    let source = "function greet(name?: string) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 15);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for optional parameter"
    );
}

#[test]
fn test_selection_range_abstract_class_method() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(1, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for abstract method"
    );
}

#[test]
fn test_selection_range_multiline_chain() {
    let source = "promise\n  .then(x => x)\n  .catch(e => e)\n  .finally(() => {});";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'catch' (line 2, column 3)
    let pos = Position::new(2, 3);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in chained call");
}

#[test]
fn test_selection_range_array_destructuring_with_rest() {
    let source = "const [first, ...rest] = [1, 2, 3];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'rest' (column 17)
    let pos = Position::new(0, 17);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for rest in array destructuring"
    );
}

#[test]
fn test_selection_range_nested_object_type() {
    let source = "type Config = {\n  db: {\n    host: string;\n    port: number;\n  };\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'host' (line 2, column 4)
    let pos = Position::new(2, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in nested object type"
    );
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(
        depth >= 3,
        "Should have deep nesting for nested type, got {depth}"
    );
}

