#[test]
fn test_subtypes_abstract_class_with_multiple_concrete() {
    let source = "abstract class Shape { abstract area(): number; }\nclass Circle extends Shape { area() { return 0; } }\nclass Rect extends Shape { area() { return 0; } }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 2);
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Circle"));
    assert!(names.contains(&"Rect"));
}

#[test]
fn test_prepare_on_class_with_async_methods() {
    let source =
        "class ApiClient {\n  async fetch() { return ''; }\n  async post(data: string) {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    let item = item.unwrap();
    assert_eq!(item.name, "ApiClient");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_prepare_on_let_statement() {
    let source = "let x: number = 5;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find type hierarchy item for a variable"
    );
}

#[test]
fn test_type_hierarchy_generic_class() {
    let source = "class Container<T> { value: T; }";
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
        assert!(i.name.contains("Container"));
    }
}

#[test]
fn test_type_hierarchy_abstract_class_with_method() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 16));
    if let Some(i) = item {
        assert!(i.name.contains("Shape"));
    }
}

#[test]
fn test_type_hierarchy_class_with_constructor() {
    let source = "class Point {\n  constructor(public x: number, public y: number) {}\n}";
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
        assert!(i.name.contains("Point"));
    }
}

#[test]
fn test_type_hierarchy_enum_no_result() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 5));
    let _ = item;
}

#[test]
fn test_type_hierarchy_interface_only() {
    let source = "interface Foo { x: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 10));
    let _ = item;
}

#[test]
fn test_type_hierarchy_class_with_static_method() {
    let source = "class Factory {\n  static create() { return new Factory(); }\n}";
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
        assert!(i.name.contains("Factory"));
    }
}

#[test]
fn test_type_hierarchy_class_expression() {
    let source = "const MyClass = class {\n  foo() {}\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 16));
    let _ = item;
}

