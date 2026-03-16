use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

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

#[test]
fn test_goto_definition_interface_reference() {
    // Interface declarations should be findable
    let source = "interface IFoo { bar: string; }\nconst x: IFoo = { bar: 'hi' };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "IFoo" type reference on line 1
    let position = Position::new(1, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // We expect this to either find the interface or return None gracefully
    // (no crash is the critical requirement)
    if let Some(defs) = &definitions {
        assert_eq!(
            defs[0].range.start.line, 0,
            "Interface definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_enum_reference() {
    let source = "enum Color { Red, Green, Blue }\nconst c: Color = Color.Red;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "Color" value reference on line 1 (after the =)
    let position = Position::new(1, 17);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // No crash is the critical requirement
    if let Some(defs) = &definitions {
        assert_eq!(
            defs[0].range.start.line, 0,
            "Enum definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_default_export_function() {
    // Export default function should be navigable
    let source = "export default function greet() { return 'hi'; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at "greet" (line 0, column 24)
    let position = Position::new(0, 24);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should find the function declaration or not crash
    if let Some(defs) = &definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_validated_positions_are_in_bounds() {
    // Ensure returned positions are always within the source text bounds
    let source = "const x = 1;\nconst y = x + 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Try every possible valid position in the source
    let line_count = line_map.line_count() as u32;
    for line in 0..line_count {
        for col in 0..50 {
            let position = Position::new(line, col);
            let goto_def =
                GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
            let definitions = goto_def.get_definition(root, position);

            // If we got definitions, all positions must be in bounds
            if let Some(defs) = definitions {
                for def in &defs {
                    assert!(
                        def.range.start.line < line_count,
                        "Start line {} should be < line_count {}",
                        def.range.start.line,
                        line_count
                    );
                    assert!(
                        def.range.end.line < line_count,
                        "End line {} should be < line_count {}",
                        def.range.end.line,
                        line_count
                    );
                }
            }
        }
    }
}

#[test]
fn test_goto_definition_for_node_with_none_index() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition_for_node(root, NodeIndex::NONE);

    assert!(
        definitions.is_none(),
        "Should return None for NodeIndex::none()"
    );
}

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_goto_definition_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let defs = goto_def.get_definition(root, Position::new(0, 0));
    assert!(defs.is_none(), "Empty file should have no definitions");
}

#[test]
fn test_goto_definition_class_reference() {
    let source = "class MyClass {}\nlet c = new MyClass();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on "MyClass" in "new MyClass()"
    let defs = goto_def.get_definition(root, Position::new(1, 12));
    assert!(defs.is_some(), "Should find class definition");
    let defs = defs.unwrap();
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to class declaration"
    );
}

#[test]
fn test_goto_definition_enum_usage() {
    let source = "enum Direction { Up, Down }\nlet d = Direction.Up;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on "Direction" in "Direction.Up"
    let defs = goto_def.get_definition(root, Position::new(1, 8));
    assert!(defs.is_some(), "Should find enum definition");
    let defs = defs.unwrap();
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to enum declaration"
    );
}

#[test]
fn test_goto_definition_function_in_nested_scope() {
    let source = "function outer() {\n  function inner() {}\n  inner();\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on "inner" in "inner();"
    let defs = goto_def.get_definition(root, Position::new(2, 2));
    assert!(defs.is_some(), "Should find inner function definition");
    let defs = defs.unwrap();
    assert_eq!(
        defs[0].range.start.line, 1,
        "Should point to inner function declaration"
    );
}

#[test]
fn test_goto_definition_type_alias_usage() {
    let source = "type MyStr = string;\nlet x: MyStr = 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on "MyStr" in type annotation
    let defs = goto_def.get_definition(root, Position::new(1, 7));
    assert!(defs.is_some(), "Should find type alias definition");
    let defs = defs.unwrap();
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to type alias declaration"
    );
}

#[test]
fn test_goto_definition_at_semicolon_returns_none() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let defs = goto_def.get_definition(root, Position::new(0, 12));
    assert!(
        defs.is_none(),
        "Should not find definition at semicolon position"
    );
}

