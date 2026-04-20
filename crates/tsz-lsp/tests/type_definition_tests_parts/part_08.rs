#[test]
fn test_type_definition_unicode_identifier() {
    let source = "interface Élément { valeur: number; }\nlet é: Élément;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Should not panic with unicode
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);
    let _ = result;
}

#[test]
fn test_type_definition_type_predicate() {
    let source =
        "interface Fish { swim(): void; }\nfunction isFish(pet: any): pet is Fish { return true; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'pet' parameter
    let pos = Position::new(1, 16);
    let result = provider.get_type_definition(root, pos);

    // 'any' type has no definition location
    let _ = result;
}

#[test]
fn test_type_definition_at_null_literal() {
    let source = "const n = null;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let result = provider.get_type_definition(root, pos);

    let _ = result;
}

#[test]
fn test_type_definition_generic_type_param() {
    let source = "function identity<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 21));
    let _ = result;
}

#[test]
fn test_type_definition_promise_type() {
    let source =
        "async function fetchData(): Promise<string> { return ''; }\nconst r = fetchData();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(1, 6));
    let _ = result;
}

#[test]
fn test_type_definition_tuple_type_pair() {
    let source = "type Pair = [string, number];\nconst p: Pair = ['a', 1];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(1, 6));
    let _ = result;
}

#[test]
fn test_type_definition_intersection_abc() {
    let source = "type A = { x: number };\ntype B = { y: string };\ntype C = A & B;\nconst c: C = { x: 1, y: '' };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(3, 6));
    let _ = result;
}

#[test]
fn test_type_definition_mapped_type_keys() {
    let source = "type Keys = 'a' | 'b';\ntype Mapped = { [K in Keys]: number };\nconst m: Mapped = { a: 1, b: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(2, 6));
    let _ = result;
}

#[test]
fn test_type_definition_conditional_is_string() {
    let source =
        "type IsString<T> = T extends string ? true : false;\nconst x: IsString<number> = false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(1, 6));
    let _ = result;
}

#[test]
fn test_type_definition_template_literal_type() {
    let source = "type EventName = `on${string}`;\nconst e: EventName = 'onClick';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(1, 6));
    let _ = result;
}

