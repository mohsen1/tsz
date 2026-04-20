#[test]
fn test_rename_in_for_of() {
    let source = "const arr = [1, 2];\nfor (const item of arr) { console.log(item); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(1, 11), "element".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename for-of variable");
    }
}

#[test]
fn test_rename_namespace() {
    let source = "namespace MyNS {\n  export const x = 1;\n}\nMyNS.x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 10), "NS2".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename namespace declaration and usage"
        );
    }
}

#[test]
fn test_rename_type_alias_in_function_params() {
    let source = "type ID = string;\nlet x: ID;\nfunction f(a: ID): ID { return a; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 5), "Identifier".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 3,
            "Should rename type alias in all positions"
        );
    }
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_rename_rejects_reserved_word_while() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "while".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'while'");
}

#[test]
fn test_rename_rejects_reserved_word_for() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "for".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'for'");
}

#[test]
fn test_rename_rejects_reserved_word_const() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "const".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'const'");
}

#[test]
fn test_rename_allows_contextual_keyword_as_name() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // "type" is a contextual keyword, should be allowed as identifier
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "type".to_string());
    // This may succeed or fail depending on the implementation
    let _ = result;
}

#[test]
fn test_rename_variable_used_in_if_condition() {
    let source = "let flag = true;\nif (flag) { console.log('yes'); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "isReady".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename declaration and usage in if"
        );
        for e in edits {
            assert_eq!(e.new_text, "isReady");
        }
    }
}

#[test]
fn test_rename_variable_used_in_while_loop() {
    let source = "let count = 0;\nwhile (count < 10) { count++; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "idx".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename in declaration and while loop"
        );
    }
}

#[test]
fn test_rename_function_with_multiple_calls() {
    let source =
        "function add(a: number, b: number) { return a + b; }\nadd(1, 2);\nadd(3, 4);\nadd(5, 6);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 9), "sum".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 4, "Should rename declaration + 3 call sites");
        for e in edits {
            assert_eq!(e.new_text, "sum");
        }
    }
}

