#[test]
fn test_type_definition_abstract_class() {
    let source = "abstract class Base { abstract foo(): void; }\nclass Impl extends Base { foo() {} }\nconst i: Base = new Impl();";
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
fn test_type_definition_keyof_type() {
    let source =
        "interface Foo { a: number; b: string; }\ntype Keys = keyof Foo;\nconst k: Keys = 'a';";
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
fn test_type_definition_readonly_array_numbers() {
    let source = "const arr: ReadonlyArray<number> = [1, 2, 3];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 6));
    let _ = result;
}

#[test]
fn test_type_definition_record_type() {
    let source = "const map: Record<string, number> = {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 6));
    let _ = result;
}

#[test]
fn test_type_definition_at_semicolon() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 11));
    let _ = result;
}

#[test]
fn test_type_definition_at_number_literal() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 10));
    let _ = result;
}

#[test]
fn test_type_definition_at_true_literal() {
    let source = "const x = true;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 10));
    let _ = result;
}

#[test]
fn test_type_definition_at_hello_string() {
    let source = r#"const x = "hello";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(0, 10));
    let _ = result;
}

#[test]
fn test_type_definition_namespace_member() {
    let source = "namespace NS { export interface Foo {} }\nconst x: NS.Foo = {};";
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
fn test_type_definition_optional_property() {
    let source = "interface Opts { x?: number; }\nconst o: Opts = {};";
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

