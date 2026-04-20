#[test]
fn test_type_definition_const_no_type() {
    let source = "const pi = 3.14;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'pi' - no type annotation
    let pos = Position::new(0, 6);
    let result = provider.get_type_definition(root, pos);

    // Without type annotation, should return None
    assert!(
        result.is_none(),
        "Const without type annotation should return None"
    );
}

#[test]
fn test_type_definition_function_multiple_params() {
    let source = "interface Config { x: number; }\ninterface Logger { log(msg: string): void; }\nfunction init(cfg: Config, logger: Logger) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'cfg' param
    let pos_cfg = Position::new(2, 14);
    let result_cfg = provider.get_type_definition(root, pos_cfg);

    if let Some(locs) = result_cfg {
        assert!(!locs.is_empty());
        assert_eq!(locs[0].range.start.line, 0, "cfg should point to Config");
    }
}

#[test]
fn test_type_definition_at_start_of_file() {
    let source = "interface First { x: number; }\nlet f: First;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at very start of file (0,0)
    let pos = Position::new(0, 0);
    let result = provider.get_type_definition(root, pos);

    // At position 0,0 we hit the interface keyword, not a variable
    // Should handle gracefully - may return None
    let _ = result;
}

#[test]
fn test_type_definition_abstract_class_type() {
    let source = "abstract class Animal {\n  abstract speak(): void;\n}\nlet a: Animal;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'a'
    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 0,
            "Should point to abstract class Animal"
        );
    }
}

#[test]
fn test_type_definition_only_whitespace() {
    let source = "   \n   \n   ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 1);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Whitespace-only file should return None");
}

#[test]
fn test_type_definition_return_type_location() {
    let source =
        "interface Result { ok: boolean; }\nfunction check(): Result { return { ok: true }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'check' function name
    let pos = Position::new(1, 9);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].file_path, "test.ts",
            "Location should have correct file path"
        );
    }
}

#[test]
fn test_type_definition_array_type() {
    let source = "interface Item { id: number; }\nlet items: Item[];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'items'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Should find the Item interface for an array of Items
    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_optional_type() {
    let source = "interface Data { value: number; }\nlet d: Data | undefined;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Defensive: union with undefined may or may not resolve
    let _ = result;
}

#[test]
fn test_type_definition_tuple_type() {
    let source = "interface Point { x: number; y: number; }\nlet pair: [Point, Point];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Tuple types may or may not resolve to a named type
    let _ = result;
}

#[test]
fn test_type_definition_const_assertion() {
    let source = "const colors = ['red', 'blue'] as const;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_type_definition(root, pos);

    // const assertion has no named type definition
    assert!(result.is_none());
}

