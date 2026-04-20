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
        "Abstract class should resolve to some class target kind, got {kind:?}"
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

