#[test]
fn test_goto_definition_nested_arrow_in_if_condition() {
    let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
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

    assert!(
        definitions.is_some(),
        "Should resolve nested arrow locals in condition"
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
fn test_goto_definition_nested_arrow_in_while_condition() {
    let source = "while ((() => {\n  const value = 1;\n  return value;\n})()) {}";
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

    assert!(
        definitions.is_some(),
        "Should resolve nested arrow locals in while condition"
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
fn test_goto_definition_nested_arrow_in_for_of_expression() {
    let source = "for (const item of (() => {\n  const value = 1;\n  return value;\n})()) {}";
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

    assert!(
        definitions.is_some(),
        "Should resolve nested arrow locals in for-of expression"
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
fn test_goto_definition_export_default_expression() {
    let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
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

    assert!(
        definitions.is_some(),
        "Should resolve locals in export default expression"
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
fn test_goto_definition_labeled_statement_local() {
    let source = "label: {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve locals inside labeled statement"
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
fn test_goto_definition_with_statement_local() {
    let source = "with (obj) {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve locals inside with statement"
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
fn test_goto_definition_var_hoisted_in_nested_block() {
    let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage before the declaration (line 1)
    let position = Position::new(1, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve hoisted var definition"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 3,
            "Definition should be on line 3"
        );
    }
}

#[test]
fn test_goto_definition_decorator_reference() {
    let source = "const deco = () => {};\n@deco\nclass Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'deco' usage in the decorator (line 1)
    let position = Position::new(1, 1);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(definitions.is_some(), "Should resolve decorator reference");
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_decorator_argument_local() {
    let source = "const deco = (cb) => cb();\n@deco(() => {\n  const value = 1;\n  return value;\n})\nclass Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage inside the decorator argument (line 3)
    let position = Position::new(3, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should resolve locals inside decorator arguments"
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
fn test_goto_definition_nested_arrow_in_object_literal() {
    let source = "const holder = { run: () => {\n  const value = 1;\n  return value;\n} };";
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

    assert!(
        definitions.is_some(),
        "Should resolve nested object literal locals"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Definition should be on line 1"
        );
    }
}

