#[test]
fn test_prepare_on_class_with_constructor() {
    let source = "class Point {\n  constructor(public x: number, public y: number) {}\n}\n";
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

    assert!(
        item.is_some(),
        "Should find hierarchy item for class with constructor"
    );
    assert_eq!(item.unwrap().name, "Point");
}

#[test]
fn test_subtypes_class_with_only_implements() {
    let source = "interface Logger { log(msg: string): void; }\nclass ConsoleLogger implements Logger {\n  log(msg: string) { }\n}\nclass FileLogger implements Logger {\n  log(msg: string) { }\n}\nclass NullLogger implements Logger {\n  log(msg: string) { }\n}\n";
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

    assert_eq!(subtypes.len(), 3, "Logger should have three implementors");
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"ConsoleLogger"));
    assert!(names.contains(&"FileLogger"));
    assert!(names.contains(&"NullLogger"));
}

#[test]
fn test_prepare_on_class_with_readonly_properties() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n  readonly port: number = 3000;\n}\n";
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
    assert_eq!(item.unwrap().name, "Config");
}

#[test]
fn test_supertypes_class_extends_generic_class() {
    let source = "class Collection<T> { items: T[] = []; }\nclass StringCollection extends Collection<string> {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "StringCollection" (line 1, col 6)
    let pos = Position::new(1, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        1,
        "StringCollection should have one supertype"
    );
    assert_eq!(supertypes[0].name, "Collection");
}

#[test]
fn test_prepare_on_class_at_start_of_name() {
    let source = "class MyWidget {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at start of "MyWidget" name (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    assert_eq!(item.unwrap().name, "MyWidget");
}

#[test]
fn test_prepare_on_class_at_end_of_name() {
    let source = "class Abc {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at end of "Abc" name (line 0, col 8)
    let pos = Position::new(0, 8);
    let item = provider.prepare(root, pos);
    // May or may not match depending on exact offset logic; should not panic
    let _ = item;
}

#[test]
fn test_subtypes_diamond_inheritance() {
    let source = "interface A {}\ninterface B extends A {}\ninterface C extends A {}\ninterface D extends B, C {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "A" (line 0, col 10) - A has direct subtypes B and C (not D)
    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        2,
        "A should have two direct subtypes (B and C)"
    );
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"B"));
    assert!(names.contains(&"C"));
}

#[test]
fn test_supertypes_diamond_inheritance() {
    let source = "interface A {}\ninterface B extends A {}\ninterface C extends A {}\ninterface D extends B, C {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "D" (line 3, col 10) - D extends B and C
    let pos = Position::new(3, 10);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        2,
        "D should have two direct supertypes (B and C)"
    );
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"B"));
    assert!(names.contains(&"C"));
}

#[test]
fn test_prepare_on_interface_with_call_signature() {
    let source = "interface Callable {\n  (x: number): string;\n}\n";
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

    assert!(
        item.is_some(),
        "Should find hierarchy item for callable interface"
    );
    assert_eq!(item.unwrap().name, "Callable");
}

#[test]
fn test_prepare_on_interface_with_index_signature() {
    let source = "interface Dict {\n  [key: string]: number;\n}\n";
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

    assert!(
        item.is_some(),
        "Should find hierarchy item for interface with index signature"
    );
    assert_eq!(item.unwrap().name, "Dict");
}

