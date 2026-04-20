#[test]
fn test_prepare_on_function_declaration() {
    let source = "function foo() {\n  return 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for 'foo'");
    let item = item.unwrap();
    assert_eq!(item.name, "foo");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_on_method_declaration() {
    let source = "class Foo {\n  bar() {\n    return 1;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for 'bar'");
    let item = item.unwrap();
    assert_eq!(item.name, "bar");
    assert_eq!(item.kind, SymbolKind::Method);
}

#[test]
fn test_prepare_on_class_static_block() {
    let source =
        "class C {\nstatic {\n  function foo() { bar(); }\n  function bar() {}\n  foo();\n}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "static" keyword (line 1, col 1).
    let pos = Position::new(1, 1);
    let item = provider
        .prepare(root, pos)
        .expect("Should find call hierarchy item for static block");

    assert_eq!(item.name, "static {}");
    assert_eq!(item.kind, SymbolKind::Constructor);
    assert_eq!(item.container_name, None);
    assert_eq!(item.selection_range.start, Position::new(1, 0));
    assert_eq!(item.selection_range.end, Position::new(1, 6));
}

#[test]
fn test_prepare_nested_function_in_static_block_has_no_class_container() {
    let source = "class C {\n  static {\n    function bar() {}\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(2, 13);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare nested function inside static block");

    assert_eq!(item.name, "bar");
    assert_eq!(item.container_name, None);
}

#[test]
fn test_prepare_not_on_function() {
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "x" (line 0, col 6) - a variable, not a function
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for variable"
    );
}

#[test]
fn test_prepare_on_export_equals_anonymous_function_uses_module_item() {
    let source = "export = function () {\n  baz();\n}\nfunction baz() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside `function` keyword of `export = function () {}`.
    let pos = Position::new(0, 10);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare call hierarchy item for export-equals function");

    assert_eq!(item.name, "test.ts");
    assert_eq!(item.kind, SymbolKind::Module);
    assert_eq!(item.range.start, Position::new(0, 0));
    assert_eq!(item.selection_range.start, Position::new(0, 0));
    assert_eq!(item.selection_range.end, Position::new(0, 0));
}

#[test]
fn test_outgoing_calls_from_export_equals_module_selection_span() {
    let source = "export = function () {\n  baz();\n}\nfunction baz() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Fourslash follow-up call hierarchy requests use the prepare item's selection span.
    let calls = provider.outgoing_calls(root, Position::new(0, 0));

    assert!(
        calls.iter().any(|call| call.to.name == "baz"),
        "Expected outgoing call to `baz` from export-equals module selection span"
    );
}

#[test]
fn test_outgoing_calls_simple() {
    let source = "function greet() {}\nfunction main() {\n  greet();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside "main" function name (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(!calls.is_empty(), "main should have outgoing calls");
    // Should find the call to greet
    let greet_call = calls.iter().find(|c| c.to.name == "greet");
    assert!(greet_call.is_some(), "Should find outgoing call to 'greet'");
    assert!(
        !greet_call.unwrap().from_ranges.is_empty(),
        "Should have at least one call range"
    );
}

#[test]
fn test_outgoing_calls_includes_new_expression_targets() {
    let source = "class Baz {}\nfunction build() {\n  new Baz();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "build" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    let baz_call = calls.iter().find(|c| c.to.name == "Baz");
    assert!(
        baz_call.is_some(),
        "Expected outgoing call target for constructor usage 'new Baz()'"
    );
    assert_eq!(
        baz_call.unwrap().from_ranges.len(),
        1,
        "Expected one constructor callsite range"
    );
    assert_eq!(
        baz_call.unwrap().to.kind,
        SymbolKind::Class,
        "Constructor target should be classified as class in call hierarchy"
    );
}

#[test]
fn test_outgoing_calls_includes_new_expression_forward_declared_class() {
    let source = "function bar() {\n  new Baz();\n}\n\nclass Baz {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "Baz"),
        "Expected outgoing call target for forward-declared constructor usage 'new Baz()'"
    );
}

