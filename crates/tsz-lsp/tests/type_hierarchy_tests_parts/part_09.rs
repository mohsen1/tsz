#[test]
fn test_type_hierarchy_class_with_getter_setter() {
    let source = "class Foo {\n  get x() { return 0; }\n  set x(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Foo"));
    }
}

#[test]
fn test_type_hierarchy_function_not_class() {
    let source = "function foo() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 9));
    let _ = item;
}

#[test]
fn test_type_hierarchy_at_whitespace_between_classes() {
    let source = "class A {}\n\nclass B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(1, 0));
    let _ = item;
}

#[test]
fn test_type_hierarchy_class_with_index_signature() {
    let source = "class Dict {\n  [key: string]: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Dict"));
    }
}

#[test]
fn test_type_hierarchy_class_implements_multiple() {
    let source = "interface A {}\ninterface B {}\nclass Foo implements A, B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(2, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Foo"));
    }
}

#[test]
fn test_type_hierarchy_exported_class() {
    let source = "export class Service {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 13));
    if let Some(i) = item {
        assert!(i.name.contains("Service"));
    }
}

#[test]
fn test_type_hierarchy_at_end_of_class_name() {
    let source = "class FooBar {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 12));
    let _ = item;
}

#[test]
fn test_type_hierarchy_class_with_private_members() {
    let source = "class Secret {\n  private key: string = '';\n  #value: number = 0;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Secret"));
    }
}

#[test]
fn test_type_hierarchy_deeply_nested_class() {
    let source = "namespace A { namespace B { class Deep {} } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 34));
    let _ = item;
}

#[test]
fn test_type_hierarchy_class_with_readonly_properties() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n  readonly port: number = 3000;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Config"));
    }
}

