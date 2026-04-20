#[test]
fn test_interface_extends_multiple_interfaces() {
    let source = "interface A { a(): void; }\ninterface B { b(): void; }\ninterface C extends A, B { c(): void; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "C" (line 2, col 10)
    let pos = Position::new(2, 10);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        2,
        "Interface C should have two supertypes (A and B)"
    );
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"A"), "Should contain A");
    assert!(names.contains(&"B"), "Should contain B");
}

#[test]
fn test_prepare_position_past_end_of_source() {
    let source = "class X {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position way past end of source
    let pos = Position::new(100, 100);
    let item = provider.prepare(root, pos);
    // Should not panic, may return None
    let _ = item;
}

#[test]
fn test_subtypes_interface_with_both_class_and_interface_subtypes() {
    let source = "interface Base { id: number; }\nclass Impl implements Base { id = 0; }\ninterface Extended extends Base { name: string; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" (line 0, col 10)
    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        2,
        "Base should have two subtypes (Impl and Extended)"
    );
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Impl"), "Should contain Impl");
    assert!(names.contains(&"Extended"), "Should contain Extended");
}

#[test]
fn test_prepare_on_class_with_generic_params() {
    let source = "class Container<T> {\n  value: T;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Container" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find hierarchy item for generic class"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "Container");
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn test_supertypes_on_class_not_in_file() {
    // Class with no heritage, querying supertypes should return empty
    let source = "interface ISerializable { serialize(): string; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "ISerializable" (line 0, col 10)
    let pos = Position::new(0, 10);
    let supertypes = provider.supertypes(root, pos);

    assert!(
        supertypes.is_empty(),
        "Interface with no extends should have no supertypes"
    );
}

#[test]
fn test_subtypes_multiple_levels_only_direct() {
    let source =
        "interface Root {}\ninterface Mid extends Root {}\ninterface Leaf extends Mid {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Root" (line 0, col 10) - should find only direct subtype Mid, not Leaf
    let pos = Position::new(0, 10);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        1,
        "Root should have only one direct subtype (Mid)"
    );
    assert_eq!(subtypes[0].name, "Mid");
}

#[test]
fn test_prepare_on_interface_with_generic_params() {
    let source = "interface Comparable<T> {\n  compareTo(other: T): number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Comparable" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find hierarchy item for generic interface"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "Comparable");
    assert_eq!(item.kind, SymbolKind::Interface);
}

#[test]
fn test_prepare_on_class_with_static_members() {
    let source = "class Registry {\n  static instance: Registry;\n  static create() { return new Registry(); }\n}\n";
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
        "Should find hierarchy item for class with static members"
    );
    assert_eq!(item.unwrap().name, "Registry");
}

#[test]
fn test_subtypes_deep_class_chain_middle() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}\nclass D extends C {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // B should have only one direct subtype: C
    let pos = Position::new(1, 6);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 1, "B should have one direct subtype (C)");
    assert_eq!(subtypes[0].name, "C");
}

#[test]
fn test_supertypes_deep_class_chain_middle() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}\nclass D extends C {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // D should have only one direct supertype: C
    let pos = Position::new(3, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        1,
        "D should have one direct supertype (C)"
    );
    assert_eq!(supertypes[0].name, "C");
}

