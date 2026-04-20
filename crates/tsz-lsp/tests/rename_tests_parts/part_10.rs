#[test]
fn test_rename_variable_in_computed_property() {
    let source = "const key = 'name';\nconst obj = { [key]: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
