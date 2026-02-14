use tsz_binder::BinderState;
use tsz_parser::ParserState;

#[test]
fn test_resolve_simple_variable() {
    // const x = 1; x + 1;
    let source = "const x = 1; x + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Should have a symbol for 'x'
    assert!(binder.file_locals.get("x").is_some());
}
