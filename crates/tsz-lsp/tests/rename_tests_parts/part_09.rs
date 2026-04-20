#[test]
fn test_rename_rejects_reserved_word_delete() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

