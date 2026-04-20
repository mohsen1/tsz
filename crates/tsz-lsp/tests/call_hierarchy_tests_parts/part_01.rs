#[test]
fn test_prepare_function_range_uses_source_body_end() {
    let source = "function bar() {\n  return 1;\n}\n\nclass Baz {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare function declaration item");

    assert_eq!(item.name, "bar");
    assert_eq!(item.range.start, Position::new(0, 0));
    assert_eq!(item.range.end, Position::new(2, 1));
}

#[test]
fn test_outgoing_calls_multiple() {
    let source = "function a() {}\nfunction b() {}\nfunction c() {\n  a();\n  b();\n  a();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "c" function name (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should find calls to a and b
    assert!(calls.len() >= 2, "Should find at least 2 outgoing targets");

    let a_call = calls.iter().find(|c| c.to.name == "a");
    assert!(a_call.is_some(), "Should find outgoing call to 'a'");
    // 'a' is called twice
    assert_eq!(
        a_call.unwrap().from_ranges.len(),
        2,
        "'a' should be called twice"
    );

    let b_call = calls.iter().find(|c| c.to.name == "b");
    assert!(b_call.is_some(), "Should find outgoing call to 'b'");
}

#[test]
fn test_outgoing_calls_for_static_block_include_only_direct_calls() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {}\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 3);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(
        calls.len(),
        1,
        "Expected only one direct outgoing call from static block body"
    );
    assert_eq!(calls[0].to.name, "foo");
    assert_eq!(calls[0].to.selection_range.start, Position::new(2, 13));
    assert_eq!(calls[0].from_ranges[0].start, Position::new(7, 4));
}

#[test]
fn test_outgoing_calls_for_function_nested_in_static_block_resolve_sibling_declaration() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {\n    }\n\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at foo declaration name.
    let pos = Position::new(2, 13);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].to.name, "bar");
    assert_eq!(calls[0].to.selection_range.start, Position::new(6, 13));
    assert_eq!(calls[0].from_ranges[0].start, Position::new(3, 6));
}

#[test]
fn test_outgoing_calls_no_calls() {
    let source = "function empty() {\n  const x = 1;\n}\n";
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
        "Function with no calls should have no outgoing calls"
    );
}

#[test]
fn test_incoming_calls_simple() {
    let source = "function target() {}\nfunction caller() {\n  target();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(!calls.is_empty(), "target should have incoming calls");
    let caller_item = calls.iter().find(|c| c.from.name == "caller");
    assert!(
        caller_item.is_some(),
        "Should find incoming call from 'caller'"
    );
}

#[test]
fn test_incoming_calls_include_decorator_references() {
    let source = "@bar\nclass Foo {\n}\n\nfunction bar() {\n  baz();\n}\n\nfunction baz() {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" declaration name.
    let pos = Position::new(4, 10);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| call.from.name == "Foo"),
        "Expected decorator-based incoming call from class 'Foo', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_include_tagged_template_references() {
    let source = "function foo() {\n  bar`a${1}b`;\n}\n\nfunction bar(array: TemplateStringsArray, ...args: any[]) {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" declaration name.
    let pos = Position::new(4, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| call.from.name == "foo"),
        "Expected tagged-template incoming call from 'foo', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_inside_static_block_report_static_block_caller() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {}\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" declaration name.
    let pos = Position::new(2, 13);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| {
            call.from.name == "static {}" && call.from.kind == SymbolKind::Constructor
        }),
        "Expected static block caller entry for foo(), got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_no_callers() {
    let source = "function unused() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "unused" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.is_empty(),
        "Uncalled function should have no incoming calls"
    );
}

