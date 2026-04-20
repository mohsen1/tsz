#[test]
fn test_prepare_on_interface_with_methods() {
    let source = "interface Repository {\n  find(id: number): void;\n  save(item: any): void;\n  delete(id: number): void;\n}\n";
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
    assert_eq!(item.name, "Repository");
    assert_eq!(item.kind, SymbolKind::Interface);
}

#[test]
fn test_class_implements_multiple_interfaces() {
    let source = "interface Readable { read(): void; }\ninterface Writable { write(): void; }\ninterface Closeable { close(): void; }\nclass Stream implements Readable, Writable, Closeable {\n  read() {}\n  write() {}\n  close() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Stream" (line 3, col 6)
    let pos = Position::new(3, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 3, "Stream should have three supertypes");
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Readable"));
    assert!(names.contains(&"Writable"));
    assert!(names.contains(&"Closeable"));
}

#[test]
fn test_subtypes_interface_with_no_implementors() {
    let source = "interface Orphan {\n  method(): void;\n}\nclass Unrelated {}\n";
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

    assert!(
        subtypes.is_empty(),
        "Interface with no implementors should have no subtypes"
    );
}

#[test]
fn test_prepare_on_whitespace_only_source() {
    let source = "   \n   \n   ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 1);
    let item = provider.prepare(root, pos);
    assert!(item.is_none(), "Whitespace-only source should return None");
}

#[test]
fn test_abstract_class_subtypes_with_concrete() {
    let source = "abstract class EventEmitter {\n  abstract emit(event: string): void;\n}\nclass NodeEmitter extends EventEmitter {\n  emit(event: string) {}\n}\nclass BrowserEmitter extends EventEmitter {\n  emit(event: string) {}\n}\n";
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

    assert_eq!(subtypes.len(), 2, "EventEmitter should have two subtypes");
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"NodeEmitter"));
    assert!(names.contains(&"BrowserEmitter"));
}

#[test]
fn test_class_extends_and_implements_multiple() {
    let source = "interface Serializable { serialize(): string; }\ninterface Cloneable { clone(): any; }\nclass Base { id: number; }\nclass Entity extends Base implements Serializable, Cloneable {\n  serialize() { return ''; }\n  clone() { return this; }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Entity" (line 3, col 6)
    let pos = Position::new(3, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        3,
        "Entity should have three supertypes (Base, Serializable, Cloneable)"
    );
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Base"));
    assert!(names.contains(&"Serializable"));
    assert!(names.contains(&"Cloneable"));
}

#[test]
fn test_prepare_on_class_inside_body() {
    let source = "class Outer {\n  method() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside the class body (at 'method'), not on the class name
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);
    // May or may not find the enclosing class; just ensure no panic
    let _ = item;
}

#[test]
fn test_interface_subtypes_both_class_and_interface() {
    let source = "interface Iterable { next(): any; }\nclass ArrayIter implements Iterable { next() { return null; } }\ninterface AsyncIterable extends Iterable { nextAsync(): any; }\n";
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
        "Iterable should have two subtypes (ArrayIter and AsyncIterable)"
    );
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"ArrayIter"));
    assert!(names.contains(&"AsyncIterable"));
}

#[test]
fn test_prepare_detail_for_class() {
    let source = "class Widget {}\n";
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
    assert_eq!(item.detail, Some("class".to_string()));
}

#[test]
fn test_prepare_detail_for_interface() {
    let source = "interface Handler {}\n";
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
    assert_eq!(item.detail, Some("interface".to_string()));
}

// =========================================================================
// Additional type hierarchy tests (batch 2)
// =========================================================================

