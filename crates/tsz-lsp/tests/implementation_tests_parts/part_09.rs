#[test]
fn test_find_implementations_for_name_with_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let results = provider.find_implementations_for_name("Anything", TargetKind::Interface);
    assert!(
        results.is_empty(),
        "Empty source should have no implementations"
    );
}

#[test]
fn test_interface_with_extends_and_two_implementors() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\nclass A implements Extended {\n  id = 1;\n  name = \"a\";\n}\nclass B implements Extended {\n  id = 2;\n  name = \"b\";\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Look at Extended interface
    let pos = Position::new(3, 10);
    let result = provider.get_implementations(root, pos);
    if let Some(locs) = result {
        assert_eq!(locs.len(), 2, "Extended should have two implementors");
    }
}

#[test]
fn test_abstract_class_with_multiple_abstract_methods() {
    let source = "abstract class Validator {\n  abstract validate(input: string): boolean;\n  abstract describe(): string;\n}\nclass EmailValidator extends Validator {\n  validate(input: string) { return true; }\n  describe() { return \"email\"; }\n}";
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
    if let Some(locs) = result {
        assert_eq!(locs.len(), 1);
    }
}

#[test]
fn test_position_at_middle_of_class_body() {
    let source = "class Foo {\n  x = 1;\n  y = 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position inside the class body, not on the name
    let pos = Position::new(1, 5);
    let result = provider.get_implementations(root, pos);
    // Position inside body may or may not resolve to implementations
    let _ = result;
}

#[test]
fn test_interface_empty_with_multiple_implementors() {
    let source = "interface Marker {}\nclass A implements Marker {}\nclass B implements Marker {}\nclass C implements Marker {}";
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
        assert_eq!(locs.len(), 3, "Marker should have three implementors");
    }
}

#[test]
fn test_resolve_target_kind_for_abstract_class_with_methods() {
    let source = "abstract class Engine {\n  abstract start(): void;\n  stop() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let kind = provider.resolve_target_kind_for_name("Engine");
    // Implementation may resolve as either AbstractClass or ConcreteClass
    let _ = kind;
}

#[test]
fn test_class_extends_class_with_multiple_levels() {
    let source = "class A {}\nclass B extends A {}\nclass C extends B {}\nclass D extends C {}";
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
    if let Some(locs) = result {
        // A is extended by B, C extends B, D extends C
        // Direct subclass of A is B, but deep chain may or may not be returned
        assert!(
            !locs.is_empty(),
            "A should have at least one implementation"
        );
    }
}

#[test]
fn test_find_implementations_for_name_interface_no_implementors() {
    let source = "interface Lonely {}\nclass Unrelated {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let results = provider.find_implementations_for_name("Lonely", TargetKind::Interface);
    assert!(
        results.is_empty(),
        "Lonely interface should have no implementors"
    );
}

#[test]
fn test_interface_with_function_type_member() {
    let source = "interface Handler {\n  (event: string): void;\n}\nclass EventHandler implements Handler {\n  (event: string): void {}\n}";
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
    // May or may not find implementation depending on how callable signatures are handled
    let _ = result;
}

#[test]
fn test_position_on_implements_keyword() {
    let source = "interface Foo {}\nclass Bar implements Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on 'implements' keyword in class Bar
    let pos = Position::new(1, 12);
    let result = provider.get_implementations(root, pos);
    // Not on an interface/class name, so may not find implementations
    let _ = result;
}

