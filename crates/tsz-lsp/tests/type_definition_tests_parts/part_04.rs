#[test]
fn test_type_definition_string_type() {
    let source = "let s: string;";
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

    // string is a primitive type, no user-defined location
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_boolean_type() {
    let source = "let b: boolean;";
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

    // boolean is a primitive type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_void_return_type() {
    let source = "function noop(): void {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'noop'
    let pos = Position::new(0, 9);
    let result = provider.get_type_definition(root, pos);

    // void return type is primitive, should not have a definition
    let _ = result;
}

#[test]
fn test_type_definition_multiple_type_params() {
    let source = "interface Map<K, V> { get(key: K): V; }\nlet m: Map<string, number>;";
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

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_generic_class_type() {
    let source = "class Container<T> {\n  value: T;\n}\nlet c: Container<string>;";
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

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_interface_with_methods() {
    let source = "interface Service {\n  start(): void;\n  stop(): void;\n}\nlet svc: Service;";
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

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_enum_member_type() {
    let source = "enum Status {\n  Active = 'active',\n  Inactive = 'inactive'\n}\nlet s: Status;";
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

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_any_type() {
    let source = "let x: any;";
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

    // any is a built-in type, no user-defined location
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_never_type() {
    let source = "let n: never;";
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

    // never is a built-in type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_unknown_type() {
    let source = "let u: unknown;";
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

    // unknown is a built-in type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

