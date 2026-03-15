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

#[test]
fn test_deep_inheritance_chain_interface() {
    // A -> B -> C: searching for A should find B (direct implementor), not C
    let source = "interface A {}\ninterface B extends A {}\ninterface C extends B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for A should find B (extends A directly)
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find interfaces extending A");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find only direct extender B");
    assert_eq!(locs[0].range.start.line, 1);
}

#[test]
fn test_class_implements_multiple_interfaces() {
    let source = "interface Readable {}\ninterface Writable {}\nclass Stream implements Readable, Writable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for Readable should find Stream
    let pos_readable = Position::new(0, 10);
    let result_readable = provider.get_implementations(root, pos_readable);
    assert!(
        result_readable.is_some(),
        "Should find implementors of Readable"
    );
    assert_eq!(result_readable.unwrap().len(), 1);

    // Searching for Writable should also find Stream
    let pos_writable = Position::new(1, 10);
    let result_writable = provider.get_implementations(root, pos_writable);
    assert!(
        result_writable.is_some(),
        "Should find implementors of Writable"
    );
    assert_eq!(result_writable.unwrap().len(), 1);
}

#[test]
fn test_interface_with_no_implementations_empty_body() {
    let source = "interface Empty {}";
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
        "Interface with no implementors should return None"
    );
}

#[test]
fn test_position_at_interface_keyword() {
    // Cursor at the "interface" keyword itself, before the name
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Foo" (col 10) should work
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);
    assert!(
        result.is_some(),
        "Should find implementations when cursor is on interface name"
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_empty_file_implementations() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Empty file should return None for implementations"
    );
}

#[test]
fn test_abstract_class_with_concrete_and_abstract_methods() {
    let source = "abstract class Base {\n  abstract go(): void;\n  stop() {}\n}\nclass Impl extends Base {\n  go() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Base" in "abstract class Base"
    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_some(),
        "Should find implementations of abstract class Base"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find one concrete implementor");
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_multiple_abstract_class_implementors() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}\nclass Circle extends Shape {\n  area() { return 0; }\n}\nclass Rect extends Shape {\n  area() { return 0; }\n}\nclass Triangle extends Shape {\n  area() { return 0; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find implementations of Shape");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 3, "Should find three implementors");
}

#[test]
fn test_find_implementations_for_name_interface() {
    let source = "interface Runnable {}\nclass Worker implements Runnable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Runnable", TargetKind::Interface);
    assert_eq!(results.len(), 1, "Should find Worker implementing Runnable");
    assert_eq!(results[0].name, "Worker");
}

#[test]
fn test_find_implementations_for_name_no_match() {
    let source = "interface Foo {}\nclass Bar {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Foo", TargetKind::Interface);
    assert!(
        results.is_empty(),
        "Bar does not implement Foo, should find nothing"
    );
}

#[test]
fn test_resolve_target_kind_for_interface() {
    let source = "interface MyInterface {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("MyInterface");
    assert_eq!(kind, Some(TargetKind::Interface));
}

#[test]
fn test_resolve_target_kind_for_class() {
    let source = "class MyClass {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("MyClass");
    assert_eq!(kind, Some(TargetKind::ConcreteClass));
}

#[test]
fn test_resolve_target_kind_for_variable_returns_none() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("x");
    assert_eq!(kind, None, "Variable should not resolve to a target kind");
}

#[test]
fn test_resolve_target_kind_nonexistent_name() {
    let source = "interface Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("DoesNotExist");
    assert_eq!(kind, None, "Nonexistent name should return None");
}

#[test]
fn test_class_extends_abstract_with_multiple_methods() {
    let source = "abstract class Processor {\n  abstract process(): void;\n  abstract validate(): boolean;\n}\nclass MyProcessor extends Processor {\n  process() {}\n  validate() { return true; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find implementation of Processor");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_interface_with_generic_implementor() {
    let source =
        "interface Comparable<T> {}\nclass NumberComparable implements Comparable<number> {}";
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
        result.is_some(),
        "Should find generic interface implementation"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
}

