#[test]
fn test_subtypes_with_extends() {
    let source = "class Base {}\nclass Child extends Base {}\nclass Other extends Base {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" (line 0, col 6)
    let pos = Position::new(0, 6);
    let subtypes = provider.subtypes(root, pos);
    // Base has two subtypes: Child and Other
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Child"),
        "Base subtypes should include Child, got: {names:?}"
    );
    assert!(
        names.contains(&"Other"),
        "Base subtypes should include Other, got: {names:?}"
    );
}

#[test]
fn test_prepare_on_abstract_class() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}\n";
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
    assert!(
        item.is_some(),
        "Should prepare hierarchy for abstract class"
    );
    assert_eq!(item.unwrap().name, "Shape");
}

#[test]
fn test_interface_supertypes_with_extends() {
    let source = "interface Printable { print(): void; }\ninterface Loggable extends Printable { log(): void; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Loggable" (line 1, col 10)
    let pos = Position::new(1, 10);
    let supertypes = provider.supertypes(root, pos);
    assert!(
        supertypes.iter().any(|s| s.name == "Printable"),
        "Loggable's supertypes should include Printable"
    );
}

#[test]
fn test_prepare_on_non_declaration() {
    let source = "const x = 42;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at a variable, not a class/interface
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);
    assert!(
        item.is_none(),
        "Should not prepare hierarchy for a variable"
    );
}

#[test]
fn test_class_implements_interface_supertypes() {
    let source =
        "interface Runnable { run(): void; }\nclass Task implements Runnable {\n  run() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Task" (line 1, col 6)
    let pos = Position::new(1, 6);
    let supertypes = provider.supertypes(root, pos);
    // Task implements Runnable
    assert!(
        supertypes.iter().any(|s| s.name == "Runnable"),
        "Task's supertypes should include Runnable, got: {:?}",
        supertypes.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

// =========================================================================
// Additional tests for improved coverage
// =========================================================================

#[test]
fn test_prepare_on_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let item = provider.prepare(root, pos);
    assert!(item.is_none(), "Empty source should return None");
}

#[test]
fn test_prepare_on_function_declaration() {
    let source = "function doStuff() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "doStuff" - function declarations are not classes/interfaces
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);
    assert!(
        item.is_none(),
        "Function declarations should not produce type hierarchy items"
    );
}

#[test]
fn test_prepare_on_type_alias() {
    let source = "type MyType = string | number;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "MyType" - type aliases are not classes/interfaces
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);
    // Type aliases may or may not be supported; just ensure no panic
    let _ = item;
}

#[test]
fn test_prepare_on_enum_declaration() {
    let source = "enum Color { Red, Green, Blue }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Color" - enums are not classes/interfaces
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);
    // Enum may or may not be supported; just ensure no panic
    let _ = item;
}

#[test]
fn test_subtypes_abstract_class_extended() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}\nclass Circle extends Shape {\n  area() { return 0; }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Shape" (line 0, col 15)
    let pos = Position::new(0, 15);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        1,
        "Abstract class Shape should have one subtype (Circle)"
    );
    assert_eq!(subtypes[0].name, "Circle");
}

