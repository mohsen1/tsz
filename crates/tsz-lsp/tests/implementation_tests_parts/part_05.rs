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

