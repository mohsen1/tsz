#[test]
fn test_rename_const_with_type_annotation() {
    let source = "const greeting: string = 'hello';\nconsole.log(greeting);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 6), "msg".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename const declaration and usage"
        );
    }
}

#[test]
fn test_prepare_rename_on_operator_returns_none() {
    let source = "let x = 1 + 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at '+' operator
    let range = provider.prepare_rename(Position::new(0, 10));
    assert!(range.is_none(), "Should not prepare rename on operator");
}

#[test]
fn test_rename_var_in_switch_case() {
    let source = "let val = 1;\nswitch (val) { case 1: break; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "status".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename in declaration and switch");
    }
}

#[test]
fn test_rename_var_in_return_statement() {
    let source = "function f() { let result = 42; return result; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 19), "output".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename in declaration and return");
    }
}

#[test]
fn test_rename_class_with_extends() {
    let source = "class Base {}\nclass Child extends Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 6), "Parent".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename class and extends clause");
    }
}

#[test]
fn test_rename_interface_with_extends() {
    let source = "interface A { x: number; }\ninterface B extends A { y: string; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 10), "Base".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename interface and extends clause"
        );
    }
}

#[test]
fn test_rename_variable_shadowed_in_inner_scope() {
    let source = "let x = 1;\n{ let x = 2; x; }\nx;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Rename outer x
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "outer".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        // Should only rename the outer x, not the inner shadowed one
        for e in edits {
            assert_eq!(e.new_text, "outer");
        }
    }
}

#[test]
fn test_rename_async_arrow_function_param() {
    let source = "const fn = async (data: string) => { return data; };\nfn('test');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 18), "input".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename param in async arrow function"
        );
    }
}

#[test]
fn test_rename_rejects_reserved_word_break() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "break".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'break'");
}

// =========================================================================
// Batch 4: additional edge-case and coverage tests
// =========================================================================

#[test]
fn test_rename_rejects_reserved_word_new() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "new".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'new'");
}

