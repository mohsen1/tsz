#[test]
fn test_detailed_refs_let_reassignment_is_write() {
    let source = "let x = 1;\nx = 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(!writes.is_empty(), "Reassignment should be a write");
}

#[test]
fn test_detailed_refs_delete_expression() {
    let source = "const obj: any = { x: 1 };\ndelete obj.x;";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);
    assert!(!refs.is_empty());
}

// =========================================================================
// Additional reference tests (batch 4 — edge cases)
// =========================================================================

#[test]
fn test_find_references_single_char_identifier() {
    let source = "const a = 1;\na;";
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
            "Should find declaration + usage for single-char id"
        );
    }
}

#[test]
fn test_find_references_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 1;\n\u{00e4}\u{00f6}\u{00fc};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    let _ = refs;
}

#[test]
fn test_find_references_let_in_block_scope() {
    let source = "{ let y = 10; y; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find block-scoped let refs");
    }
}

#[test]
fn test_find_references_var_in_function() {
    let source = "function f() { var v = 1; return v; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 19));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find var decl + return usage");
    }
}

#[test]
fn test_find_references_ternary_condition() {
    let source = "const flag = true;\nconst result = flag ? 'yes' : 'no';";
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
            "Should find flag decl + ternary condition usage"
        );
    }
}

#[test]
fn test_find_references_while_loop_condition() {
    let source = "let running = true;\nwhile (running) { running = false; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 3, "Should find decl + condition + assignment");
    }
}

#[test]
fn test_find_references_do_while_condition() {
    let source = "let count = 0;\ndo { count++; } while (count < 5);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nested_destructuring() {
    let source = "const { a: { b } } = { a: { b: 42 } };\nb;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 0));
    let _ = refs;
}

