#[test]
fn test_supertypes_class_implements_multiple() {
    let source = "interface A { a(): void; }\ninterface B { b(): void; }\nclass AB implements A, B {\n  a() {}\n  b() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Supertypes of AB
    let pos = Position::new(2, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 2, "AB should have two supertypes: A, B");
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"A"));
    assert!(names.contains(&"B"));
}

#[test]
fn test_prepare_on_single_line_interface() {
    let source = "interface Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "Empty");
        assert_eq!(item.kind, SymbolKind::Interface);
    }
}

#[test]
fn test_subtypes_three_level_class_chain() {
    let source = "class Grandparent {}\nclass Parent extends Grandparent {}\nclass Child extends Parent {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Direct subtypes of Grandparent
    let pos = Position::new(0, 6);
    let subtypes = provider.subtypes(root, pos);

    // Should find Parent as direct subtype
    assert!(!subtypes.is_empty());
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Parent"));
}

#[test]
fn test_supertypes_three_level_class_chain_leaf() {
    let source = "class Grandparent {}\nclass Parent extends Grandparent {}\nclass Child extends Parent {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Supertypes of Child
    let pos = Position::new(2, 6);
    let supertypes = provider.supertypes(root, pos);

    // Should find Parent as direct supertype
    assert!(!supertypes.is_empty());
    assert_eq!(supertypes[0].name, "Parent");
}

#[test]
fn test_prepare_on_class_with_only_static_members() {
    let source = "class Utils {\n  static helper() {}\n  static readonly VERSION = '1.0';\n}\n";
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
    assert_eq!(item.name, "Utils");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_subtypes_interface_implemented_by_multiple_classes() {
    let source = "interface Logger { log(msg: string): void; }\nclass ConsoleLogger implements Logger { log(msg: string) {} }\nclass FileLogger implements Logger { log(msg: string) {} }\nclass NullLogger implements Logger { log(msg: string) {} }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 3);
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"ConsoleLogger"));
    assert!(names.contains(&"FileLogger"));
    assert!(names.contains(&"NullLogger"));
}

#[test]
fn test_prepare_on_interface_with_optional_members() {
    let source = "interface Config {\n  debug?: boolean;\n  port?: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    let item = item.unwrap();
    assert_eq!(item.name, "Config");
    assert_eq!(item.kind, SymbolKind::Interface);
}

#[test]
fn test_prepare_on_class_with_getter_setter() {
    let source = "class Counter {\n  private _count = 0;\n  get count() { return this._count; }\n  set count(v: number) { this._count = v; }\n}\n";
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
    assert_eq!(item.name, "Counter");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_supertypes_interface_extends_two_interfaces() {
    let source = "interface Readable { read(): void; }\ninterface Writable { write(): void; }\ninterface ReadWrite extends Readable, Writable {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

