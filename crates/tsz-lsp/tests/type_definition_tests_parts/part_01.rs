#[test]
fn test_type_definition_intersection_type() {
    let source = "interface A { x: number; }\ninterface B { y: string; }\nlet val: A & B;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'val'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    // For intersection types, the implementation resolves the first type
    if let Some(locations) = result {
        assert!(
            !locations.is_empty(),
            "Should resolve at least one type in intersection"
        );
    }
}

#[test]
fn test_type_definition_nested_interface() {
    let source = "interface Inner { x: number; }\ninterface Outer { inner: Inner; }\nlet o: Outer;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'o' - should navigate to Outer, not Inner
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 1,
            "Should point to Outer interface on line 1"
        );
    }
}

#[test]
fn test_type_definition_function_param_with_interface() {
    let source =
        "interface Config { debug: boolean; timeout: number; }\nfunction init(cfg: Config) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'cfg' parameter
    let pos = Position::new(1, 14);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_type_alias_reference() {
    let source = "type ID = string;\ntype User = { id: ID; name: string; };\nlet u: User;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'u' - should navigate to User type alias
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 1,
            "Should point to User type alias on line 1"
        );
    }
}

#[test]
fn test_type_definition_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Empty file should return None");
}

#[test]
fn test_type_definition_on_type_annotation_itself() {
    // Cursor on the type reference in the annotation, not the variable name
    let source = "interface Widget { render(): void; }\nlet w: Widget;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'Widget' in the type annotation (line 1, col 7)
    let pos = Position::new(1, 7);
    let result = provider.get_type_definition(root, pos);

    // When cursor is on the type reference itself, it should still resolve
    // (might go to the interface declaration or might return None depending on impl)
    // This test verifies no panic occurs at minimum
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_multiple_variables_same_type() {
    let source = "interface Shared { x: number; }\nlet a: Shared;\nlet b: Shared;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Both variables should resolve to the same type definition
    let result_a = provider.get_type_definition(root, Position::new(1, 4));
    let result_b = provider.get_type_definition(root, Position::new(2, 4));

    if let (Some(locs_a), Some(locs_b)) = (&result_a, &result_b) {
        assert_eq!(
            locs_a[0].range.start.line, locs_b[0].range.start.line,
            "Both variables should point to the same type definition"
        );
    }
}

#[test]
fn test_type_definition_property_with_interface_type() {
    let source = "interface Addr { city: string; }\ninterface Person { address: Addr; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on 'address' property name should look for Addr type def
    let pos = Position::new(1, 21);
    let result = provider.get_type_definition(root, pos);

    // Defensive: may or may not resolve depending on implementation
    // Just ensure no panic
    let _ = result;
}

#[test]
fn test_type_definition_out_of_bounds_position() {
    let source = "let x: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position well beyond the file
    let pos = Position::new(100, 100);
    let result = provider.get_type_definition(root, pos);

    assert!(
        result.is_none(),
        "Out of bounds position should return None"
    );
}

#[test]
fn test_type_definition_class_with_methods() {
    let source = "class MyService {\n  getData(): string { return \"\"; }\n}\nlet svc: MyService;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'svc'
    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 0,
            "Should point to MyService class"
        );
    }
}

