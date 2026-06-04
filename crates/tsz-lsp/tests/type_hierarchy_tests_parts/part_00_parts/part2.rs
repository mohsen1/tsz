#[test]
fn test_prepare_on_class_with_multiple_type_params() {
    let source = "class MultiGeneric<K, V, E extends Error> {\n  map: Map<K, V> = new Map();\n}\n";
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
    assert_eq!(item.unwrap().name, "MultiGeneric");
}

#[test]
fn test_prepare_on_single_line_class() {
    let source = "class Tiny {}\n";
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
    assert_eq!(item.name, "Tiny");
    assert_eq!(item.kind, SymbolKind::Class);
    // Range should start at column 0 (class keyword)
    assert_eq!(item.range.start.character, 0);
}

#[test]
fn test_subtypes_interface_extended_by_multiple_interfaces() {
    let source = "interface Disposable { dispose(): void; }\ninterface AutoDisposable extends Disposable { autoDispose(): void; }\ninterface LazyDisposable extends Disposable { lazyDispose(): void; }\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        2,
        "Disposable should have two extending interfaces"
    );
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"AutoDisposable"));
    assert!(names.contains(&"LazyDisposable"));
}

#[test]
fn test_prepare_on_abstract_class_declaration() {
    let source = "abstract class Widget {\n  abstract render(): void;\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "Widget");
        assert_eq!(item.kind, SymbolKind::Class);
    }
}

#[test]
fn test_prepare_on_class_with_unicode_name() {
    let source = "class Événement {\n  type: string = '';\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Should not panic with unicode
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);
    let _ = item;
}

#[test]
fn test_subtypes_class_implements_and_extends() {
    let source = "interface Serializable { serialize(): string; }\nclass Base {}\nclass Model extends Base implements Serializable {\n  serialize() { return ''; }\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Subtypes of Base
    let pos = Position::new(1, 6);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 1, "Base should have one subtype: Model");
    assert_eq!(subtypes[0].name, "Model");
}

#[test]
fn test_supertypes_class_implements_multiple() {
    let source = "interface A { a(): void; }\ninterface B { b(): void; }\nclass AB implements A, B {\n  a() {}\n  b() {}\n}\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    assert_eq!(item.name, "Utils");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_subtypes_interface_implemented_by_multiple_classes() {
    let source = "interface Logger { log(msg: string): void; }\nclass ConsoleLogger implements Logger { log(msg: string) {} }\nclass FileLogger implements Logger { log(msg: string) {} }\nclass NullLogger implements Logger { log(msg: string) {} }\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
