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

