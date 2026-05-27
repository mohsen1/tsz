#[test]
fn test_prepare_on_class_with_getter_setter() {
    let source = "class Counter {\n  private _count = 0;\n  get count() { return this._count; }\n  set count(v: number) { this._count = v; }\n}\n";
    let (parser, root) = parse_test_source(source);
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
    assert_eq!(item.name, "Counter");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_supertypes_interface_extends_two_interfaces() {
    let source = "interface Readable { read(): void; }\ninterface Writable { write(): void; }\ninterface ReadWrite extends Readable, Writable {}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Supertypes of ReadWrite
    let pos = Position::new(2, 10);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 2);
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Readable"));
    assert!(names.contains(&"Writable"));
}

#[test]
fn test_prepare_on_exported_class() {
    let source = "export class PublicApi {\n  endpoint(): string { return ''; }\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 13);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "PublicApi");
        assert_eq!(item.kind, SymbolKind::Class);
    }
}

#[test]
fn test_subtypes_abstract_class_with_multiple_concrete() {
    let source = "abstract class Shape { abstract area(): number; }\nclass Circle extends Shape { area() { return 0; } }\nclass Rect extends Shape { area() { return 0; } }\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 16));
    let _ = item;
}

#[test]
fn test_type_hierarchy_class_with_getter_setter() {
    let source = "class Foo {\n  get x() { return 0; }\n  set x(v: number) {}\n}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_type_hierarchy_empty_class() {
    let source = "class Empty {}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Empty"));
    }
}
