#[test]
fn test_find_references_namespace_name() {
    let source = "namespace Utils {\n  export function helper() {}\n}\nUtils.helper();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Utils' usage (line 3, col 0)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(3, 0));

    assert!(refs.is_some(), "Should find references for namespace name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find namespace declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_empty_file_returns_none() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 0));

    assert!(refs.is_none(), "Empty file should return None");
}

#[test]
fn test_find_references_for_loop_counter() {
    let source = "for (let i = 0; i < 5; i++) {\n  console.log(i);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'i' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for for-loop counter"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + condition + increment + body usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_arrow_function_param() {
    let source = "const double = (n: number) => n * 2;\ndouble(3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'n' parameter (col 16)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));

    assert!(
        refs.is_some(),
        "Should find references for arrow function param"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find param declaration + usage in body, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_nested_function_scoping() {
    let source = "function outer() {\n  const x = 1;\n  function inner() {\n    const x = 2;\n    x;\n  }\n  x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'x' in outer scope (line 1, col 8)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 8));

    assert!(refs.is_some(), "Should find references for outer x");
    let refs = refs.unwrap();
    // Outer x should have declaration + usage on line 6, but NOT include inner x
    assert!(
        refs.len() >= 2,
        "Should find outer x declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_type_alias_in_multiple_annotations() {
    let source = "type ID = string;\nlet a: ID;\nlet b: ID;\nfunction process(id: ID) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'ID' declaration (line 0, col 5)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 4,
        "Should find type alias decl + 3 type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_const_enum_name() {
    let source = "const enum Fruit { Apple, Banana }\nlet f: Fruit = Fruit.Apple;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Fruit' declaration (line 0, col 11)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 11));

    assert!(refs.is_some(), "Should find references for const enum");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find const enum declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_function_used_as_callback() {
    let source = "function handler() {}\nconst arr = [1, 2];\narr.forEach(handler);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'handler' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for function used as callback"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find function declaration + callback usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_default_parameter() {
    let source = "function greet(name = 'world') { return name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 15));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find name param + usage");
    }
}

#[test]
fn test_find_references_computed_property_name() {
    let source = "const key = 'x';\nconst obj = { [key]: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find key decl + computed usage");
    }
}

