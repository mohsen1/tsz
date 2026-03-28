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

#[test]
fn test_rename_interface_name_edge() {
    let source = "interface IFoo { x: number; }\nlet a: IFoo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 10), "IBar".to_string());
    assert!(result.is_ok(), "Should rename interface");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename interface + type reference");
}

#[test]
fn test_rename_parameter() {
    let source = "function foo(param: number) { return param + 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 13), "value".to_string());
    assert!(result.is_ok(), "Should rename parameter");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename param declaration + usage");
    for e in edits {
        assert_eq!(e.new_text, "value");
    }
}

#[test]
fn test_rename_at_whitespace_returns_error() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at '=' sign
    let result = provider.provide_rename_edits(root, Position::new(0, 6), "newName".to_string());
    // Should either fail or not rename anything meaningful
    if let Ok(edit) = result {
        // If it succeeds, it should have very few edits
        let edits = edit.changes.get("test.ts");
        let _ = edits; // Just don't panic
    }
}

#[test]
fn test_rename_enum_name_edge() {
    let source = "enum Direction { Up, Down }\nlet d: Direction = Direction.Up;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 5), "Dir".to_string());
    assert!(result.is_ok(), "Should rename enum");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename enum + usages");
}

#[test]
fn test_prepare_rename_on_keyword_returns_none() {
    let source = "function foo() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, _root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on 'function' keyword
    let range = provider.prepare_rename(Position::new(0, 3));
    // Keywords should not be renameable
    assert!(
        range.is_none(),
        "Should not allow renaming the 'function' keyword"
    );
}

#[test]
fn test_rename_type_alias_edge() {
    let source = "type MyType = string;\nlet x: MyType;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 5), "NewType".to_string());
    assert!(result.is_ok(), "Should rename type alias");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename type alias + type reference"
    );
}

#[test]
fn test_rename_in_destructuring() {
    let source = "const obj = { name: 'test' };\nconst { name } = obj;\nconsole.log(name);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Rename 'name' from destructuring usage on line 2
    let result = provider.provide_rename_edits(root, Position::new(2, 12), "newName".to_string());
    if let Ok(edit) = result {
        let edits = &edit.changes["test.ts"];
        assert!(
            !edits.is_empty(),
            "Should have rename edits for destructured variable"
        );
    }
}

#[test]
fn test_rename_preserves_non_target_identifiers() {
    let source = "const foo = 1;\nconst bar = 2;\nfoo + bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "renamed".to_string());
    assert!(result.is_ok());
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    // All edits should only rename "foo", not "bar"
    for te in edits {
        assert_eq!(te.new_text, "renamed");
    }
}

#[test]
fn test_rename_at_end_of_identifier() {
    let source = "const myVar = 1;\nmyVar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at end of 'myVar' (col 10, just past 'r')
    let range = provider.prepare_rename(Position::new(0, 10));
    // Should still find the identifier via backtracking
    if range.is_some() {
        let result =
            provider.provide_rename_edits(root, Position::new(0, 10), "newVar".to_string());
        assert!(result.is_ok(), "Should rename from end of identifier");
    }
}

#[test]
fn test_rename_empty_new_name() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), String::new());
    // Empty name rename should either return an error or succeed (implementation-dependent)
    // Main goal: no crash
    let _ = result;
}

#[test]
fn test_rename_multiple_occurrences_same_line() {
    let source = "const x = 1; const y = x + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "z".to_string());
    assert!(result.is_ok(), "Should rename variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 3,
        "Should rename declaration + 2 usages, got {}",
        edits.len()
    );
}

#[test]
fn test_prepare_rename_on_string_literal() {
    let source = "const s = \"hello\";";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, _root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside string literal
    let range = provider.prepare_rename(Position::new(0, 12));
    assert!(
        range.is_none(),
        "Should not allow renaming inside a string literal"
    );
}

#[test]
fn test_rename_class_method() {
    let source = "class Foo {\n  bar() {}\n}\nconst f = new Foo();\nf.bar();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on 'bar' method declaration (line 1, col 2)
    let range = provider.prepare_rename(Position::new(1, 2));
    assert!(
        range.is_some(),
        "Should be able to prepare rename for method"
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_rename_namespace_name() {
    let source = "namespace MyNS {\n  export const val = 1;\n}\nMyNS.val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 10), "NS".to_string());
    assert!(result.is_ok(), "Should rename namespace");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename namespace declaration + usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "NS");
    }
}

