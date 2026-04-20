#[test]
fn test_goto_definition_class_static_block_local() {
    let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 4);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(definitions.is_some(), "Should resolve static block locals");
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 2,
            "Definition should be on line 2"
        );
    }
}

#[test]
fn test_goto_definition_not_found() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position outside any identifier
    let position = Position::new(0, 11); // At the semicolon

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should not find a definition
    assert!(
        definitions.is_none(),
        "Should not find definition at semicolon"
    );
}

// =========================================================================
// New edge case tests
// =========================================================================

#[test]
fn test_goto_definition_builtin_console_returns_none() {
    // "console" is a built-in global with no user declaration.
    // Should return None gracefully instead of crashing.
    let source = "console.log('hello');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "console" (line 0, column 0)
    let position = Position::new(0, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should return None (no crash) since console is a built-in
    assert!(
        definitions.is_none(),
        "Built-in global 'console' should return None, not crash"
    );
}

#[test]
fn test_goto_definition_builtin_array_returns_none() {
    let source = "const arr = new Array(10);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "Array" (line 0, column 16)
    let position = Position::new(0, 16);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "Built-in global 'Array' should return None"
    );
}

#[test]
fn test_goto_definition_builtin_promise_returns_none() {
    let source = "const p: Promise<number> = Promise.resolve(42);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the Promise usage (after the =)
    let position = Position::new(0, 27);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "Built-in global 'Promise' should return None"
    );
}

#[test]
fn test_goto_definition_no_crash_on_position_beyond_file() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position way beyond the file (line 100, column 0)
    let position = Position::new(100, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should return None (no crash)
    assert!(
        definitions.is_none(),
        "Position beyond file should return None without crash"
    );
}

#[test]
fn test_goto_definition_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let position = Position::new(0, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "Empty source should return None without crash"
    );
}

#[test]
fn test_goto_definition_self_declaration_identifier() {
    // Clicking on the declaration itself should navigate to it
    let source = "function hello() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "hello" in the function declaration (line 0, column 9)
    let position = Position::new(0, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should find the declaration (itself)
    assert!(
        definitions.is_some(),
        "Should find declaration for function name"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_is_builtin_global_helper() {
    // Test the is_builtin_global helper function directly
    assert!(is_builtin_global("console"));
    assert!(is_builtin_global("Array"));
    assert!(is_builtin_global("Promise"));
    assert!(is_builtin_global("Map"));
    assert!(is_builtin_global("Set"));
    assert!(is_builtin_global("setTimeout"));
    assert!(is_builtin_global("fetch"));
    assert!(is_builtin_global("process"));
    assert!(is_builtin_global("Buffer"));

    // User-defined names should NOT be built-in
    assert!(!is_builtin_global("myFunction"));
    assert!(!is_builtin_global("MyClass"));
    assert!(!is_builtin_global("handler"));
    assert!(!is_builtin_global("data"));
}

#[test]
fn test_goto_definition_multiple_builtin_globals_no_crash() {
    // Multiple built-in references in one file should all return None
    let source =
        "console.log(Array.from([1, 2, 3]));\nPromise.resolve(42);\nsetTimeout(() => {}, 100);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // console at (0, 0)
    let d1 = goto_def.get_definition(root, Position::new(0, 0));
    assert!(d1.is_none(), "console should return None");

    // Promise at (1, 0)
    let d2 = goto_def.get_definition(root, Position::new(1, 0));
    assert!(d2.is_none(), "Promise should return None");

    // setTimeout at (2, 0)
    let d3 = goto_def.get_definition(root, Position::new(2, 0));
    assert!(d3.is_none(), "setTimeout should return None");
}

