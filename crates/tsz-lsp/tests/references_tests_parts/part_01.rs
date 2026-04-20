#[test]
fn test_find_references_parameter_binding_pattern() {
    let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage in the return (line 1)
    let position = Position::new(1, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for parameter binding name"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find parameter declaration and usage"
    );
}

#[test]
fn test_find_references_parameter_array_binding() {
    let source = "function demo([foo]: number[]) {\n  return foo;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage in the return (line 1)
    let position = Position::new(1, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for array binding name"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find parameter declaration and usage"
    );
}

#[test]
fn test_find_references_nested_arrow_in_switch_case() {
    let source = "switch (state) {\n  case (() => {\n    const value = 1;\n    return value;\n  })():\n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for switch case locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_nested_arrow_in_if_condition() {
    let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for nested arrow locals in condition"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_export_default_expression() {
    let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for export default expression locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_labeled_statement_local() {
    let source = "label: {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for labeled statement locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_with_statement_local() {
    let source = "with (obj) {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for with statement locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_var_hoisted_in_nested_block() {
    let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage before the declaration (line 1)
    let position = Position::new(1, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for hoisted var"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_decorator_reference() {
    let source = "const deco = () => {};\n@deco\nclass Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'deco' usage in the decorator (line 1)
    let position = Position::new(1, 1);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for decorator usage"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_method_local() {
    let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for method local"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

