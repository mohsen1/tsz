#[test]
fn test_find_references_class_private_field() {
    let source = "class Foo {\n  #secret = 42;\n  get() { return this.#secret; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_async_function_name() {
    let source = "async function fetchData() {}\nawait fetchData();";
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
        assert!(r.len() >= 2, "Should find async function decl + call");
    }
}

#[test]
fn test_find_references_generator_function_name() {
    let source = "function* gen() { yield 1; }\nconst it = gen();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find generator decl + call");
    }
}

#[test]
fn test_find_references_type_parameter_in_function() {
    let source = "function identity<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 18));
    let _ = refs;
}

#[test]
fn test_find_references_type_parameter_in_class() {
    let source = "class Container<T> {\n  value: T;\n  get(): T { return this.value; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));
    let _ = refs;
}

#[test]
fn test_find_references_comma_operator() {
    let source = "let x = 0;\n(x++, x);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_logical_assignment() {
    let source = "let x: number | null = null;\nx ??= 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_in_arrow_return_expression() {
    let source = "const val = 10;\nconst fn = () => val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + arrow return usage");
    }
}

#[test]
fn test_find_references_in_object_spread() {
    let source = "const base = { a: 1 };\nconst ext = { ...base, b: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + object spread usage");
    }
}

#[test]
fn test_find_references_in_array_index() {
    let source = "const idx = 0;\nconst arr = [1, 2, 3];\narr[idx];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find idx decl + element access usage");
    }
}

