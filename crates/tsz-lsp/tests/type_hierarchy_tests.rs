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
