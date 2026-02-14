use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

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

#[test]
fn test_call_hierarchy_item_serialization() {
    let item = CallHierarchyItem {
        name: "test".to_string(),
        kind: SymbolKind::Function,
        uri: "file:///test.ts".to_string(),
        range: Range::new(Position::new(0, 0), Position::new(1, 0)),
        selection_range: Range::new(Position::new(0, 9), Position::new(0, 13)),
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"name\":\"test\""));
    // SymbolKind::Function serializes as "Function" (serde default for enums)
    assert!(
        json.contains("\"kind\":\"Function\"") || json.contains("\"kind\":12"),
        "kind should serialize correctly, got: {}",
        json
    );

    let deserialized: CallHierarchyItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "test");
    assert_eq!(deserialized.kind, SymbolKind::Function);
}
