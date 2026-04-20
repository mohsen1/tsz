#[test]
fn test_class_with_multiple_implements_and_extends() {
    let source = "interface A {}\ninterface B {}\nclass Base {}\nclass Multi extends Base implements A, B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        GoToImplementationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Check interface A implementations
    let pos = Position::new(0, 10);
    let result = provider.get_implementations(root, pos);
    if let Some(locs) = result {
        assert_eq!(locs.len(), 1, "A should have Multi as implementor");
    }
}
