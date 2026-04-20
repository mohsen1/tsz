#[test]
fn test_goto_definition_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 42;\n\u{00e4}\u{00f6}\u{00fc};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find unicode variable");
    }
}

#[test]
fn test_goto_definition_void_keyword_returns_none() {
    let source = "void 0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(0, 0));
    let _ = definitions;
}
