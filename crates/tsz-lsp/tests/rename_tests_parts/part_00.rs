#[test]
fn test_rename_variable() {
    let source = "let oldName = 1; const b = oldName + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);

    let range = rename_provider.prepare_rename(pos);
    assert!(range.is_some(), "Should be able to prepare rename");

    let result = rename_provider.provide_rename_edits(root, pos, "newName".to_string());
    assert!(result.is_ok(), "Rename should succeed");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];

    assert!(
        edits.len() >= 2,
        "Should have at least 2 edits (declaration + usage)"
    );

    for edit in edits {
        assert_eq!(edit.new_text, "newName");
    }
}

#[test]
fn test_rename_uses_scope_cache() {
    let source = "let value = 1;\nvalue;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let mut scope_cache = ScopeCache::default();
    let pos = Position::new(1, 0);

    let result = rename_provider.provide_rename_edits_with_scope_cache(
        root,
        pos,
        "next".to_string(),
        &mut scope_cache,
        None,
    );
    assert!(result.is_ok(), "Rename should succeed with scope cache");
    assert!(
        !scope_cache.is_empty(),
        "Expected scope cache to populate for rename"
    );
}

#[test]
fn test_rename_invalid_keyword() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = rename_provider.provide_rename_edits(root, pos, "class".to_string());
    assert!(result.is_err(), "Should not allow renaming to keyword");
}

#[test]
fn test_rename_invalid_chars() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = rename_provider.provide_rename_edits(root, pos, "123var".to_string());
    assert!(result.is_err(), "Should not allow invalid identifier");
}

#[test]
fn test_rename_function() {
    let source = "function foo() {}\nfoo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 0);
    let result = rename_provider.provide_rename_edits(root, pos, "bar".to_string());
    assert!(result.is_ok(), "Rename should succeed");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should have at least 2 edits");
    for edit in edits {
        assert_eq!(edit.new_text, "bar");
    }
}

#[test]
fn test_rename_private_identifier() {
    let source = "class Foo {\n  #bar = 1;\n  method() {\n    this.#bar;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 9);
    let result = rename_provider.provide_rename_edits(root, pos, "baz".to_string());
    assert!(
        result.is_ok(),
        "Rename should succeed for private identifier"
    );

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename declaration and usage");
    for edit in edits {
        assert_eq!(edit.new_text, "#baz");
    }
}

#[test]
fn test_rename_private_identifier_with_hash() {
    let source = "class Foo {\n  #bar = 1;\n  method() {\n    this.#bar;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 9);
    let result = rename_provider.provide_rename_edits(root, pos, "#qux".to_string());
    assert!(
        result.is_ok(),
        "Rename should accept '#qux' for private identifier"
    );

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    for edit in edits {
        assert_eq!(edit.new_text, "#qux");
    }
}

#[test]
fn test_prepare_rename_invalid_position() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 8);
    let range = rename_provider.prepare_rename(pos);
    assert!(
        range.is_none(),
        "Should not be able to rename non-identifier"
    );
}

#[test]
fn test_rename_rejects_private_name_for_identifier() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = rename_provider.provide_rename_edits(root, pos, "#foo".to_string());
    assert!(
        result.is_err(),
        "Should not allow private names for identifiers"
    );
}

#[test]
fn test_rename_to_contextual_keyword() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let rename_provider =
        RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);

    let result = rename_provider.provide_rename_edits(root, pos, "string".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to 'string' (contextual keyword)"
    );

    let result = rename_provider.provide_rename_edits(root, pos, "type".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to 'type' (contextual keyword)"
    );

    let result = rename_provider.provide_rename_edits(root, pos, "async".to_string());
    assert!(
        result.is_ok(),
        "Should allow renaming to 'async' (contextual keyword)"
    );
}

// -----------------------------------------------------------------------
// New edge-case tests
// -----------------------------------------------------------------------

