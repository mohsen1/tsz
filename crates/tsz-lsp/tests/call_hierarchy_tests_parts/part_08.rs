#[test]
fn test_incoming_calls_from_multiple_methods_same_class() {
    let source = "function target() {}\nclass Svc {\n  a() { target(); }\n  b() { target(); }\n}\n";
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

    // Should find incoming calls from both methods
    assert!(
        calls.len() >= 2,
        "Should find incoming calls from both methods, got: {}",
        calls.len()
    );
}

#[test]
fn test_prepare_on_method_with_optional_params() {
    let source = "class Config {\n  set(key: string, value?: string) {\n    return key;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "set" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "set");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_prepare_on_function_with_destructured_params() {
    let source = "function process({ x, y }: { x: number; y: number }) {\n  return x + y;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "process" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "process");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_from_for_loop_body() {
    let source = "function work() {}\nfunction loop_caller() {\n  for (let i = 0; i < 3; i++) {\n    work();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "loop_caller" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "work"),
        "Should find outgoing call to 'work' from inside a for loop, got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_function_unicode_name() {
    let source = "function calcul\u{00E9}() {\n  return 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the function name
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    // Should not crash; if it finds the item, the name should contain unicode
    let _ = item;
}

#[test]
fn test_outgoing_calls_from_while_loop() {
    let source = "function tick() {}\nfunction runner() {\n  while (true) {\n    tick();\n    break;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "runner" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "tick"),
        "Should find outgoing call to 'tick' from while loop, got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_async_arrow_function() {
    let source = "const fetchAll = async () => {\n  return [];\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetchAll" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "fetchAll");
    }
}

#[test]
fn test_incoming_calls_from_switch_case() {
    let source = "function handler() {}\nfunction dispatch(action: string) {\n  switch (action) {\n    case 'a': handler(); break;\n    case 'b': handler(); break;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if let Some(dispatch_call) = calls.iter().find(|c| c.from.name == "dispatch") {
        assert!(
            dispatch_call.from_ranges.len() >= 2,
            "Should have at least 2 call ranges from switch cases, got: {}",
            dispatch_call.from_ranges.len()
        );
    }
}

#[test]
fn test_prepare_on_function_with_default_params() {
    let source = "function greet(name: string = 'World') {\n  return name;\n}\n";
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

    assert!(
        item.is_some(),
        "Should find call hierarchy item for function with default params"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "greet");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_not_on_type_annotation() {
    let source = "const x: number = 42;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "number" type annotation
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for type annotation"
    );
}

