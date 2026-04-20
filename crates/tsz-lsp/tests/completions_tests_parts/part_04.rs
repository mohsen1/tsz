#[test]
fn test_is_new_identifier_location_false_for_type_annotation_identifier_prefix() {
    let source = "interface VFS { getSourceFile(path: string): ts";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Type annotation identifier prefixes should not be treated as new identifier declaration locations"
    );
}

#[test]
fn test_is_new_identifier_location_false_for_interface_member_return_type_prefix() {
    let source = "export interface VFS {\n  getSourceFile(path: string): ts";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Interface member return type positions should not be treated as new identifier declaration locations"
    );
}

#[test]
fn test_is_new_identifier_location_false_for_identifier_prefix_after_statement_boundary() {
    let source = "import { x } from \"./a\";\nf";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Typing an identifier prefix at the start of a new statement should not be treated as a new identifier declaration location"
    );
}

#[test]
fn test_completion_result_struct_member_completion() {
    // Member completions should have is_member_completion = true and is_new_identifier_location = false
    let source = "const obj = { foo: 1 };
obj.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        &arena,
        &binder,
        &line_map,
        &interner,
        &src,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 4);
    let result = completions.get_completion_result(root, position);
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(result.is_member_completion, "Should be member completion");
    assert!(
        !result.is_global_completion,
        "Should not be global completion"
    );
    assert!(
        !result.is_new_identifier_location,
        "Member completion should not be new identifier location"
    );
}

#[test]
fn test_completion_result_struct_global_completion() {
    let source = "const x = 1;
";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let position = Position::new(1, 0);
    let result = completions.get_completion_result(root, position);
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(result.is_global_completion, "Should be global completion");
    assert!(
        !result.is_member_completion,
        "Should not be member completion"
    );
    assert!(!result.entries.is_empty(), "Should have entries");
}

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_completions_inside_class_body() {
    let source = "class Foo {\n  x = 1;\n  method() {\n    \n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside method body
    let items = completions.get_completions(root, Position::new(3, 4));
    assert!(
        items.is_some(),
        "Should have completions inside method body"
    );
}

#[test]
fn test_completions_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(0, 0));
    // Should not panic on empty file, may return None or empty
    let _ = items;
}

#[test]
fn test_completions_after_dot_with_multiple_properties() {
    let source = "const obj = { a: 1, b: 'hello', c: true };\nobj.";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let items = completions.get_completions(root, Position::new(1, 4));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"a"), "Should suggest property 'a'");
        assert!(names.contains(&"b"), "Should suggest property 'b'");
        assert!(names.contains(&"c"), "Should suggest property 'c'");
    }
}

#[test]
fn test_completions_in_for_loop() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside for-of loop"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"item"),
            "Should suggest loop variable 'item'"
        );
        assert!(
            names.contains(&"items"),
            "Should suggest outer variable 'items'"
        );
    }
}

#[test]
fn test_completions_in_arrow_function() {
    let source = "const outer = 42;\nconst fn = (param: number) => {\n  \n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside arrow function"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"outer"), "Should suggest outer variable");
    }
}

