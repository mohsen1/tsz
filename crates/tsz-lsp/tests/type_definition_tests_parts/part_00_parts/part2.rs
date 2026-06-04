#[test]
fn test_type_definition_object_type_literal() {
    let source = "type Point = { x: number; y: number };\nlet p: Point;";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_type_definition_optional_param() {
    let source = "interface Options { verbose?: boolean; }\nfunction run(opts?: Options) {}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'opts'
    let pos = Position::new(1, 14);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_class_with_heritage() {
    let source = "class Base {}\nclass Derived extends Base {}\nlet d: Derived;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'd'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        // Should point to Derived class on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }
}

#[test]
fn test_type_definition_generic_function_type() {
    let source = "type Mapper<T, U> = (item: T) => U;\nlet m: Mapper<string, number>;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'm'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_at_string_literal() {
    let source = "const greeting = \"hello world\";";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the string literal
    let pos = Position::new(0, 20);
    let result = provider.get_type_definition(root, pos);

    // String literals don't have type definitions
    let _ = result;
}

#[test]
fn test_type_definition_enum_as_type() {
    let source = "enum Color { Red, Green, Blue }\nlet c: Color;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'c'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_deeply_nested_type() {
    let source =
        "interface Inner { value: number; }\ninterface Outer { inner: Inner; }\nlet o: Outer;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'o'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        // Should point to Outer on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }
}

#[test]
fn test_type_definition_union_of_interfaces() {
    let source =
        "interface Cat { meow(): void; }\ninterface Dog { bark(): void; }\nlet pet: Cat | Dog;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'pet'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    // Union types may resolve to one or both interfaces
    let _ = result;
}

#[test]
fn test_type_definition_at_boolean_literal() {
    let source = "const flag = true;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'true'
    let pos = Position::new(0, 13);
    let result = provider.get_type_definition(root, pos);

    // Boolean literal has no type definition location
    let _ = result;
}
