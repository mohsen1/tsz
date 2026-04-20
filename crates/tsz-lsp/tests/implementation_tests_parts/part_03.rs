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
        assert!(result.is_some(), "Should find implementor of {name}");
        assert_eq!(result.unwrap().len(), 1);
    }
}

