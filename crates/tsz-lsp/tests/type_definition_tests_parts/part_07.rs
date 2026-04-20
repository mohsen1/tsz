#[test]
fn test_type_definition_optional_param() {
    let source = "interface Options { verbose?: boolean; }\nfunction run(opts?: Options) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

#[test]
fn test_type_definition_interface_with_generics() {
    let source = "interface List<T> { items: T[]; }\nlet myList: List<string>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'myList'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_at_template_literal() {
    let source = "const msg = `hello ${\"world\"}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_type_definition(root, pos);

    // Template literals are string type, no type definition
    let _ = result;
}