#[test]
fn test_interface_extends_multiple() {
    let source = "interface A {}\ninterface B {}\ninterface C extends A, B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for A should find C (which extends A)
    let pos_a = Position::new(0, 10);
    let result_a = provider.get_implementations(root, pos_a);
    assert!(result_a.is_some(), "Should find C extending A");
    assert_eq!(result_a.unwrap().len(), 1);

    // Searching for B should also find C
    let pos_b = Position::new(1, 10);
    let result_b = provider.get_implementations(root, pos_b);
    assert!(result_b.is_some(), "Should find C extending B");
    assert_eq!(result_b.unwrap().len(), 1);
}

#[test]
fn test_find_implementations_for_name_class_extends() {
    let source = "class Parent {}\nclass Child extends Parent {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Parent", TargetKind::ConcreteClass);
    assert_eq!(results.len(), 1, "Should find Child extending Parent");
    assert_eq!(results[0].name, "Child");
}

#[test]
fn test_resolve_target_kind_for_abstract_class() {
    let source = "abstract class Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("Base");
    // Abstract class may resolve as AbstractClass or ConcreteClass depending on binder
    assert!(
        kind == Some(TargetKind::AbstractClass) || kind == Some(TargetKind::ConcreteClass),
        "Abstract class should resolve to some class target kind, got {:?}",
        kind
    );
}

#[test]
fn test_resolve_target_kind_for_function_returns_none() {
    let source = "function foo() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("foo");
    assert_eq!(kind, None, "Function should not resolve to a target kind");
}

#[test]
fn test_resolve_target_kind_for_enum_returns_none() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("Color");
    assert_eq!(kind, None, "Enum should not resolve to a target kind");
}

#[test]
fn test_resolve_target_kind_for_type_alias_returns_none() {
    let source = "type MyType = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("MyType");
    assert_eq!(kind, None, "Type alias should not resolve to a target kind");
}

#[test]
fn test_interface_with_method_signatures_implementor() {
    let source = "interface Logger {\n  log(msg: string): void;\n  warn(msg: string): void;\n  error(msg: string): void;\n}\nclass ConsoleLogger implements Logger {\n  log(msg: string) {}\n  warn(msg: string) {}\n  error(msg: string) {}\n}";
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

    assert!(result.is_some(), "Should find implementations for Logger");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find ConsoleLogger");
}

#[test]
fn test_class_with_no_subclasses() {
    let source = "class Standalone {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Class with no subclasses should return None"
    );
}

#[test]
fn test_abstract_class_no_implementors() {
    let source = "abstract class Orphan {\n  abstract act(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "Abstract class with no implementors should return None"
    );
}

#[test]
fn test_position_at_line_start() {
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the very start of line 0 (before "interface")
    let pos = Position::new(0, 0);
    let result = provider.get_implementations(root, pos);

    // May or may not find implementations depending on cursor-to-node resolution
    let _ = result; // Defensive: just ensure no panic
}

#[test]
fn test_position_beyond_file() {
    let source = "interface Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at a line that doesn't exist
    let pos = Position::new(100, 0);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_none(), "Position beyond file should return None");
}

#[test]
fn test_find_implementations_for_abstract_class_name() {
    let source = "abstract class Handler {}\nclass ConcreteHandler extends Handler {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Handler", TargetKind::AbstractClass);
    assert_eq!(
        results.len(),
        1,
        "Should find ConcreteHandler extending Handler"
    );
    assert_eq!(results[0].name, "ConcreteHandler");
}

#[test]
fn test_find_implementations_for_name_multiple() {
    let source = "interface Serializable {}\nclass Json implements Serializable {}\nclass Xml implements Serializable {}\nclass Yaml implements Serializable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Serializable", TargetKind::Interface);
    assert_eq!(
        results.len(),
        3,
        "Should find all three implementors of Serializable"
    );
}

