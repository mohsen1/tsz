#[test]
fn test_selection_range_delete_expression() {
    let source = "delete obj.prop;";
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
    let (parser, _root) = parse_test_source(source);
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