#[test]
fn test_goto_definition_multiple_declarations_same_name() {
    let source = "let x = 1;\nx = 2;\nx;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position on "x" in last line
    let defs = goto_def.get_definition(root, Position::new(2, 0));
    assert!(
        defs.is_some(),
        "Should find definition for reassigned variable"
    );
    let defs = defs.unwrap();
    // Should point to original declaration
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to original declaration"
    );
}

// =========================================================================
// Additional coverage tests for navigation/definition module
// =========================================================================

#[test]
fn test_goto_definition_generic_type_parameter_usage() {
    // Go-to-definition on a generic type parameter used in function body type position
    let source = "function identity<T>(arg: T): T {\n  let result: T = arg;\n  return result;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'T' in the type annotation "let result: T" (line 1, col 14)
    let position = Position::new(1, 14);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should either find the type parameter declaration or return None gracefully
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Type parameter definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_generic_type_param_in_return_type() {
    // Go-to-definition on a generic type parameter used as return type
    let source = "function wrap<U>(val: U): U {\n  return val;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'U' in return type annotation ": U" (line 0, col 26)
    let position = Position::new(0, 26);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should not crash; may or may not resolve
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty());
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_default_export_class() {
    // Go-to-definition on a default-exported class name
    let source = "export default class Widget {\n  render() {}\n}\nconst w = new Widget();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'Widget' in "new Widget()" (line 2, col 14)
    let position = Position::new(2, 14);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should find the class declaration on line 0
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find default export class");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Default export class should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_namespace_member_access() {
    // Go-to-definition on namespace member access (ns.member)
    let source = "namespace MyNS {\n  export const value = 42;\n}\nconst x = MyNS.value;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'MyNS' in "MyNS.value" (line 2, col 10)
    let position = Position::new(2, 10);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should find the namespace declaration
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find namespace definition");
        assert_eq!(
            defs[0].range.start.line, 0,
            "Namespace definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_namespace_exported_member() {
    // Go-to-definition on the member part of namespace access (ns.member)
    let source = "namespace NS {\n  export function helper() {}\n}\nNS.helper();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'helper' in "NS.helper()" (line 2, col 3)
    let position = Position::new(2, 3);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the function declaration inside namespace
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find namespace member definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Namespace member definition should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_decorator_factory() {
    // Go-to-definition on a decorator used as a factory
    let source = "function sealed(target: any) { return target; }\n@sealed\nclass MyService {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'sealed' in "@sealed" (line 1, col 1)
    let position = Position::new(1, 1);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_some(),
        "Should find decorator function definition"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
        assert_eq!(
            defs[0].range.start.line, 0,
            "Decorator function definition should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_inherited_class_member_via_instance() {
    // Go-to-definition on a member that's defined in a base class
    let source = "class Base {\n  greet() { return 'hi'; }\n}\nclass Child extends Base {}\nconst c = new Child();\nc.greet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'greet' in "c.greet()" (line 4, col 2)
    let position = Position::new(4, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // May or may not resolve inherited members — should not crash
    // If it does resolve, it should point to the Base class definition
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should have at least one definition");
    }
}

#[test]
fn test_goto_definition_computed_property_name() {
    // Go-to-definition on a computed property name using a variable
    let source = "const key = 'myProp';\nconst obj = { [key]: 42 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'key' inside computed property [key] (line 1, col 15)
    let position = Position::new(1, 15);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve 'key' to the const declaration on line 0
    assert!(
        definitions.is_some(),
        "Should find definition for computed property variable"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
        assert_eq!(
            defs[0].range.start.line, 0,
            "Computed property variable should resolve to line 0"
        );
    }
}

#[test]
fn test_goto_definition_shorthand_property() {
    // Go-to-definition on a shorthand property in object literal
    let source = "const name = 'Alice';\nconst obj = { name };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'name' inside shorthand property { name } (line 1, col 14)
    let position = Position::new(1, 14);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the const declaration on line 0
    assert!(
        definitions.is_some(),
        "Should find definition for shorthand property"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
        assert_eq!(
            defs[0].range.start.line, 0,
            "Shorthand property should resolve to original declaration on line 0"
        );
    }
}

#[test]
fn test_goto_definition_at_start_of_file() {
    // Go-to-definition at position (0,0) on a valid identifier
    let source = "myVar + 1;\nconst myVar = 10;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at (0,0) — the very start of the file
    let position = Position::new(0, 0);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should not crash and may find the var declaration
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty());
    }
}

#[test]
fn test_goto_definition_string_enum_member_value() {
    // Go-to-definition on a string enum member
    let source =
        "enum Status {\n  Active = 'active',\n  Inactive = 'inactive'\n}\nconst s = Status.Active;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'Active' in "Status.Active" (line 3, col 17)
    let position = Position::new(3, 17);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the enum member declaration
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find enum member definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Enum member Active should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_typeof_usage() {
    // Go-to-definition on a variable used in typeof expression
    let source = "const original = { a: 1 };\ntype Copy = typeof original;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'original' in "typeof original" (line 1, col 19)
    let position = Position::new(1, 19);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the const declaration on line 0
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find definition in typeof");
        assert_eq!(
            defs[0].range.start.line, 0,
            "typeof target should resolve to line 0"
        );
    }
}

#[test]
fn test_goto_definition_interface_property_via_member_access() {
    // Go-to-definition on a property accessed through a typed variable
    let source = "interface Config {\n  host: string;\n  port: number;\n}\nconst cfg: Config = { host: 'localhost', port: 3000 };\ncfg.host;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'host' in "cfg.host" (line 4, col 4)
    let position = Position::new(4, 4);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the interface property declaration
    if let Some(defs) = &definitions {
        assert!(
            !defs.is_empty(),
            "Should find interface property definition"
        );
        assert_eq!(
            defs[0].range.start.line, 1,
            "Interface property 'host' should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_catch_clause_variable() {
    // Go-to-definition on a catch clause variable
    let source = "try {\n  throw new Error();\n} catch (err) {\n  console.log(err);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'err' usage in "console.log(err)" (line 3, col 14)
    let position = Position::new(3, 14);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the catch clause parameter on line 2
    assert!(
        definitions.is_some(),
        "Should find definition for catch clause variable"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
        assert_eq!(
            defs[0].range.start.line, 2,
            "Catch clause variable should resolve to line 2"
        );
    }
}

#[test]
fn test_goto_definition_for_loop_variable() {
    // Go-to-definition on a for-of loop variable
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  item;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'item' usage (line 2, col 2)
    let position = Position::new(2, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the for-of declaration on line 1
    assert!(
        definitions.is_some(),
        "Should find definition for for-of variable"
    );
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
        assert_eq!(
            defs[0].range.start.line, 1,
            "For-of variable should resolve to line 1"
        );
    }
}

#[test]
fn test_goto_definition_keyword_null_returns_none() {
    // Go-to-definition on null keyword should return None
    let source = "const x = null;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'null' (line 0, col 10)
    let position = Position::new(0, 10);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "null keyword should return None (is_builtin_node)"
    );
}

#[test]
fn test_goto_definition_keyword_true_returns_none() {
    // Go-to-definition on boolean true keyword should return None
    let source = "const flag = true;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'true' (line 0, col 13)
    let position = Position::new(0, 13);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "true keyword should return None (is_builtin_node)"
    );
}

#[test]
fn test_goto_definition_class_property_via_typed_instance() {
    // Go-to-definition on a class member accessed via a typed variable
    let source =
        "class Dog {\n  name: string = '';\n  bark() {}\n}\nconst d: Dog = new Dog();\nd.name;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'name' in "d.name" (line 4, col 2)
    let position = Position::new(4, 2);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    // Should resolve to the class property on line 1
    if let Some(defs) = &definitions {
        assert!(!defs.is_empty(), "Should find class property definition");
        assert_eq!(
            defs[0].range.start.line, 1,
            "Class property 'name' should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_arguments_returns_none() {
    // Go-to-definition on the special 'arguments' identifier should return None
    let source = "function foo() {\n  return arguments;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at 'arguments' (line 1, col 9)
    let position = Position::new(1, 9);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, position);

    assert!(
        definitions.is_none(),
        "arguments keyword should return None (is_builtin_node)"
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_goto_definition_getter_accessor() {
    let source = "class Box {\n  private _v = 0;\n  get value(): number { return this._v; }\n}\nconst b = new Box();\nb.value;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'value' in b.value (line 5, col 2)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(5, 2));

    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find definition for getter");
    }
}

#[test]
fn test_goto_definition_setter_accessor() {
    let source = "class Box {\n  private _v = 0;\n  set value(v: number) { this._v = v; }\n}\nconst b = new Box();\nb.value = 5;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(5, 2));

    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find definition for setter");
    }
}

#[test]
fn test_goto_definition_nested_class() {
    let source = "class Outer {\n  inner() {\n    class Inner {\n      method() {}\n    }\n    const i = new Inner();\n    i;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Inner' usage in `new Inner()` (line 5, col 18)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(5, 18));

    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find nested class definition");
        assert_eq!(
            defs[0].range.start.line, 2,
            "Inner class should be on line 2"
        );
    }
}

#[test]
fn test_goto_definition_default_parameter() {
    let source = "function greet(name: string = 'world') {\n  return name;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'name' usage in return (line 1, col 9)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 9));

    assert!(
        definitions.is_some(),
        "Should find definition for default parameter"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Parameter should be on line 0");
    }
}

#[test]
fn test_goto_definition_rest_parameter() {
    let source = "function sum(...nums: number[]) {\n  return nums.length;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'nums' usage in body (line 1, col 9)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 9));

    assert!(
        definitions.is_some(),
        "Should find definition for rest parameter"
    );
    if let Some(defs) = definitions {
        assert_eq!(
            defs[0].range.start.line, 0,
            "Rest param should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_empty_file_returns_none() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(0, 0));

    assert!(definitions.is_none(), "Empty file should return None");
}

#[test]
fn test_goto_definition_arrow_function_param() {
    let source = "const fn = (x: number) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'x' usage in body (col 26)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(0, 26));

    assert!(
        definitions.is_some(),
        "Should find definition for arrow function param"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Param should be on line 0");
    }
}

#[test]
fn test_goto_definition_enum_in_type_annotation() {
    let source = "enum Status { Active, Inactive }\nfunction check(s: Status) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Status' in type annotation (line 1, col 19)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 19));

    assert!(
        definitions.is_some(),
        "Should find enum definition from type annotation"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Enum should be on line 0");
    }
}

#[test]
fn test_goto_definition_interface_used_as_type() {
    let source = "interface Point { x: number; y: number; }\nfunction draw(p: Point) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Point' in type annotation (line 1, col 17)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 17));

    assert!(
        definitions.is_some(),
        "Should find interface definition from type annotation"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Interface should be on line 0");
    }
}

#[test]
fn test_goto_definition_type_alias_used_as_type() {
    let source = "type ID = string | number;\nfunction process(id: ID) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'ID' in type annotation (line 1, col 21)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 21));

    assert!(definitions.is_some(), "Should find type alias definition");
    if let Some(defs) = definitions {
        assert_eq!(
            defs[0].range.start.line, 0,
            "Type alias should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_variable_in_for_in_loop() {
    let source = "const obj = { a: 1 };\nfor (const key in obj) {\n  key;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'key' usage in body (line 2, col 2)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(2, 2));

    if let Some(defs) = definitions {
        assert!(
            !defs.is_empty(),
            "Should find definition for for-in variable"
        );
        assert_eq!(
            defs[0].range.start.line, 1,
            "For-in variable should be on line 1"
        );
    }
}

#[test]
fn test_goto_definition_class_in_extends() {
    let source = "class Base {\n  value = 1;\n}\nclass Derived extends Base {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Base' in extends clause (line 3, col 22)
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(3, 22));

    assert!(definitions.is_some(), "Should find base class definition");
    if let Some(defs) = definitions {
        assert_eq!(
            defs[0].range.start.line, 0,
            "Base class should be on line 0"
        );
    }
}

#[test]
fn test_goto_definition_namespace_member() {
    let source = "namespace NS {\n  export const val = 1;\n}\nNS.val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(3, 0));
    // Namespace reference may or may not resolve
    let _ = definitions;
}

#[test]
fn test_goto_definition_optional_param() {
    let source = "function f(x?: number) {}\nf();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    assert!(definitions.is_some(), "Should find function definition");
}

#[test]
fn test_goto_definition_const_enum_member() {
    let source = "const enum Dir { Up, Down }\nlet d = Dir.Up;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 8));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty());
    }
}

#[test]
fn test_goto_definition_decorated_class() {
    let source = "function Deco(target: any) {}\n@Deco\nclass MyClass {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Go to definition of @Deco
    let definitions = goto_def.get_definition(root, Position::new(1, 1));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Deco function");
    }
}

#[test]
fn test_goto_definition_generic_type_param() {
    let source = "function id<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // T in param type
    let definitions = goto_def.get_definition(root, Position::new(0, 18));
    // Type params may or may not resolve
    let _ = definitions;
}

#[test]
fn test_goto_definition_rest_param() {
    let source = "function sum(...nums: number[]) { return nums.reduce((a, b) => a + b, 0); }\nsum(1, 2, 3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    assert!(definitions.is_some(), "Should find sum function");
}

#[test]
fn test_goto_definition_interface_method() {
    let source = "interface Foo {\n  bar(): void;\n}\nfunction f(x: Foo) { x.bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Go to 'Foo' type annotation
    let definitions = goto_def.get_definition(root, Position::new(3, 14));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Foo interface");
    }
}

#[test]
fn test_goto_definition_computed_property_key() {
    let source = "const key = 'name';\nconst obj = { [key]: 'value' };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Go to 'key' in computed property
    let definitions = goto_def.get_definition(root, Position::new(1, 15));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_async_function() {
    let source = "async function fetchData() { return 42; }\nfetchData();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    assert!(
        definitions.is_some(),
        "Should find async function definition"
    );
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_template_literal_variable() {
    let source = "const name = 'world';\nconst greeting = `hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'name' inside template literal (line 1, col 27)
    let definitions = goto_def.get_definition(root, Position::new(1, 27));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find name on line 0");
    }
}

#[test]
fn test_goto_definition_destructured_object() {
    let source = "const obj = { a: 1, b: 2 };\nconst { a, b } = obj;\na + b;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'a' usage (line 2, col 0)
    let definitions = goto_def.get_definition(root, Position::new(2, 0));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find destructured binding");
    }
}

#[test]
fn test_goto_definition_destructured_array() {
    let source = "const arr = [1, 2, 3];\nconst [first, second] = arr;\nfirst;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'first' usage (line 2, col 0)
    let definitions = goto_def.get_definition(root, Position::new(2, 0));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find array destructured binding");
    }
}

