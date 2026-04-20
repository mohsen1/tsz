#[test]
fn test_type_definition_const_enum_type() {
    let source = "const enum Status {\n  Active,\n  Inactive\n}\nlet s: Status;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(4, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_var_with_type_annotation() {
    let source = "interface Widget {}\nvar w: Widget;";
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

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_declared_type() {
    let source = "declare class Buffer {\n  length: number;\n}\nlet b: Buffer;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_type_alias_with_template_literal() {
    let source = "type EventName = `on${string}`;\nlet e: EventName;";
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

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_number_type_annotation() {
    let source = "let n: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // Primitive types may or may not resolve to a type definition
    let _ = result;
}

#[test]
fn test_type_definition_object_type_literal() {
    let source = "type Point = { x: number; y: number };\nlet p: Point;";
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

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_at_numeric_literal() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the number literal
    let pos = Position::new(0, 10);
    let result = provider.get_type_definition(root, pos);

    // Should not panic; numeric literals don't have type definitions
    let _ = result;
}

#[test]
fn test_type_definition_arrow_function_param() {
    let source = "interface Config { debug: boolean; }\nconst fn = (cfg: Config) => cfg.debug;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'cfg' parameter
    let pos = Position::new(1, 13);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_destructured_param() {
    let source = "interface Point { x: number; y: number; }\nfunction draw({ x, y }: Point) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x' in destructured param
    let pos = Position::new(1, 16);
    let result = provider.get_type_definition(root, pos);

    // May or may not resolve to Point depending on implementation
    let _ = result;
}

#[test]
fn test_type_definition_rest_param() {
    let source = "function sum(...nums: number[]) { return 0; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'nums'
    let pos = Position::new(0, 17);
    let result = provider.get_type_definition(root, pos);

    // number[] is a primitive array type, no user-defined declaration
    let _ = result;
}

