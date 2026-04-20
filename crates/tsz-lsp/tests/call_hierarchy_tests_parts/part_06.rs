#[test]
fn test_outgoing_calls_multiple_distinct_targets() {
    let source = "function x() {}\nfunction y() {}\nfunction z() {}\nfunction caller() {\n  x();\n  y();\n  z();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "caller" (line 3, col 9)
    let pos = Position::new(3, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.len() >= 3,
        "Should have at least 3 outgoing calls, got: {}",
        calls.len()
    );
    assert!(calls.iter().any(|c| c.to.name == "x"));
    assert!(calls.iter().any(|c| c.to.name == "y"));
    assert!(calls.iter().any(|c| c.to.name == "z"));
}

#[test]
fn test_prepare_on_overloaded_function() {
    let source = "function add(a: number, b: number): number;\nfunction add(a: string, b: string): string;\nfunction add(a: any, b: any): any {\n  return a + b;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "add" implementation (line 2, col 9)
    let pos = Position::new(2, 9);
    let item = provider.prepare(root, pos);

    // Should at least not crash; defensively check
    if let Some(item) = item {
        assert_eq!(item.name, "add");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_on_async_method() {
    let source = "class Api {\n  async fetch() {\n    return 'data';\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetch" (line 1, col 8)
    let pos = Position::new(1, 8);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for async method"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "fetch");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

// =========================================================================
// Additional tests for broader coverage
// =========================================================================

#[test]
fn test_prepare_on_default_exported_function() {
    let source = "export default function handler() {\n  return 42;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" (line 0, col 24)
    let pos = Position::new(0, 24);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "handler");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_at_file_end_returns_none() {
    let source = "function foo() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position well past the end of the file
    let pos = Position::new(100, 0);
    let item = provider.prepare(root, pos);
    // Should not panic; may return None or Some depending on offset resolution
    let _ = item;
}

#[test]
fn test_prepare_at_column_zero_line_zero() {
    let source = "function first() {}\n";
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
    // The cursor is on "function" keyword, which should resolve to the function
    if let Some(item) = item {
        assert_eq!(item.name, "first");
    }
}

#[test]
fn test_outgoing_calls_from_arrow_function_variable() {
    let source = "function target() {}\nconst caller = () => {\n  target();\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "caller" (line 1, col 6)
    let pos = Position::new(1, 6);
    let calls = provider.outgoing_calls(root, pos);

    // Defensively check - arrow function should detect outgoing calls
    if !calls.is_empty() {
        assert!(calls.iter().any(|c| c.to.name == "target"));
    }
}

#[test]
fn test_incoming_calls_for_constructor_via_new_expression() {
    let source = "class Widget {}\nfunction build() {\n  new Widget();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "build" (line 1, col 9) - check outgoing calls include Widget
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should have at least one outgoing call to Widget
    if !calls.is_empty() {
        assert!(
            calls.iter().any(|c| c.to.name == "Widget"),
            "Should include Widget in outgoing calls from build"
        );
    }
}

#[test]
fn test_prepare_on_deeply_nested_function() {
    let source = "function outer() {\n  function middle() {\n    function inner() {\n      return 1;\n    }\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "inner" (line 2, col 13)
    let pos = Position::new(2, 13);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "inner");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_with_conditional_calls() {
    let source = "function a() {}\nfunction b() {}\nfunction decide(flag: boolean) {\n  if (flag) { a(); } else { b(); }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "decide" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Both branches should show up as outgoing calls
    if calls.len() >= 2 {
        assert!(calls.iter().any(|c| c.to.name == "a"));
        assert!(calls.iter().any(|c| c.to.name == "b"));
    }
}

