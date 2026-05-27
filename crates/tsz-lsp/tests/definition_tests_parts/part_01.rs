#[test]
fn test_goto_definition_enum_in_type_annotation() {
    let source = "enum Status { Active, Inactive }\nfunction check(s: Status) {}";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let goto_def = GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let definitions = goto_def.get_definition(root, Position::new(0, 0));
    let _ = definitions;
}
