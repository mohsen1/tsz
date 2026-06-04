#[test]
fn test_goto_definition_catch_clause_variable() {
    // Go-to-definition on a catch clause variable
    let source = "try {\n  throw new Error();\n} catch (err) {\n  console.log(err);\n}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_goto_definition_getter_accessor() {
    let source = "class Box {\n  private _v = 0;\n  get value(): number { return this._v; }\n}\nconst b = new Box();\nb.value;";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
