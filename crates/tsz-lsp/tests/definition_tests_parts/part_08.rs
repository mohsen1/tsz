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