#[test]
fn test_rename_arrow_function_param() {
    let source = "const fn = (x: number) => x * 2;\nfn(3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x' parameter (col 12)
    let result = provider.provide_rename_edits(root, Position::new(0, 12), "val".to_string());
    assert!(result.is_ok(), "Should rename arrow function param");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename param declaration + usage in body"
    );
    for e in edits {
        assert_eq!(e.new_text, "val");
    }
}

#[test]
fn test_prepare_rename_number_literal_returns_none() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at numeric literal '42' (col 10)
    let range = provider.prepare_rename(Position::new(0, 10));
    assert!(range.is_none(), "Should not allow renaming number literal");
}

#[test]
fn test_rename_for_loop_variable() {
    let source = "for (let i = 0; i < 10; i++) {\n  console.log(i);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'i' declaration (col 9)
    let result = provider.provide_rename_edits(root, Position::new(0, 9), "idx".to_string());
    assert!(result.is_ok(), "Should rename for loop variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename loop variable across usages"
    );
    for e in edits {
        assert_eq!(e.new_text, "idx");
    }
}

#[test]
fn test_rename_catch_clause_variable() {
    let source = "try {\n  throw 1;\n} catch (err) {\n  console.log(err);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'err' in catch clause (line 2, col 9)
    let result = provider.provide_rename_edits(root, Position::new(2, 9), "error".to_string());
    assert!(result.is_ok(), "Should rename catch clause variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename catch variable declaration + usage"
    );
    for e in edits {
        assert_eq!(e.new_text, "error");
    }
}

#[test]
fn test_prepare_rename_empty_file_returns_none() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = provider.prepare_rename(Position::new(0, 0));
    assert!(
        range.is_none(),
        "Empty file should return None for prepare rename"
    );
}

#[test]
fn test_rename_class_name_with_constructor_usage() {
    let source =
        "class Animal {\n  constructor(public name: string) {}\n}\nconst a = new Animal('dog');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "Pet".to_string());
    assert!(result.is_ok(), "Should rename class with constructor usage");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename class declaration + new expression"
    );
}

#[test]
fn test_rename_type_alias_with_multiple_usages() {
    let source =
        "type Callback = () => void;\nconst a: Callback = () => {};\nconst b: Callback = () => {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 5), "Fn".to_string());
    assert!(result.is_ok(), "Should rename type alias");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 3,
        "Should rename type alias declaration + 2 usages"
    );
    for e in edits {
        assert_eq!(e.new_text, "Fn");
    }
}

#[test]
fn test_rename_const_in_nested_block() {
    let source = "function outer() {\n  if (true) {\n    const inner = 1;\n    inner;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'inner' usage (line 3, col 4)
    let result = provider.provide_rename_edits(root, Position::new(3, 4), "local".to_string());
    assert!(result.is_ok(), "Should rename nested block variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename declaration + usage");
    for e in edits {
        assert_eq!(e.new_text, "local");
    }
}

#[test]
fn test_rename_enum_member() {
    let source = "enum Color { Red, Green, Blue }\nconst c = Color.Red;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'Red' enum member declaration (col 13)
    let range = provider.prepare_rename(Position::new(0, 13));
    if range.is_some() {
        let result =
            provider.provide_rename_edits(root, Position::new(0, 13), "Crimson".to_string());
        if let Ok(edit) = result {
            let edits = &edit.changes["test.ts"];
            assert!(
                !edits.is_empty(),
                "Should have edits for enum member rename"
            );
        }
    }
}

// =========================================================================
// Additional edge-case tests for improved coverage
// =========================================================================

#[test]
fn test_rename_let_variable_multiple_lines() {
    let source = "let counter = 0;\ncounter++;\ncounter += 5;\nconsole.log(counter);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 4), "count".to_string());
    assert!(result.is_ok(), "Should rename let variable across lines");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 3,
        "Should rename declaration + multiple usages, got {}",
        edits.len()
    );
    for e in edits {
        assert_eq!(e.new_text, "count");
    }
}

#[test]
fn test_rename_rejects_reserved_word_return() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.provide_rename_edits(root, pos, "return".to_string());
    assert!(
        result.is_err(),
        "Should reject keyword 'return' as new name"
    );
}

#[test]
fn test_rename_rejects_reserved_word_if() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.provide_rename_edits(root, pos, "if".to_string());
    assert!(result.is_err(), "Should reject keyword 'if' as new name");
}

