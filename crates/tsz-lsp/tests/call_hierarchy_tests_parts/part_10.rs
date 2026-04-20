#[test]
fn test_prepare_on_generic_function() {
    let source = "function identity<T>(x: T): T {\n  return x;\n}\n";
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
        assert_eq!(item.name, "identity");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_multiple_calls_to_same_function() {
    let source = "function log(msg: string) {}\nfunction verbose() {\n  log('start');\n  log('middle');\n  log('end');\n}\n";
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
