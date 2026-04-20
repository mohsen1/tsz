#[test]
fn test_find_references_in_if_condition() {
    let source = "const cond = true;\nif (cond) { }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + if-condition usage");
    }
}

#[test]
fn test_find_references_class_method_name() {
    let source = "class A {\n  run() {}\n}\nnew A().run();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_multiline_string_variable() {
    let source = "const msg = `line1\nline2\nline3`;\nconsole.log(msg);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find decl + log usage across multiline template"
        );
    }
}

#[test]
fn test_detailed_refs_for_loop_counter_is_write() {
    let source = "for (let i = 0; i < 10; i++) { i; }";
    let refs = get_detailed_refs(source, "test.ts", 0, 9);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(
        !writes.is_empty(),
        "for-loop init and increment should be writes"
    );
}
