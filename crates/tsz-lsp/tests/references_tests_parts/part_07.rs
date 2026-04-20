#[test]
fn test_find_references_switch_case_variable() {
    let source = "const x = 1;\nswitch(x) { case 0: break; default: x; }";
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
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_class_constructor_param() {
    let source = "class Foo {\n  constructor(public x: number) {}\n  get() { return this.x; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 22));
    let _ = refs;
}

#[test]
fn test_find_references_spread_element() {
    let source = "const arr = [1, 2];\nconst copy = [...arr];";
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
        assert!(r.len() >= 2, "Should find arr decl + spread usage");
    }
}

#[test]
fn test_find_references_typeof_expression() {
    let source = "const x = 42;\ntype T = typeof x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(!r.is_empty());
    }
}

#[test]
fn test_find_references_optional_chaining_variable() {
    let source = "const obj = { a: 1 };\nconst val = obj?.a;";
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
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nullish_coalescing_variable() {
    let source = "const x = null;\nconst y = x ?? 'default';";
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
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_multiple_declarations_same_name() {
    let source =
        "function foo() { const x = 1; return x; }\nfunction bar() { const x = 2; return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at x in foo
    let refs = find_refs.find_references(root, Position::new(0, 23));
    let _ = refs;
}

#[test]
fn test_find_references_export_assignment() {
    let source = "const value = 42;\nexport default value;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + export usage");
    }
}

#[test]
fn test_find_references_shorthand_property() {
    let source = "const x = 1;\nconst obj = { x };";
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
        assert!(r.len() >= 2, "Should find decl + shorthand usage");
    }
}

#[test]
fn test_find_references_class_static_property() {
    let source = "class Foo {\n  static count = 0;\n  inc() { Foo.count++; }\n}";
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

