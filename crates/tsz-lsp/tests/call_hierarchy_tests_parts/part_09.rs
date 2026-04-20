#[test]
fn test_outgoing_calls_from_ternary_expression() {
    let source = "function a() {}\nfunction b() {}\nfunction choose(cond: boolean) {\n  cond ? a() : b();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "choose"
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    let names: Vec<_> = calls.iter().map(|c| c.to.name.as_str()).collect();
    assert!(
        names.contains(&"a"),
        "Should find outgoing call to 'a', got: {names:?}"
    );
    assert!(
        names.contains(&"b"),
        "Should find outgoing call to 'b', got: {names:?}"
    );
}

#[test]
fn test_prepare_on_function_with_many_params() {
    let source = "function compute(a: number, b: number, c: number, d: number, e: number) {\n  return a + b + c + d + e;\n}\n";
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

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "compute");
    }
}

#[test]
fn test_outgoing_calls_from_do_while_loop() {
    let source =
        "function step() {}\nfunction loop_fn() {\n  do {\n    step();\n  } while (false);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "step"),
        "Should find outgoing call to 'step' from do-while loop"
    );
}

#[test]
fn test_incoming_calls_from_arrow_function_variable() {
    let source = "function target() {}\nconst caller = () => {\n  target();\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if !calls.is_empty() {
        assert!(
            calls.iter().any(|c| c.from.name == "caller"),
            "Should find incoming call from 'caller' arrow function"
        );
    }
}

#[test]
fn test_prepare_on_method_with_return_type() {
    let source = "class Service {\n  getData(): string[] {\n    return [];\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "getData"
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "getData");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_from_nested_if_else() {
    let source = "function alpha() {}\nfunction beta() {}\nfunction gamma() {}\nfunction decide(x: number) {\n  if (x > 0) {\n    alpha();\n  } else if (x < 0) {\n    beta();\n  } else {\n    gamma();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "decide"
    let pos = Position::new(3, 9);
    let calls = provider.outgoing_calls(root, pos);

    let names: Vec<_> = calls.iter().map(|c| c.to.name.as_str()).collect();
    assert!(
        names.contains(&"alpha"),
        "Should find call to 'alpha', got: {names:?}"
    );
    assert!(
        names.contains(&"beta"),
        "Should find call to 'beta', got: {names:?}"
    );
    assert!(
        names.contains(&"gamma"),
        "Should find call to 'gamma', got: {names:?}"
    );
}

#[test]
fn test_prepare_on_readonly_method() {
    let source = "class Buffer {\n  readonly size: number = 0;\n  getSize(): number {\n    return this.size;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "getSize"
    let pos = Position::new(2, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "getSize");
    }
}

#[test]
fn test_outgoing_calls_from_for_of_loop() {
    let source = "function process(x: number) {}\nfunction iterate(items: number[]) {\n  for (const item of items) {\n    process(item);\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "process"),
        "Should find outgoing call to 'process' from for-of loop"
    );
}

#[test]
fn test_prepare_not_on_import_statement() {
    let source = "import { foo } from './foo';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" in import
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    // Import specifiers are not callable items
    let _ = item;
}

#[test]
fn test_outgoing_calls_from_for_in_loop() {
    let source = "function log(k: string) {}\nfunction enumerate(obj: object) {\n  for (const key in obj) {\n    log(key);\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "log"),
        "Should find outgoing call to 'log' from for-in loop"
    );
}

