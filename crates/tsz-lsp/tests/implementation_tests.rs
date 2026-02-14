use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_interface_single_implementor() {
    let source =
        "interface Animal {\n  speak(): void;\n}\nclass Dog implements Animal {\n  speak() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Animal" in "interface Animal" (line 0, col ~10)
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find implementations for Animal");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find exactly one implementor");
    // The implementing class "Dog" is on line 3
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_interface_multiple_implementors() {
    let source = "interface Shape {\n  area(): number;\n}\nclass Circle implements Shape {\n  area() { return 0; }\n}\nclass Square implements Shape {\n  area() { return 0; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Shape" in "interface Shape"
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find implementations for Shape");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 2, "Should find two implementors");
}

#[test]
fn test_interface_extends_interface() {
    let source =
        "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" in "interface Base"
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find interfaces extending Base");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find one extending interface");
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_abstract_class_implementor() {
    let source = "abstract class Vehicle {\n  abstract drive(): void;\n}\nclass Car extends Vehicle {\n  drive() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Vehicle" in "abstract class Vehicle"
    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_some(),
        "Should find implementations for abstract class Vehicle"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find one implementor");
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_class_extends_concrete_class() {
    let source = "class Base {\n  method() {}\n}\nclass Derived extends Base {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" in "class Base"
    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find subclasses of Base");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find one subclass");
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_no_implementations() {
    let source = "interface Lonely {\n  value: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Should return None when no implementations exist"
    );
}

#[test]
fn test_not_on_interface_or_class() {
    let source = "const x = 1;\nx + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "x" in "const x = 1"
    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Should return None for non-interface/class symbols"
    );
}

#[test]
fn test_interface_with_multiple_heritage_types() {
    let source = "interface A {}\ninterface B {}\nclass C implements A, B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Test that searching for A finds C
    let pos_a = Position::new(0, 10);
    let result_a = provider.get_implementations(root, pos_a);
    assert!(result_a.is_some(), "Should find implementors of A");
    assert_eq!(result_a.unwrap().len(), 1);

    // Test that searching for B also finds C
    let pos_b = Position::new(1, 10);
    let result_b = provider.get_implementations(root, pos_b);
    assert!(result_b.is_some(), "Should find implementors of B");
    assert_eq!(result_b.unwrap().len(), 1);
}

#[test]
fn test_position_at_semicolon() {
    let source = "interface Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position past the end of the content
    let pos = Position::new(0, 50);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Should return None for position outside content"
    );
}

#[test]
fn test_class_chain() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for A should only find direct subclass B
    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find direct subclasses of A");
    let locs = result.unwrap();
    assert_eq!(
        locs.len(),
        1,
        "Should find only direct subclass (single-level)"
    );
    assert_eq!(locs[0].range.start.line, 1);
}
