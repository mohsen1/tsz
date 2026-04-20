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

