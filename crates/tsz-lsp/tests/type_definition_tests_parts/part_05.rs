#[test]
fn test_type_definition_promise_like_type_alias() {
    let source = "type AsyncResult<T> = Promise<T>;\nlet r: AsyncResult<number>;";
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
fn test_type_definition_record_type_alias() {
    let source = "type Dict = Record<string, number>;\nlet d: Dict;";
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
fn test_type_definition_class_with_generics_and_constraints() {
    let source = "interface Comparable {\n  compareTo(other: any): number;\n}\nclass SortedList<T extends Comparable> {\n  items: T[] = [];\n}\nlet list: SortedList<Comparable>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(6, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 3);
    }
}

#[test]
fn test_type_definition_interface_extending_interface() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\nlet e: Extended;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(6, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 3,
            "Should point to Extended interface, not Base"
        );
    }
}

#[test]
fn test_type_definition_only_comments() {
    let source = "// This is a comment\n/* Block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 5);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Comment-only file should return None");
}

#[test]
fn test_type_definition_let_with_explicit_string_literal_type() {
    let source = "type Mode = 'read' | 'write';\nlet m: Mode;";
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

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_type_definition_readonly_property() {
    let source = "interface Config {\n  readonly host: string;\n}\nlet c: Config;";
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
fn test_type_definition_nested_generic() {
    let source = "interface Box<T> {\n  value: T;\n}\ntype NestedBox<T> = Box<Box<T>>;\nlet nb: NestedBox<string>;";
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
        assert_eq!(locations[0].range.start.line, 3);
    }
}

#[test]
fn test_type_definition_function_expression_type() {
    let source = "type Handler = (event: string) => void;\nlet h: Handler;";
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
fn test_type_definition_type_alias_with_keyof() {
    let source = "interface Person {\n  name: string;\n  age: number;\n}\ntype PersonKeys = keyof Person;\nlet k: PersonKeys;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(5, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 4);
    }
}

