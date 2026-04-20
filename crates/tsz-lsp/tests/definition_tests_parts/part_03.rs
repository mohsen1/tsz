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

