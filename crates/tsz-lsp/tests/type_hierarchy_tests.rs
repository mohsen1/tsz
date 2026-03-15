use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

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

#[test]
fn test_subtypes_interface_extended_by_interface() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\n";
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

    assert_eq!(subtypes.len(), 1, "Base interface should have one subtype");
    assert_eq!(subtypes[0].name, "Extended");
    assert_eq!(subtypes[0].kind, SymbolKind::Interface);
}

#[test]
fn test_subtypes_multiple_implementors() {
    let source = "interface Shape {\n  area(): number;\n}\nclass Circle implements Shape {\n  area() { return 0; }\n}\nclass Square implements Shape {\n  area() { return 0; }\n}\n";
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
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(subtypes.len(), 2, "Shape should have two subtypes");
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Circle"), "Should contain Circle");
    assert!(names.contains(&"Square"), "Should contain Square");
}

#[test]
fn test_subtypes_no_subtypes() {
    let source = "class Lonely {\n  value: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Lonely" (line 0, col 6)
    let pos = Position::new(0, 6);
    let subtypes = provider.subtypes(root, pos);

    assert!(
        subtypes.is_empty(),
        "Class with no subtypes should return empty list"
    );
}

#[test]
fn test_class_chain_subtypes() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "A" (line 0, col 6) - should find only direct subtype B
    let pos = Position::new(0, 6);
    let subtypes = provider.subtypes(root, pos);

    assert_eq!(
        subtypes.len(),
        1,
        "A should have only one direct subtype (B)"
    );
    assert_eq!(subtypes[0].name, "B");
}

#[test]
fn test_class_chain_supertypes() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "C" (line 2, col 6) - should find only direct supertype B
    let pos = Position::new(2, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        1,
        "C should have only one direct supertype (B)"
    );
    assert_eq!(supertypes[0].name, "B");
}

#[test]
fn test_class_extends_and_implements() {
    let source = "interface Flyable {\n  fly(): void;\n}\nclass Vehicle {}\nclass FlyingCar extends Vehicle implements Flyable {\n  fly() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "FlyingCar" (line 4, col 6)
    let pos = Position::new(4, 6);
    let supertypes = provider.supertypes(root, pos);

    assert_eq!(
        supertypes.len(),
        2,
        "FlyingCar should have two supertypes (Vehicle and Flyable)"
    );
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Vehicle"), "Should contain Vehicle");
    assert!(names.contains(&"Flyable"), "Should contain Flyable");
}

#[test]
fn test_prepare_returns_correct_ranges() {
    let source = "class MyClass {\n  value: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "MyClass" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    let item = item.unwrap();
    assert_eq!(item.name, "MyClass");

    // The selection range should cover just the name "MyClass"
    assert_eq!(item.selection_range.start.line, 0);
    assert_eq!(item.selection_range.start.character, 6);

    // The full range should cover the entire class declaration
    assert_eq!(item.range.start.line, 0);
    assert_eq!(item.range.start.character, 0);
}

#[test]
fn test_prepare_uri_is_set() {
    let source = "class Foo {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = TypeHierarchyProvider::new(
        arena,
        &binder,
        &line_map,
        "file:///test.ts".to_string(),
        source,
    );

    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    assert_eq!(item.unwrap().uri, "file:///test.ts");
}

#[test]
fn test_supertypes_with_extends() {
    let source = "class Animal {}\nclass Dog extends Animal {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Dog" (line 1, col 6)
    let pos = Position::new(1, 6);
    let supertypes = provider.supertypes(root, pos);
    // Dog extends Animal, so supertypes should include Animal
    assert!(
        supertypes.iter().any(|s| s.name == "Animal"),
        "Dog's supertypes should include Animal, got: {:?}",
        supertypes.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_supertypes_no_extends() {
    let source = "class Standalone {}\n";
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
        "Class without extends should have no supertypes"
    );
}

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
        "Base subtypes should include Child, got: {:?}",
        names
    );
    assert!(
        names.contains(&"Other"),
        "Base subtypes should include Other, got: {:?}",
        names
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
