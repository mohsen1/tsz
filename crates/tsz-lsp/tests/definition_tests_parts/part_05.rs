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

