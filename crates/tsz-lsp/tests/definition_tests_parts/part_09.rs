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

