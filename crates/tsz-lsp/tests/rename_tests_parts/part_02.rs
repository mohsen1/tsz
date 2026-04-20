#[test]
fn test_rename_import_specifier_produces_prefix() {
    let source = "import { foo } from \"./mod\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let result = provider.provide_rich_rename_edits(root, pos, "bar".to_string());
    assert!(result.is_ok(), "Rename should succeed: {:?}", result.err());

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];

    let has_prefix = edits.iter().any(|e| e.prefix_text.is_some());
    assert!(
        has_prefix,
        "Should produce prefix_text for import specifier: edits = {edits:?}"
    );

    if let Some(prefix_edit) = edits.iter().find(|e| e.prefix_text.is_some()) {
        assert_eq!(
            prefix_edit.prefix_text.as_deref(),
            Some("foo as "),
            "Prefix should be 'foo as ' for import specifier expansion"
        );
        assert_eq!(prefix_edit.new_text, "bar");
    }
}

#[test]
fn test_rename_parameter_across_body() {
    let source = "function demo(x: number) {\n  return x + 1;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let result = provider.provide_rename_edits(root, pos, "val".to_string());
    assert!(result.is_ok(), "Rename should succeed for parameter");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename parameter declaration and usage"
    );
    for edit in edits {
        assert_eq!(edit.new_text, "val");
    }
}

#[test]
fn test_rename_interface_name() {
    let source = "interface Foo { x: number; }\nconst a: Foo = { x: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 10);
    let result = provider.provide_rename_edits(root, pos, "Bar".to_string());
    assert!(result.is_ok(), "Rename should succeed for interface");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename interface declaration and usage"
    );
    for edit in edits {
        assert_eq!(edit.new_text, "Bar");
    }
}

#[test]
fn test_rename_type_alias() {
    let source = "type ID = string;\nconst x: ID = \"hello\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 5);
    let result = provider.provide_rename_edits(root, pos, "Ident".to_string());
    assert!(result.is_ok(), "Rename should succeed for type alias");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename type alias declaration and usage"
    );
    for edit in edits {
        assert_eq!(edit.new_text, "Ident");
    }
}

#[test]
fn test_rename_enum_name() {
    let source = "enum Color { Red, Green }\nconst c: Color = Color.Red;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 5);
    let result = provider.provide_rename_edits(root, pos, "Colour".to_string());
    assert!(result.is_ok(), "Rename should succeed for enum");

    let workspace_edit = result.unwrap();
    let edits = &workspace_edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename enum name across usages");
    for edit in edits {
        assert_eq!(edit.new_text, "Colour");
    }
}

#[test]
fn test_rename_text_edit_prefix_suffix_serialization() {
    let edit_plain = RenameTextEdit::new(
        Range::new(Position::new(0, 0), Position::new(0, 3)),
        "foo".to_string(),
    );
    let json_plain = serde_json::to_value(&edit_plain).unwrap();
    assert!(
        !json_plain.as_object().unwrap().contains_key("prefixText"),
        "prefixText should be omitted when None"
    );
    assert!(
        !json_plain.as_object().unwrap().contains_key("suffixText"),
        "suffixText should be omitted when None"
    );

    let edit_prefix = RenameTextEdit::with_prefix(
        Range::new(Position::new(0, 0), Position::new(0, 3)),
        "bar".to_string(),
        "old: ".to_string(),
    );
    let json_prefix = serde_json::to_value(&edit_prefix).unwrap();
    assert_eq!(
        json_prefix.get("prefixText").and_then(|v| v.as_str()),
        Some("old: ")
    );

    let edit_suffix = RenameTextEdit::with_suffix(
        Range::new(Position::new(0, 0), Position::new(0, 3)),
        "baz".to_string(),
        " as old".to_string(),
    );
    let json_suffix = serde_json::to_value(&edit_suffix).unwrap();
    assert_eq!(
        json_suffix.get("suffixText").and_then(|v| v.as_str()),
        Some(" as old")
    );
}

#[test]
fn test_rename_rejects_strict_mode_reserved_word() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.provide_rename_edits(root, pos, "implements".to_string());
    assert!(
        result.is_err(),
        "Should reject strict-mode reserved word 'implements'"
    );
}

#[test]
fn test_prepare_rename_info_kind_modifiers() {
    let source = "let localVar = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let info = provider.prepare_rename_info(root, pos);

    assert!(info.can_rename);
    assert!(
        !info.kind_modifiers.contains("declare"),
        "Local var should not have 'declare' modifier"
    );
}

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_rename_function_name() {
    let source = "function greet() {}\ngreet();\ngreet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 9), "hello".to_string());
    assert!(result.is_ok(), "Should rename function");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 3, "Should rename declaration + 2 calls");
    for e in edits {
        assert_eq!(e.new_text, "hello");
    }
}

#[test]
fn test_rename_class_name() {
    let source = "class Foo {}\nlet f = new Foo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "Bar".to_string());
    assert!(result.is_ok(), "Should rename class");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename class declaration + usage");
}

