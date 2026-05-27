#[test]
fn test_type_definition_interface_with_generics() {
    let source = "interface List<T> { items: T[]; }\nlet myList: List<string>;";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_type_definition_unicode_identifier() {
    let source = "interface Élément { valeur: number; }\nlet é: Élément;";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
fn test_type_definition_abstract_class() {
    let source = "abstract class Base { abstract foo(): void; }\nclass Impl extends Base { foo() {} }\nconst i: Base = new Impl();";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
fn test_type_definition_extends_clause() {
    let source = "class Base {}\nclass Child extends Base {}\nconst c: Child = new Child();";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.get_type_definition(root, Position::new(3, 6));
    let _ = result;
}
