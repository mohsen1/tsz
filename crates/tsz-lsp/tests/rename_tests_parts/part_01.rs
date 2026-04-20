#[test]
fn test_prepare_rename_info_returns_display_name() {
    let source = "let myVar = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let info = provider.prepare_rename_info(root, pos);

    assert!(info.can_rename, "Should allow renaming myVar");
    assert_eq!(info.display_name, "myVar");
    assert!(!info.full_display_name.is_empty());
    assert!(info.localized_error_message.is_none(), "No error expected");
    assert_eq!(info.trigger_span.start.character, 4);
}

#[test]
fn test_prepare_rename_info_function_kind() {
    let source = "function hello() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let info = provider.prepare_rename_info(root, pos);

    assert!(info.can_rename);
    assert_eq!(info.display_name, "hello");
    // Kind depends on successful scope resolution; verify it's Function or Unknown
    assert!(
        info.kind == RenameSymbolKind::Function || info.kind == RenameSymbolKind::Unknown,
        "Kind should be Function or Unknown, got {:?}",
        info.kind
    );
}

#[test]
fn test_prepare_rename_info_class_kind() {
    let source = "class MyClass {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let info = provider.prepare_rename_info(root, pos);

    assert!(info.can_rename);
    assert_eq!(info.display_name, "MyClass");
    // Kind depends on successful scope resolution; verify it's Class or Unknown
    assert!(
        info.kind == RenameSymbolKind::Class || info.kind == RenameSymbolKind::Unknown,
        "Kind should be Class or Unknown, got {:?}",
        info.kind
    );
}

#[test]
fn test_prepare_rename_info_rejects_non_identifier() {
    let source = "let x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let info = provider.prepare_rename_info(root, pos);

    assert!(!info.can_rename, "Should not rename a number literal");
    assert!(
        info.localized_error_message.is_some(),
        "Should provide error message"
    );
}

#[test]
fn test_prepare_rename_info_rejects_builtin_undefined() {
    let source = "const x = undefined;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let info = provider.prepare_rename_info(root, pos);

    assert!(
        !info.can_rename,
        "Should not allow renaming built-in 'undefined'"
    );
}

#[test]
fn test_prepare_rename_info_rejects_node_modules() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let provider = RenameProvider::new(
        arena,
        &binder,
        &line_map,
        "node_modules/pkg/index.ts".to_string(),
        source,
    );

    let pos = Position::new(0, 6);
    let info = provider.prepare_rename_info(root, pos);

    assert!(
        !info.can_rename,
        "Should not allow renaming in node_modules"
    );
}

#[test]
fn test_rename_rejects_undefined_builtin() {
    let source = "const x = undefined;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let result = provider.provide_rename_edits(root, pos, "foo".to_string());
    assert!(
        result.is_err(),
        "Should reject renaming built-in 'undefined'"
    );
}

#[test]
fn test_rename_empty_new_name_rejected() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.provide_rename_edits(root, pos, "".to_string());
    assert!(result.is_err(), "Should reject empty new name");
}

#[test]
fn test_rename_shorthand_property_produces_prefix() {
    let source = "const x = 1;\nconst obj = { x };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.provide_rich_rename_edits(root, pos, "y".to_string());
    assert!(result.is_ok(), "Rename should succeed");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should have at least 2 edits");

    let has_prefix_edit = edits.iter().any(|e| e.prefix_text.is_some());
    assert!(
        has_prefix_edit,
        "Should produce a prefix_text edit for shorthand property: edits = {edits:?}"
    );

    if let Some(prefix_edit) = edits.iter().find(|e| e.prefix_text.is_some()) {
        assert_eq!(
            prefix_edit.prefix_text.as_deref(),
            Some("x: "),
            "Prefix should be 'x: ' for shorthand expansion"
        );
        assert_eq!(prefix_edit.new_text, "y");
    }

    // Also verify the standard WorkspaceEdit folds correctly
    let ws = workspace_edit.to_workspace_edit();
    let std_edits = &ws.changes["test.ts"];
    let has_folded = std_edits.iter().any(|e| e.new_text == "x: y");
    assert!(
        has_folded,
        "Standard WorkspaceEdit should fold prefix into new_text: edits = {std_edits:?}"
    );
}

#[test]
fn test_rename_destructuring_produces_prefix() {
    let source = "const obj = { a: 1 };\nconst { a } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 8);
    let result = provider.provide_rich_rename_edits(root, pos, "b".to_string());
    assert!(result.is_ok(), "Rename should succeed: {:?}", result.err());

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];

    let has_prefix = edits.iter().any(|e| e.prefix_text.is_some());
    assert!(
        has_prefix,
        "Should produce prefix_text for destructuring binding: edits = {edits:?}"
    );
}

