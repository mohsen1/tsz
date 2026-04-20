#[test]
fn test_interface_method_signature_prepare_and_incoming_calls() {
    let source =
        "interface I {\n    foo(): void;\n}\n\nconst obj: I = { foo() {} };\n\nobj.foo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let method_pos = Position::new(1, 4);
    let item = provider
        .prepare(root, method_pos)
        .expect("Should prepare call hierarchy item for interface method signature");
    assert_eq!(item.name, "foo");
    assert_eq!(item.kind, SymbolKind::Method);

    let incoming = provider.incoming_calls(root, method_pos);
    assert!(
        incoming
            .iter()
            .any(|call| call.from.kind == SymbolKind::File && !call.from_ranges.is_empty()),
        "Expected script-level incoming call for interface method signature, got: {incoming:?}"
    );

    let outgoing = provider.outgoing_calls(root, method_pos);
    assert!(
        outgoing.is_empty(),
        "Interface method signatures have no body and should not report outgoing calls"
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
        container_name: None,
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"name\":\"test\""));
    // SymbolKind::Function serializes as "Function" (serde default for enums)
    assert!(
        json.contains("\"kind\":\"Function\"") || json.contains("\"kind\":12"),
        "kind should serialize correctly, got: {json}"
    );

    let deserialized: CallHierarchyItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "test");
    assert_eq!(deserialized.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_on_arrow_function_assigned_to_variable() {
    let source = "const greet = (name: string) => {\n  return `Hello ${name}`;\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "greet" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for arrow function variable 'greet'"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "greet");
}

#[test]
fn test_prepare_on_constructor() {
    let source = "class Foo {\n  constructor(x: number) {\n    this.x = x;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "constructor" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for constructor"
    );
    let item = item.unwrap();
    assert_eq!(item.kind, SymbolKind::Constructor);
}

#[test]
fn test_prepare_on_getter() {
    let source = "class Foo {\n  get value(): number {\n    return 42;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "value" in getter (line 1, col 6)
    let pos = Position::new(1, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for getter");
    let item = item.unwrap();
    assert_eq!(item.name, "get value");
}

#[test]
fn test_prepare_on_setter() {
    let source = "class Foo {\n  set value(v: number) {\n    this._v = v;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "value" in setter (line 1, col 6)
    let pos = Position::new(1, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for setter");
    let item = item.unwrap();
    assert_eq!(item.name, "set value");
}

#[test]
fn test_prepare_on_static_method() {
    let source = "class Util {\n  static helper() {\n    return 1;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" (line 1, col 9)
    let pos = Position::new(1, 9);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for static method"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "helper");
    assert_eq!(item.kind, SymbolKind::Method);
}

#[test]
fn test_incoming_calls_from_nested_functions() {
    let source =
        "function target() {}\nfunction outer() {\n  function inner() {\n    target();\n  }\n}\n";
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
        calls.iter().any(|c| c.from.name == "inner"),
        "Should find incoming call from nested function 'inner', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_from_callbacks() {
    let source = "function handler() {}\nfunction setup() {\n  [1, 2].forEach(() => {\n    handler();\n  });\n}\n";
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

    // The call to handler() is inside an arrow function inside setup()
    // It should report the arrow function or setup as the caller
    assert!(
        !calls.is_empty(),
        "Should find incoming calls from callback/closure context"
    );
}

#[test]
fn test_outgoing_calls_from_class_constructor() {
    let source = "function init() {}\nfunction validate() {}\nclass App {\n  constructor() {\n    init();\n    validate();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "constructor" (line 3, col 2)
    let pos = Position::new(3, 2);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "init"),
        "Constructor should have outgoing call to 'init', got: {calls:?}"
    );
    assert!(
        calls.iter().any(|c| c.to.name == "validate"),
        "Constructor should have outgoing call to 'validate', got: {calls:?}"
    );
}

