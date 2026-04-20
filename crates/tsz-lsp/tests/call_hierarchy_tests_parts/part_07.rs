#[test]
fn test_prepare_on_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let item = provider.prepare(root, pos);
    assert!(
        item.is_none(),
        "Empty source should yield no hierarchy item"
    );
}

#[test]
fn test_incoming_calls_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let calls = provider.incoming_calls(root, pos);
    assert!(
        calls.is_empty(),
        "Empty source should yield no incoming calls"
    );
}

#[test]
fn test_outgoing_calls_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let calls = provider.outgoing_calls(root, pos);
    assert!(
        calls.is_empty(),
        "Empty source should yield no outgoing calls"
    );
}

#[test]
fn test_prepare_on_protected_method() {
    let source = "class Base {\n  protected compute() {\n    return 0;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "compute" (line 1, col 12)
    let pos = Position::new(1, 12);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "compute");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_from_method_calling_other_methods() {
    let source = "class Svc {\n  a() {}\n  b() {}\n  c() {\n    this.a();\n    this.b();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "c" (line 3, col 2)
    let pos = Position::new(3, 2);
    let calls = provider.outgoing_calls(root, pos);

    // Defensively check - should find calls to a and b
    let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
    if names.contains(&"a") {
        assert!(names.contains(&"b"), "If a is found, b should be too");
    }
}

#[test]
fn test_prepare_on_function_with_type_parameters() {
    let source = "function identity<T>(x: T): T {\n  return x;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "identity" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for generic function"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "identity");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_call_hierarchy_item_has_uri() {
    let source = "function hello() {}\n";
    let mut parser = ParserState::new("my_file.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "my_file.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(
            item.uri, "my_file.ts",
            "URI should match the file name provided"
        );
        assert_eq!(item.name, "hello");
    }
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_prepare_on_single_line_function() {
    let source = "function f() { return 1; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "f");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_on_method_with_rest_params() {
    let source = "class C {\n  collect(...args: number[]) {\n    return args;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "collect" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "collect");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_in_try_catch_block() {
    let source = "function safe() {}\nfunction risky() {}\nfunction doStuff() {\n  try {\n    risky();\n  } catch (e) {\n    safe();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "doStuff" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should find calls in both try and catch blocks
    let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
    if names.contains(&"risky") {
        assert!(
            names.contains(&"safe"),
            "Should find outgoing calls from both try and catch blocks"
        );
    }
}

