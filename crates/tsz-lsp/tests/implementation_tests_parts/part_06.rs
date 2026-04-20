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

