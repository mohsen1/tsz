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

