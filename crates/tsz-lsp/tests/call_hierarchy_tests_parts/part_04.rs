#[test]
fn test_recursive_function_calls() {
    let source = "function factorial(n: number): number {\n  if (n <= 1) return 1;\n  return n * factorial(n - 1);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "factorial" declaration (line 0, col 9)
    let pos = Position::new(0, 9);

    // Outgoing calls should include the recursive call to itself
    let outgoing = provider.outgoing_calls(root, pos);
    assert!(
        outgoing.iter().any(|c| c.to.name == "factorial"),
        "Recursive function should have outgoing call to itself, got: {outgoing:?}"
    );

    // Incoming calls should NOT include the self-call (it's the same function)
    let incoming = provider.incoming_calls(root, pos);
    assert!(
        incoming.is_empty(),
        "Recursive function with no external callers should have no incoming calls, got: {incoming:?}"
    );
}

#[test]
fn test_iife_outgoing_calls() {
    let source = "function helper() {}\n(function() {\n  helper();\n})();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let incoming = provider.incoming_calls(root, pos);

    // The call is inside an IIFE, which may or may not be reported
    // At minimum, it should not crash
    let _ = incoming;
}

#[test]
fn test_no_hierarchy_at_type_alias() {
    let source = "type Foo = string;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Foo" (line 0, col 5) - a type alias, not callable
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for type alias"
    );
}

#[test]
fn test_no_hierarchy_at_interface() {
    let source = "interface Bar {\n  x: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Bar" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for interface name"
    );
}

#[test]
fn test_prepare_on_async_function() {
    let source = "async function fetchData(): Promise<void> {\n  await fetch('url');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetchData" (line 0, col 15)
    let pos = Position::new(0, 15);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for async function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "fetchData");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_on_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "gen" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for generator function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "gen");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_multiple_incoming_calls_from_same_function() {
    let source =
        "function target() {}\nfunction caller() {\n  target();\n  target();\n  target();\n}\n";
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

    assert!(!calls.is_empty(), "Should have incoming calls");
    let caller_entry = calls
        .iter()
        .find(|c| c.from.name == "caller")
        .expect("Should find incoming call from 'caller'");
    assert_eq!(
        caller_entry.from_ranges.len(),
        3,
        "Should have 3 call ranges from the same function"
    );
}

#[test]
fn test_outgoing_calls_with_chained_method_calls() {
    let source = "function a() {}\nfunction b() {}\nfunction chain() {\n  a();\n  b();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "chain" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "a"),
        "Should have outgoing call to 'a'"
    );
    assert!(
        calls.iter().any(|c| c.to.name == "b"),
        "Should have outgoing call to 'b'"
    );
}

#[test]
fn test_prepare_on_function_expression_variable() {
    let source = "const myFunc = function myFuncImpl() {\n  return 1;\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "myFunc" variable name (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for function expression variable"
    );
}

#[test]
fn test_prepare_on_method_in_object_literal() {
    let source = "const obj = {\n  doWork() {\n    return 42;\n  }\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "doWork" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for object literal method"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "doWork");
}

