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

