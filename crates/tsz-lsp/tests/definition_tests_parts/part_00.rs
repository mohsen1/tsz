#[test]
fn test_goto_definition_simple_variable() {
    // const x = 1;
    // x + 1;
    let source = "const x = 1;\nx + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'x' in "x + 1" (line 1, column 0)
    let position = Position::new(1, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should find the definition at "const x = 1"
    assert!(definitions.is_some(), "Should find definition for x");

    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        // The definition should be on line 0
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_type_reference() {
    let source = "type Foo = { value: string };\nconst x: Foo = { value: \"\" };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Foo' in the type annotation (line 1)
    let position = Position::new(1, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find definition for type reference"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_binding_pattern() {
    let source = "const { foo } = obj;\nfoo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage (line 1)
    let position = Position::new(1, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find definition for binding pattern name"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_parameter_binding_pattern() {
    let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage in the return (line 1)
    let position = Position::new(1, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find definition for parameter binding name"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_class_method_local() {
    let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find definition for method local"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 2,
            "Definition should be on line 2"
        );
    }
}

#[test]
fn test_goto_definition_class_method_name() {
    let source = "class Foo {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'method' name (line 1)
    let position = Position::new(1, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find definition for method name"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Definition should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_class_member_not_in_scope() {
    let source = "class Foo {\n  value = 1;\n  method() {\n    return value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "Class members should not resolve as lexical identifiers"
    );
}

#[test]
fn test_goto_definition_class_self_reference() {
    let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Foo' usage (line 2)
    let position = Position::new(2, 11);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve class name within class scope"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_class_expression_name() {
    let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Bar' usage (line 2)
    let position = Position::new(2, 11);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve class expression name in body"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_nested_arrow_in_conditional() {
    let source = "const handler = cond ? (() => {\n  const value = 1;\n  return value;\n}) : null;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(definitions.is_some(), "Should resolve nested arrow locals");
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Definition should be on line 1"
        );
    }
}

