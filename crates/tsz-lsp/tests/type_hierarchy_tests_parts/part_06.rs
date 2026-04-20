#[test]
fn test_subtypes_empty_class() {
    let source = "class Empty {}\nclass NotRelated {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let subtypes = provider.subtypes(root, pos);

    assert!(subtypes.is_empty(), "Empty should have no subtypes");
}

#[test]
fn test_prepare_on_class_with_private_members() {
    let source = "class Encapsulated {\n  private secret: string = 'hidden';\n  #realSecret: number = 42;\n}\n";
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
    assert_eq!(item.unwrap().name, "Encapsulated");
}

#[test]
fn test_supertypes_single_class_no_parents() {
    let source = "class Root {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let supertypes = provider.supertypes(root, pos);

    assert!(
        supertypes.is_empty(),
        "Root class should have no supertypes"
    );
}

#[test]
fn test_prepare_multiple_classes_in_file() {
    let source = "class Alpha {}\nclass Beta {}\nclass Gamma {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Check each class name can be prepared
    let item_a = provider.prepare(root, Position::new(0, 6));
    assert!(item_a.is_some());
    assert_eq!(item_a.unwrap().name, "Alpha");

    let item_b = provider.prepare(root, Position::new(1, 6));
    assert!(item_b.is_some());
    assert_eq!(item_b.unwrap().name, "Beta");

    let item_c = provider.prepare(root, Position::new(2, 6));
    assert!(item_c.is_some());
    assert_eq!(item_c.unwrap().name, "Gamma");
}

#[test]
fn test_prepare_on_class_with_multiple_type_params() {
    let source = "class MultiGeneric<K, V, E extends Error> {\n  map: Map<K, V> = new Map();\n}\n";
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
    assert_eq!(item.unwrap().name, "MultiGeneric");
}

#[test]
fn test_prepare_on_single_line_class() {
    let source = "class Tiny {}\n";
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
    assert_eq!(item.name, "Tiny");
    assert_eq!(item.kind, SymbolKind::Class);
    // Range should start at column 0 (class keyword)
    assert_eq!(item.range.start.character, 0);
}

#[test]
fn test_subtypes_interface_extended_by_multiple_interfaces() {
    let source = "interface Disposable { dispose(): void; }\ninterface AutoDisposable extends Disposable { autoDispose(): void; }\ninterface LazyDisposable extends Disposable { lazyDispose(): void; }\n";
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

