#[test]
fn test_type_hierarchy_empty_class() {
    let source = "class Empty {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        TypeHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let item = provider.prepare(root, Position::new(0, 6));
    if let Some(i) = item {
        assert!(i.name.contains("Empty"));
    }
}
