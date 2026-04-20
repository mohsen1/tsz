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

