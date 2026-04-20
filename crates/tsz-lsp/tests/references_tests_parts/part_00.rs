#[test]
fn test_find_references_simple() {
    // const x = 1;
    // x + x;
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the first 'x' in "x + x" (line 1, column 0)
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(references.is_some(), "Should find references for x");

    if let Some(refs) = references {
        // Should find at least the declaration and two usages
        assert!(
            refs.len() >= 2,
            "Should find at least 2 references (declaration + usages)"
        );
    }
}

#[test]
fn test_find_references_for_symbol() {
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let symbol_id = binder.file_locals.get("x").expect("Expected symbol for x");

    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references_for_symbol(root, symbol_id);

    assert!(references.is_some(), "Should find references for x");
    if let Some(refs) = references {
        assert!(
            refs.len() >= 2,
            "Should find at least 2 references (declaration + usages)"
        );
    }
}

#[test]
fn test_find_references_not_found() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position outside any identifier
    let position = Position::new(0, 11); // At the semicolon

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    // Should not find references
    assert!(
        references.is_none(),
        "Should not find references at semicolon"
    );
}

#[test]
fn test_find_references_template_expression() {
    let source = "const name = \"Ada\";\nconst msg = `hi ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'name' inside the template expression (line 1)
    let position = Position::new(1, 18);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in template expression"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and template usage"
    );
}

#[test]
fn test_find_references_jsx_expression() {
    let source = "const name = \"Ada\";\nconst el = <div>{name}</div>;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'name' inside JSX expression (line 1)
    let position = Position::new(1, 17);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.tsx".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in JSX expression"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and JSX usage");
}

#[test]
fn test_find_references_await_expression() {
    let source = "const value = 1;\nasync function run() {\n  await value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside await (line 2)
    let position = Position::new(2, 8);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in await expression"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and await usage");
}

#[test]
fn test_find_references_tagged_template_expression() {
    let source =
        "const tag = (strings: TemplateStringsArray) => strings[0];\nconst msg = tag`hello`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'tag' inside tagged template (line 1)
    let position = Position::new(1, 16);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in tagged template"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and tagged template usage"
    );
}

#[test]
fn test_find_references_as_expression() {
    let source = "const value = 1;\nconst result = value as number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside the as-expression (line 1)
    let position = Position::new(1, 15);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in as expression"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and as-expression usage"
    );
}

#[test]
fn test_find_references_binding_pattern() {
    let source = "const { foo } = obj;\nfoo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage (line 1)
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for binding pattern name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_binding_pattern_initializer() {
    let source = "const value = 1;\nconst { foo = value } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside the initializer (line 1)
    let position = Position::new(1, 14);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in binding pattern initializer"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and initializer usage"
    );
}

