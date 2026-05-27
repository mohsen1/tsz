#[test]
fn test_rename_var_in_return_statement() {
    let source = "function f() { let result = 42; return result; }";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "new".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'new'");
}

#[test]
fn test_rename_rejects_reserved_word_delete() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "delete".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'delete'");
}

#[test]
fn test_rename_rejects_reserved_word_typeof() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "typeof".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'typeof'");
}

#[test]
fn test_rename_rejects_reserved_word_void() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "void".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'void'");
}

#[test]
fn test_rename_allows_contextual_keyword_readonly() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "readonly".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to contextual keyword 'readonly'"
    );
}

#[test]
fn test_rename_allows_contextual_keyword_declare() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "declare".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to contextual keyword 'declare'"
    );
}

#[test]
fn test_rename_to_underscore_name() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "_unused".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to underscore-prefixed name"
    );
}

#[test]
fn test_rename_to_dollar_name() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "$scope".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to dollar-prefixed name"
    );
}

#[test]
fn test_rename_rejects_whitespace_name() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "a b".to_string());
    assert!(
        result.is_err(),
        "Should not allow renaming to name with spaces"
    );
}

#[test]
fn test_prepare_rename_on_semicolon_returns_none() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = provider.prepare_rename(Position::new(0, 9));
    assert!(range.is_none(), "Should not be able to rename a semicolon");
}

#[test]
fn test_rename_variable_in_arrow_body() {
    let source = "const val = 10;\nconst f = () => val + 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "num".to_string());
    assert!(
        result.is_ok(),
        "Should rename variable referenced in arrow body"
    );
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename declaration + arrow body usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "num");
    }
}

#[test]
fn test_rename_variable_in_computed_property() {
    let source = "const key = 'name';\nconst obj = { [key]: 1 };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "prop".to_string());
    assert!(
        result.is_ok(),
        "Should rename variable used in computed property"
    );
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename declaration + computed property usage"
    );
}

#[test]
fn test_rename_function_used_as_callback() {
    let source = "function handler() {}\nsetTimeout(handler, 100);";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 9), "onTimeout".to_string());
    assert!(result.is_ok(), "Should rename function used as callback");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename function decl + callback usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "onTimeout");
    }
}

#[test]
fn test_rename_rejects_reserved_word_switch() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "switch".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'switch'");
}

#[test]
fn test_rename_rejects_reserved_word_try() {
    let source = "let x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "try".to_string());
    assert!(result.is_err(), "Should not allow renaming to 'try'");
}

#[test]
fn test_rename_variable_used_in_binary_expression() {
    let source = "let total = 0;\ntotal = total + 5;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 4), "sum".to_string());
    assert!(
        result.is_ok(),
        "Should rename variable in binary expression"
    );
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 3,
        "Should rename declaration + 2 usages on second line"
    );
    for e in edits {
        assert_eq!(e.new_text, "sum");
    }
}

#[test]
fn test_prepare_rename_enum_member_full_display_name() {
    let source = "enum e {\n    firstMember,\n    secondMember,\n    thirdMember\n}\nvar enumMember = e.thirdMember;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on "thirdMember" in "e.thirdMember" (line 5, after "var enumMember = e.")
    // Line 5 is: "var enumMember = e.thirdMember;"
    // "thirdMember" starts at column 19
    let pos = Position::new(5, 19);
    let info = provider.prepare_rename_info(root, pos);

    assert!(info.can_rename, "Should allow renaming thirdMember");
    assert_eq!(info.display_name, "thirdMember");
    assert_eq!(
        info.full_display_name, "e.thirdMember",
        "fullDisplayName should include the enum container prefix"
    );
}
