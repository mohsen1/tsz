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

