#[test]
fn test_type_definition_extends_clause() {
    let source = "class Base {}\nclass Child extends Base {}\nconst c: Child = new Child();";
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
fn test_type_definition_union_a_or_b() {
    let source = "interface A { a: number; }\ninterface B { b: string; }\ntype AorB = A | B;\nconst x: AorB = { a: 1 };";
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
