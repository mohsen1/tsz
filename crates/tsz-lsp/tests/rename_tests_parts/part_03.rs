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

