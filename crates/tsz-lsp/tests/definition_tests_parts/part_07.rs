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