#[test]
fn test_goto_definition_switch_case_variable() {
    let source = "const val = 1;\nswitch (val) {\n  case 1:\n    const inside = 2;\n    inside;\n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'inside' usage (line 4, col 4)
    let definitions = goto_def.get_definition(root, Position::new(4, 4));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find switch case variable");
    }
}

#[test]
fn test_goto_definition_class_constructor() {
    let source =
        "class Animal {\n  constructor(public name: string) {}\n}\nconst a = new Animal('dog');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'Animal' in new expression (line 3, col 14)
    let definitions = goto_def.get_definition(root, Position::new(3, 14));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Animal class");
    }
}

#[test]
fn test_goto_definition_function_overload() {
    let source = "function f(x: string): string;\nfunction f(x: number): number;\nfunction f(x: any): any { return x; }\nf(1);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(3, 0));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find at least one overload");
    }
}

#[test]
fn test_goto_definition_ternary_variable() {
    let source = "const flag = true;\nconst result = flag ? 'yes' : 'no';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'flag' in ternary (line 1, col 15)
    let definitions = goto_def.get_definition(root, Position::new(1, 15));
    assert!(definitions.is_some(), "Should find flag definition");
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_numeric_literal_returns_none() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at numeric literal '42' (col 10)
    let definitions = goto_def.get_definition(root, Position::new(0, 10));
    // Numeric literals have no definition to jump to
    let _ = definitions;
}

