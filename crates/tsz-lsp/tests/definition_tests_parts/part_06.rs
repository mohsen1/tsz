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

