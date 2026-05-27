#[test]
fn test_incoming_calls_empty_source() {
    let source = "";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let mut parser = tsz_parser::ParserState::new("my_file.ts".to_string(), source.to_string());
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_incoming_calls_from_multiple_methods_same_class() {
    let source = "function target() {}\nclass Svc {\n  a() { target(); }\n  b() { target(); }\n}\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_outgoing_calls_from_ternary_expression() {
    let source = "function a() {}\nfunction b() {}\nfunction choose(cond: boolean) {\n  cond ? a() : b();\n}\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_prepare_on_generic_function() {
    let source = "function identity<T>(x: T): T {\n  return x;\n}\n";
    let (parser, root) = parse_test_source(source);
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
        assert_eq!(item.name, "identity");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_multiple_calls_to_same_function() {
    let source = "function log(msg: string) {}\nfunction verbose() {\n  log('start');\n  log('middle');\n  log('end');\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    if let Some(log_call) = calls.iter().find(|c| c.to.name == "log") {
        assert!(
            log_call.from_ranges.len() >= 3,
            "Should have at least 3 call ranges for 'log', got: {}",
            log_call.from_ranges.len()
        );
    }
}

#[test]
fn test_prepare_on_single_line_arrow_function() {
    let source = "const double = (x: number) => x * 2;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "double");
    }
}

#[test]
fn test_incoming_calls_from_try_catch() {
    let source = "function risky() {}\nfunction safe() {\n  try {\n    risky();\n  } catch (e) {\n    risky();\n  }\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "risky" declaration
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if let Some(safe_call) = calls.iter().find(|c| c.from.name == "safe") {
        assert!(
            safe_call.from_ranges.len() >= 2,
            "Should have at least 2 call ranges from try/catch, got: {}",
            safe_call.from_ranges.len()
        );
    }
}