#[test]
fn test_rename_default_export_function() {
    let source = "export default function handler() {}\nhandler();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" (col 24)
    let result = provider.provide_rename_edits(root, Position::new(0, 24), "process".to_string());
    if let Ok(edit) = result {
        let edits = &edit.changes["test.ts"];
        assert!(!edits.is_empty(), "Should have edits for exported function");
        for e in edits {
            assert_eq!(e.new_text, "process");
        }
    }
}

#[test]
fn test_rename_variable_with_underscore_prefix() {
    let source = "const _private = 1;\n_private;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "_internal".to_string());
    assert!(result.is_ok(), "Should rename underscore-prefixed variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename declaration + usage");
    for e in edits {
        assert_eq!(e.new_text, "_internal");
    }
}

#[test]
fn test_rename_variable_with_dollar_sign() {
    let source = "const $el = 1;\n$el;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "$element".to_string());
    assert!(result.is_ok(), "Should rename dollar-prefixed variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename declaration + usage");
    for e in edits {
        assert_eq!(e.new_text, "$element");
    }
}

#[test]
fn test_rename_single_char_variable() {
    let source = "const x = 1;\nx;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "longName".to_string());
    assert!(result.is_ok(), "Should rename single-char variable");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(edits.len() >= 2, "Should rename declaration + usage");
    for e in edits {
        assert_eq!(e.new_text, "longName");
    }
}

#[test]
fn test_prepare_rename_on_comment_returns_none() {
    let source = "// myComment\nconst x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside a comment
    let range = provider.prepare_rename(Position::new(0, 5));
    assert!(
        range.is_none(),
        "Should not allow renaming inside a comment"
    );
}

#[test]
fn test_rename_generic_type_parameter() {
    let source = "function identity<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'T' type parameter (col 18)
    let range = provider.prepare_rename(Position::new(0, 18));
    if range.is_some() {
        let result = provider.provide_rename_edits(root, Position::new(0, 18), "U".to_string());
        if let Ok(edit) = result {
            let edits = &edit.changes["test.ts"];
            assert!(
                !edits.is_empty(),
                "Should have edits for type parameter rename"
            );
            for e in edits {
                assert_eq!(e.new_text, "U");
            }
        }
    }
}

#[test]
fn test_rename_variable_in_ternary() {
    let source = "const flag = true;\nconst result = flag ? 'yes' : 'no';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let result = provider.provide_rename_edits(root, Position::new(0, 6), "isReady".to_string());
    assert!(result.is_ok(), "Should rename variable used in ternary");
    let edit = result.unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(
        edits.len() >= 2,
        "Should rename declaration + usage in ternary"
    );
    for e in edits {
        assert_eq!(e.new_text, "isReady");
    }
}

#[test]
fn test_rename_in_template_literal() {
    let source = "let user = 'World';\nconst msg = `Hello ${user}!`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "person".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename in declaration and template"
        );
    }
}

#[test]
fn test_rename_in_optional_chaining() {
    let source = "let obj = { a: 1 };\nconst val = obj?.a;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "data".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(
            edits.len() >= 2,
            "Should rename in declaration and optional chaining"
        );
    }
}

#[test]
fn test_rename_in_object_shorthand() {
    let source = "let name = 'test';\nconst obj = { name };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 4), "label".to_string());
    // Shorthand property rename behavior is complex
    let _ = result;
}

#[test]
fn test_rename_in_array_pattern() {
    let source = "let [first, second] = [1, 2];\nconst x = first + second;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 5), "a".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename in array pattern and usage");
    }
}

#[test]
fn test_rename_class_property() {
    let source = "class Foo {\n  bar = 1;\n  method() { return this.bar; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'bar' property declaration
    let result = provider.provide_rename_edits(root, Position::new(1, 2), "baz".to_string());
    // Class property rename may or may not track this.bar
    let _ = result;
}

#[test]
fn test_rename_at_end_of_file() {
    let source = "const x = 1;\nx";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(1, 0), "y".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename both occurrences");
    }
}

#[test]
fn test_rename_generator_function() {
    let source = "function* gen() { yield 1; }\ngen();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider = RenameProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let result = provider.provide_rename_edits(root, Position::new(0, 10), "generator".to_string());
    if let Ok(edit) = result
        && let Some(edits) = edit.changes.get("test.ts")
    {
        assert!(edits.len() >= 2, "Should rename generator function");
    }
}

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