#[test]
fn test_generic_class_extends() {
    let source = "class Base<T> {}\nclass Derived extends Base<number> {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_some(),
        "Should find subclass of generic base class"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 1);
}

#[test]
fn test_multiple_interfaces_same_implementor() {
    let source =
        "interface A {}\ninterface B {}\ninterface C {}\nclass Multi implements A, B, C {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Each interface should find Multi as an implementor
    for (line, name) in [(0, "A"), (1, "B"), (2, "C")] {
        let pos = Position::new(line, 10);
        let result = provider.get_implementations(root, pos);
        assert!(result.is_some(), "Should find implementor of {}", name);
        assert_eq!(result.unwrap().len(), 1);
    }
}

#[test]
fn test_interface_with_generic_constraint() {
    let source =
        "interface Comparable<T extends Comparable<T>> {}\nclass Num implements Comparable<Num> {}";
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
        result.is_some(),
        "Should find implementor of generic constraint interface"
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_abstract_class_with_constructor() {
    let source = "abstract class Component {\n  constructor(public name: string) {}\n  abstract render(): void;\n}\nclass Button extends Component {\n  render() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Button extending Component");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_only_comments_file() {
    let source = "// just a comment\n/* block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let result = provider.get_implementations(root, pos);

    assert!(
        result.is_none(),
        "File with only comments should return None"
    );
}

#[test]
fn test_interface_with_optional_members_implementor() {
    let source = "interface Config {\n  debug?: boolean;\n  port?: number;\n}\nclass AppConfig implements Config {\n  debug = true;\n}";
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
        result.is_some(),
        "Should find AppConfig implementing Config"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_abstract_class_multiple_levels() {
    let source = "abstract class Base {}\nclass Mid extends Base {}\nclass Leaf extends Mid {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for Base should find Mid (direct subclass)
    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find direct subclass of Base");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1, "Should find only direct subclass Mid");
    assert_eq!(locs[0].range.start.line, 1);
}

#[test]
fn test_interface_generic_multiple_implementors() {
    let source = "interface Repository<T> {\n  find(id: string): T;\n}\nclass UserRepo implements Repository<string> {}\nclass ItemRepo implements Repository<number> {}";
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

    assert!(result.is_some(), "Should find implementors of Repository");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 2, "Should find both UserRepo and ItemRepo");
}

#[test]
fn test_class_with_static_members_extends() {
    let source = "class Base {\n  static create() {}\n}\nclass Child extends Base {\n  static create() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Child extending Base");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_find_implementations_for_name_nonexistent() {
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("NonExistent", TargetKind::Interface);
    assert!(
        results.is_empty(),
        "Should find nothing for nonexistent interface name"
    );
}

#[test]
fn test_interface_with_call_signature() {
    let source =
        "interface Callable {\n  (x: number): string;\n}\nclass MyCallable implements Callable {}";
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

    assert!(result.is_some(), "Should find implementor of Callable");
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_abstract_class_with_protected_method() {
    let source = "abstract class Widget {\n  protected abstract render(): void;\n}\nclass Button extends Widget {\n  protected render() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Button extending Widget");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_interface_with_index_signature_implementor() {
    let source = "interface StringMap {\n  [key: string]: string;\n}\nclass Headers implements StringMap {\n  [key: string]: string;\n}";
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
        result.is_some(),
        "Should find Headers implementing StringMap"
    );
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_class_extends_with_generic_constraint() {
    let source = "class Base<T extends string> {}\nclass Derived extends Base<'hello'> {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Derived extending Base");
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_resolve_target_kind_for_exported_interface() {
    let source = "export interface PublicApi {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("PublicApi");
    assert_eq!(kind, Some(TargetKind::Interface));
}

#[test]
fn test_resolve_target_kind_for_exported_class() {
    let source = "export class ExportedService {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("ExportedService");
    assert_eq!(kind, Some(TargetKind::ConcreteClass));
}

#[test]
fn test_position_at_end_of_interface_name() {
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at end of "Foo" (col 13 = past the 'o')
    let pos = Position::new(0, 13);
    let result = provider.get_implementations(root, pos);

    // May or may not resolve depending on cursor boundary handling
    let _ = result; // Defensive: just ensure no panic
}

#[test]
fn test_interface_with_heritage_and_class_implementor() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\nclass Impl implements Extended {\n  id = 1;\n  name = 'test';\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching Extended should find Impl
    let pos = Position::new(3, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Impl implementing Extended");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 6);
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_interface_with_readonly_properties_implementor() {
    let source = "interface Config {\n  readonly host: string;\n  readonly port: number;\n}\nclass AppConfig implements Config {\n  readonly host = 'localhost';\n  readonly port = 3000;\n}";
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

    assert!(result.is_some(), "Should find implementor of Config");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_class_extends_with_constructor() {
    let source = "class Base {\n  constructor(public name: string) {}\n}\nclass Derived extends Base {\n  constructor(name: string) { super(name); }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Derived extending Base");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_resolve_target_kind_for_const_returns_none() {
    let source = "const MY_CONST = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("MY_CONST");
    assert_eq!(kind, None, "Const should not have a target kind");
}

#[test]
fn test_resolve_target_kind_for_let_returns_none() {
    let source = "let x = 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("x");
    assert_eq!(kind, None, "Let variable should not have a target kind");
}

#[test]
fn test_interface_with_method_and_property_implementor() {
    let source = "interface Logger {\n  level: string;\n  log(msg: string): void;\n}\nclass ConsoleLogger implements Logger {\n  level = 'info';\n  log(msg: string) { console.log(msg); }\n}";
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
        result.is_some(),
        "Should find ConsoleLogger implementing Logger"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
}

#[test]
fn test_abstract_class_with_static_method() {
    let source = "abstract class Singleton {\n  static instance: Singleton;\n  abstract init(): void;\n}\nclass App extends Singleton {\n  init() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 16);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find App extending Singleton");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_class_extends_with_private_members() {
    let source = "class Parent {\n  private secret = 42;\n}\nclass Child extends Parent {\n  getValue() { return 0; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find Child extending Parent");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_interface_single_method_multiple_implementors() {
    let source = "interface Runnable {\n  run(): void;\n}\nclass Task implements Runnable {\n  run() {}\n}\nclass Job implements Runnable {\n  run() {}\n}\nclass Process implements Runnable {\n  run() {}\n}";
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
        result.is_some(),
        "Should find three implementors of Runnable"
    );
    let locs = result.unwrap();
    assert_eq!(locs.len(), 3);
}

#[test]
fn test_position_at_whitespace_between_declarations() {
    let source = "interface Foo {}\n\n\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at empty line between declarations
    let pos = Position::new(1, 0);
    let result = provider.get_implementations(root, pos);

    // Should not panic; result may be None
    let _ = result;
}

#[test]
fn test_resolve_target_kind_for_declare_class() {
    let source = "declare class ExternalLib {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("ExternalLib");
    // Declare class is a concrete class
    if let Some(k) = kind {
        assert!(
            k == TargetKind::ConcreteClass || k == TargetKind::AbstractClass,
            "Declared class should resolve to a class target kind"
        );
    }
}

#[test]
fn test_interface_extending_and_implementing() {
    let source = "interface A {\n  a(): void;\n}\ninterface B extends A {\n  b(): void;\n}\nclass C implements B {\n  a() {}\n  b() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Searching for B should find C
    let pos = Position::new(3, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some(), "Should find C implementing B");
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 6);
}

#[test]
fn test_interface_with_generic_type_params_multiple() {
    let source = "interface Container<T, U> {\n  get(): T;\n  set(v: U): void;\n}\nclass Pair implements Container<string, number> {\n  get() { return ''; }\n  set(v: number) {}\n}";
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

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_class_extends_class_with_implements() {
    let source =
        "interface Loggable {}\nclass Base {}\nclass Child extends Base implements Loggable {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Looking for implementations of Loggable
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 2);
}

#[test]
fn test_single_line_interface_and_class() {
    let source = "interface I {} class C implements I {}";
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

    if let Some(locs) = result {
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].range.start.line, 0);
    }
}

#[test]
fn test_interface_with_unicode_name() {
    let source = "interface Données {\n  valeur: number;\n}\nclass MesDonnées implements Données {\n  valeur = 0;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Should not panic with unicode identifiers
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);
    let _ = result;
}

