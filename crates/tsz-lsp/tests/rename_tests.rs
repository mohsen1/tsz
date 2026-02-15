use super::*;
use crate::resolver::ScopeCache;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

// -----------------------------------------------------------------------
// Original tests (preserved)
// -----------------------------------------------------------------------

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
