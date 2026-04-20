#[test]
fn test_prepare_on_class_declaration() {
    let source = "class Animal {\n  speak() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Animal" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find type hierarchy item for 'Animal'"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "Animal");
    assert_eq!(item.kind, SymbolKind::Class);
    assert_eq!(item.detail, Some("class".to_string()));
}

#[test]
fn test_prepare_on_interface_declaration() {
    let source = "interface Shape {\n  area(): number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Shape" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find type hierarchy item for 'Shape'"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "Shape");
    assert_eq!(item.kind, SymbolKind::Interface);
    assert_eq!(item.detail, Some("interface".to_string()));
}

#[test]
fn test_prepare_not_on_type_declaration() {
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "x" (line 0, col 6) - a variable, not a type
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find type hierarchy item for a variable"
    );
}

#[test]
fn test_supertypes_class_extends() {
    let source = "class Base {\n  method() {}\n}\nclass Derived extends Base {\n  method() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Derived" (line 3, col 6)
    let pos = Position::new(3, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 1, "Derived should have one supertype");
    assert_eq!(supertypes[0].name, "Base");
    assert_eq!(supertypes[0].kind, SymbolKind::Class);
}

#[test]
fn test_supertypes_class_implements_interface() {
    let source = "interface Walkable {\n  walk(): void;\n}\nclass Person implements Walkable {\n  walk() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Person" (line 3, col 6)
    let pos = Position::new(3, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 1, "Person should have one supertype");
    assert_eq!(supertypes[0].name, "Walkable");
    assert_eq!(supertypes[0].kind, SymbolKind::Interface);
}

#[test]
fn test_supertypes_interface_extends_interface() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Extended" (line 3, col 10)
    let pos = Position::new(3, 10);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 1, "Extended should have one supertype");
    assert_eq!(supertypes[0].name, "Base");
    assert_eq!(supertypes[0].kind, SymbolKind::Interface);
}

#[test]
fn test_supertypes_multiple() {
    let source = "interface A {}\ninterface B {}\nclass C implements A, B {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "C" (line 2, col 6)
    let pos = Position::new(2, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(supertypes.len(), 2, "C should have two supertypes");
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"A"), "Should contain supertype A");
    assert!(names.contains(&"B"), "Should contain supertype B");
}

#[test]
fn test_supertypes_no_heritage() {
    let source = "class Standalone {\n  value: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Standalone" (line 0, col 6)
    let pos = Position::new(0, 6);
    let supertypes = provider.supertypes(root, pos);

    assert!(
        supertypes.is_empty(),
        "Class with no heritage should have no supertypes"
    );
}

#[test]
fn test_subtypes_class_extended_by_class() {
    let source = "class Base {\n  method() {}\n}\nclass Derived extends Base {\n  method() {}\n}\n";
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

    assert_eq!(subtypes.len(), 1, "Base should have one subtype");
    assert_eq!(subtypes[0].name, "Derived");
    assert_eq!(subtypes[0].kind, SymbolKind::Class);
}

#[test]
fn test_subtypes_interface_implemented_by_class() {
    let source =
        "interface Animal {\n  speak(): void;\n}\nclass Dog implements Animal {\n  speak() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Animal" (line 0, col 10)
    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 1, "Animal should have one subtype");
    assert_eq!(subtypes[0].name, "Dog");
    assert_eq!(subtypes[0].kind, SymbolKind::Class);
}