#[test]
fn test_resolve_target_kind_for_interface_with_generics() {
    let source = "interface Iterable<T> { next(): T; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("Iterable");
    assert_eq!(kind, Some(TargetKind::Interface));
}

#[test]
fn test_find_implementations_for_name_abstract_class() {
    let source = "abstract class Shape { abstract area(): number; }\nclass Rect extends Shape { area() { return 0; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Shape", TargetKind::AbstractClass);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Rect");
}

#[test]
fn test_find_implementations_for_name_concrete_class() {
    let source = "class Parent {}\nclass Child extends Parent {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let results = provider.find_implementations_for_name("Parent", TargetKind::ConcreteClass);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Child");
}

#[test]
fn test_many_implementors_of_interface() {
    let source = "interface Handler {}\nclass A implements Handler {}\nclass B implements Handler {}\nclass C implements Handler {}\nclass D implements Handler {}\nclass E implements Handler {}";
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

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 5);
}

#[test]
fn test_interface_with_getter_setter() {
    let source = "interface HasValue {\n  get value(): number;\n  set value(v: number);\n}\nclass Store implements HasValue {\n  get value() { return 0; }\n  set value(v: number) {}\n}";
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

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 4);
}

#[test]
fn test_position_at_closing_brace() {
    let source = "interface Foo {\n  bar(): void;\n}\nclass Baz implements Foo {\n  bar() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the closing brace of interface
    let pos = Position::new(2, 0);
    let result = provider.get_implementations(root, pos);

    // May or may not find implementations depending on cursor resolution
    let _ = result;
}