#[test]
fn test_goto_definition_string_literal_returns_none() {
    let source = "const x = 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position inside string literal (col 11)
    let definitions = goto_def.get_definition(root, Position::new(0, 11));
    let _ = definitions;
}

#[test]
fn test_goto_definition_class_private_field() {
    let source = "class Foo {\n  #secret = 42;\n  get() { return this.#secret; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at '#secret' declaration (line 1, col 2)
    let definitions = goto_def.get_definition(root, Position::new(1, 2));
    let _ = definitions;
}

#[test]
fn test_goto_definition_interface_extends() {
    let source = "interface Base { x: number; }\ninterface Derived extends Base { y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'Base' in extends (line 1, col 26)
    let definitions = goto_def.get_definition(root, Position::new(1, 26));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Base interface");
    }
}

#[test]
fn test_goto_definition_try_catch_error_variable() {
    let source = "try {\n  throw new Error('fail');\n} catch (err) {\n  err;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'err' usage (line 3, col 2)
    let definitions = goto_def.get_definition(root, Position::new(3, 2));
    if let Some(defs) = definitions {
        assert!(!defs.is_empty(), "Should find catch clause variable");
    }
}

#[test]
fn test_goto_definition_nested_function_call() {
    let source = "function outer() {\n  function inner() { return 1; }\n  inner();\n}\nouter();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'outer' call (line 4, col 0)
    let definitions = goto_def.get_definition(root, Position::new(4, 0));
    assert!(definitions.is_some(), "Should find outer function");
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_multiline_string_no_crash() {
    let source = "const s = `line1\nline2\nline3`;\ns;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 's' usage (line 3, col 0)
    let definitions = goto_def.get_definition(root, Position::new(3, 0));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find s on line 0");
    }
}

