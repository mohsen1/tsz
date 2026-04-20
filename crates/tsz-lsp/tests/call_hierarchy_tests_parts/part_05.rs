#[test]
fn test_incoming_calls_multiple_callers() {
    let source = "function target() {}\nfunction callerA() {\n  target();\n}\nfunction callerB() {\n  target();\n}\nfunction callerC() {\n  target();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.len() >= 3,
        "Should have at least 3 incoming callers, got: {}",
        calls.len()
    );
    assert!(calls.iter().any(|c| c.from.name == "callerA"));
    assert!(calls.iter().any(|c| c.from.name == "callerB"));
    assert!(calls.iter().any(|c| c.from.name == "callerC"));
}

// ---- Additional call hierarchy tests ----

#[test]
fn test_prepare_on_exported_function() {
    let source = "export function greet(name: string) {\n  return `Hello ${name}`;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "greet" (line 0, col 16)
    let pos = Position::new(0, 16);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for exported function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "greet");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_not_on_enum() {
    let source = "enum Color { Red, Green, Blue }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Color" (line 0, col 5)
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for enum name"
    );
}

#[test]
fn test_prepare_on_namespace_function() {
    let source = "namespace NS {\n  export function helper() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" (line 1, col 19)
    let pos = Position::new(1, 19);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for namespace function"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "helper");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_empty_function() {
    let source = "function empty() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "empty" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.is_empty(),
        "Empty function should have no outgoing calls"
    );
}

#[test]
fn test_outgoing_calls_single_call() {
    let source = "function helper() {}\nfunction main() {\n  helper();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "main" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(calls.len(), 1, "Should have exactly one outgoing call");
    assert_eq!(calls[0].to.name, "helper");
}

#[test]
fn test_incoming_calls_from_method() {
    let source = "function target() {}\nclass Svc {\n  run() {\n    target();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.from.name == "run"),
        "Should find incoming call from method 'run', got: {calls:?}"
    );
}

#[test]
fn test_outgoing_calls_method_calling_function() {
    let source = "function doWork() {}\nclass Worker {\n  process() {\n    doWork();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "process" method (line 2, col 2)
    let pos = Position::new(2, 2);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "doWork"),
        "Method should have outgoing call to 'doWork', got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_abstract_method() {
    let source = "abstract class Base {\n  abstract compute(): number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "compute" (line 1, col 11)
    let pos = Position::new(1, 11);
    let item = provider.prepare(root, pos);

    // Abstract methods may or may not produce hierarchy items
    // This tests that it doesn't crash
    let _ = item;
}

#[test]
fn test_prepare_on_private_method() {
    let source = "class Foo {\n  private secret() {\n    return 42;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "secret" (line 1, col 10)
    let pos = Position::new(1, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for private method"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "secret");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