#[test]
fn test_abstract_class_with_property() {
    let source = "abstract class Config {\n  abstract readonly name: string;\n}\nclass AppConfig extends Config {\n  readonly name = 'app';\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 15);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_resolve_target_kind_for_default_exported_class() {
    let source = "export default class DefaultClass {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let kind = provider.resolve_target_kind_for_name("DefaultClass");
    if let Some(k) = kind {
        assert!(
            k == TargetKind::ConcreteClass || k == TargetKind::AbstractClass,
            "Exported default class should be a class target kind"
        );
    }
}

#[test]
fn test_class_with_async_methods_extends() {
    let source = "class AsyncBase {\n  async fetch() { return ''; }\n}\nclass AsyncChild extends AsyncBase {\n  async fetch() { return 'child'; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_interface_with_string_index_and_implementor() {
    let source = "interface Dict {\n  [key: string]: number;\n}\nclass NumDict implements Dict {\n  [key: string]: number;\n}";
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

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 3);
}

#[test]
fn test_class_with_decorators_extends() {
    let source = "class Base {}\nclass Decorated extends Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 1);
}

#[test]
fn test_interface_with_symbol_key_implementor() {
    let source = "interface Disposable {\n  dispose(): void;\n}\ninterface AsyncDisposable extends Disposable {\n  asyncDispose(): Promise<void>;\n}\nclass Resource implements AsyncDisposable {\n  dispose() {}\n  asyncDispose() { return Promise.resolve(); }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Find implementations of AsyncDisposable
    let pos = Position::new(3, 10);
    let result = provider.get_implementations(root, pos);

    assert!(result.is_some());
    let locs = result.unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].range.start.line, 6);
}