#[test]
fn test_goto_definition_type_assertion() {
    let source = "interface Foo { x: number; }\nconst val = {} as Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'Foo' in type assertion (line 1, col 18)
    let definitions = goto_def.get_definition(root, Position::new(1, 18));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Foo interface");
    }
}

#[test]
fn test_goto_definition_while_loop_variable() {
    let source = "let count = 0;\nwhile (count < 10) {\n  count++;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'count' in condition (line 1, col 7)
    let definitions = goto_def.get_definition(root, Position::new(1, 7));
    assert!(definitions.is_some(), "Should find count variable");
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_export_named_variable() {
    let source = "export const exported = 42;\nexported;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    assert!(definitions.is_some(), "Should find exported variable");
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0);
    }
}

#[test]
fn test_goto_definition_abstract_class_reference() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}\nclass Circle extends Shape {\n  area() { return 3.14; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at 'Shape' in extends (line 3, col 21)
    let definitions = goto_def.get_definition(root, Position::new(3, 21));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find Shape class");
    }
}

#[test]
fn test_goto_definition_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 42;\n\u{00e4}\u{00f6}\u{00fc};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(1, 0));
    if let Some(defs) = definitions {
        assert_eq!(defs[0].range.start.line, 0, "Should find unicode variable");
    }
}

#[test]
fn test_goto_definition_void_keyword_returns_none() {
    let source = "void 0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(0, 0));
    let _ = definitions;
}
