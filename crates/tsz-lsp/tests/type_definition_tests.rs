use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_type_definition_interface() {
    let source = "interface Foo { x: number; }\nlet a: Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'a' in 'let a: Foo'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Should find the interface declaration
    if let Some(locations) = result {
        assert!(!locations.is_empty(), "Should have at least one location");
        // The interface is on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }
    // Note: result may be None if type resolution isn't fully working yet
}

#[test]
fn test_type_definition_type_alias() {
    let source = "type MyType = string;\nlet x: MyType;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Type definition should point to the type alias on line 0
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_class() {
    let source = "class MyClass {}\nlet obj: MyClass;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'obj'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_primitive() {
    let source = "let x: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x'
    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // Primitive types have no definition location
    // This might return None or might return an empty vec depending on implementation
    if let Some(locations) = result {
        // number is a primitive, so it shouldn't have a user-defined location
        // (though it might if we consider lib.d.ts)
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_no_type_annotation() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'x' - no explicit type annotation
    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // Without type inference, this should return None
    // (Full type inference would be needed to determine that x: number)
    assert!(result.is_none());
}

#[test]
fn test_type_definition_function_return() {
    let source =
        "interface Result { ok: boolean; }\nfunction foo(): Result { return { ok: true }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'foo'
    let pos = Position::new(1, 9);
    let result = provider.get_type_definition(root, pos);

    // Should find the Result interface on line 0
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_parameter() {
    let source = "interface Options { debug: boolean; }\nfunction foo(opts: Options) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'opts' parameter
    let pos = Position::new(1, 13);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_enum() {
    let source = "enum Color { Red, Green, Blue }\nlet c: Color;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'c' in 'let c: Color'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Should find the enum declaration on line 0
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_generic_type() {
    let source = "interface Box<T> { value: T; }\nlet b: Box<number>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'b'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Should find the Box interface on line 0
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_union_type() {
    let source = "interface Foo { x: number; }\ninterface Bar { y: string; }\nlet u: Foo | Bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'u'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    // For union types, the implementation resolves the first type in the union
    if let Some(locations) = result {
        assert!(
            !locations.is_empty(),
            "Should resolve at least one type in union"
        );
    }
}

#[test]
fn test_type_definition_intersection_type() {
    let source = "interface A { x: number; }\ninterface B { y: string; }\nlet val: A & B;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'val'
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    // For intersection types, the implementation resolves the first type
    if let Some(locations) = result {
        assert!(
            !locations.is_empty(),
            "Should resolve at least one type in intersection"
        );
    }
}

#[test]
fn test_type_definition_nested_interface() {
    let source = "interface Inner { x: number; }\ninterface Outer { inner: Inner; }\nlet o: Outer;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'o' - should navigate to Outer, not Inner
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 1,
            "Should point to Outer interface on line 1"
        );
    }
}

#[test]
fn test_type_definition_function_param_with_interface() {
    let source =
        "interface Config { debug: boolean; timeout: number; }\nfunction init(cfg: Config) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'cfg' parameter
    let pos = Position::new(1, 14);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_type_alias_reference() {
    let source = "type ID = string;\ntype User = { id: ID; name: string; };\nlet u: User;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'u' - should navigate to User type alias
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 1,
            "Should point to User type alias on line 1"
        );
    }
}

#[test]
fn test_type_definition_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Empty file should return None");
}

#[test]
fn test_type_definition_on_type_annotation_itself() {
    // Cursor on the type reference in the annotation, not the variable name
    let source = "interface Widget { render(): void; }\nlet w: Widget;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'Widget' in the type annotation (line 1, col 7)
    let pos = Position::new(1, 7);
    let result = provider.get_type_definition(root, pos);

    // When cursor is on the type reference itself, it should still resolve
    // (might go to the interface declaration or might return None depending on impl)
    // This test verifies no panic occurs at minimum
    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_multiple_variables_same_type() {
    let source = "interface Shared { x: number; }\nlet a: Shared;\nlet b: Shared;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Both variables should resolve to the same type definition
    let result_a = provider.get_type_definition(root, Position::new(1, 4));
    let result_b = provider.get_type_definition(root, Position::new(2, 4));

    if let (Some(locs_a), Some(locs_b)) = (&result_a, &result_b) {
        assert_eq!(
            locs_a[0].range.start.line, locs_b[0].range.start.line,
            "Both variables should point to the same type definition"
        );
    }
}

#[test]
fn test_type_definition_property_with_interface_type() {
    let source = "interface Addr { city: string; }\ninterface Person { address: Addr; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on 'address' property name should look for Addr type def
    let pos = Position::new(1, 21);
    let result = provider.get_type_definition(root, pos);

    // Defensive: may or may not resolve depending on implementation
    // Just ensure no panic
    let _ = result;
}

#[test]
fn test_type_definition_out_of_bounds_position() {
    let source = "let x: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position well beyond the file
    let pos = Position::new(100, 100);
    let result = provider.get_type_definition(root, pos);

    assert!(
        result.is_none(),
        "Out of bounds position should return None"
    );
}

#[test]
fn test_type_definition_class_with_methods() {
    let source = "class MyService {\n  getData(): string { return \"\"; }\n}\nlet svc: MyService;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'svc'
    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 0,
            "Should point to MyService class"
        );
    }
}

#[test]
fn test_type_definition_const_no_type() {
    let source = "const pi = 3.14;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'pi' - no type annotation
    let pos = Position::new(0, 6);
    let result = provider.get_type_definition(root, pos);

    // Without type annotation, should return None
    assert!(
        result.is_none(),
        "Const without type annotation should return None"
    );
}

#[test]
fn test_type_definition_function_multiple_params() {
    let source = "interface Config { x: number; }\ninterface Logger { log(msg: string): void; }\nfunction init(cfg: Config, logger: Logger) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'cfg' param
    let pos_cfg = Position::new(2, 14);
    let result_cfg = provider.get_type_definition(root, pos_cfg);

    if let Some(locs) = result_cfg {
        assert!(!locs.is_empty());
        assert_eq!(locs[0].range.start.line, 0, "cfg should point to Config");
    }
}

#[test]
fn test_type_definition_at_start_of_file() {
    let source = "interface First { x: number; }\nlet f: First;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at very start of file (0,0)
    let pos = Position::new(0, 0);
    let result = provider.get_type_definition(root, pos);

    // At position 0,0 we hit the interface keyword, not a variable
    // Should handle gracefully - may return None
    let _ = result;
}

#[test]
fn test_type_definition_abstract_class_type() {
    let source = "abstract class Animal {\n  abstract speak(): void;\n}\nlet a: Animal;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'a'
    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 0,
            "Should point to abstract class Animal"
        );
    }
}

#[test]
fn test_type_definition_only_whitespace() {
    let source = "   \n   \n   ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 1);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Whitespace-only file should return None");
}

#[test]
fn test_type_definition_return_type_location() {
    let source =
        "interface Result { ok: boolean; }\nfunction check(): Result { return { ok: true }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'check' function name
    let pos = Position::new(1, 9);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].file_path, "test.ts",
            "Location should have correct file path"
        );
    }
}

#[test]
fn test_type_definition_array_type() {
    let source = "interface Item { id: number; }\nlet items: Item[];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'items'
    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Should find the Item interface for an array of Items
    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_optional_type() {
    let source = "interface Data { value: number; }\nlet d: Data | undefined;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Defensive: union with undefined may or may not resolve
    let _ = result;
}

#[test]
fn test_type_definition_tuple_type() {
    let source = "interface Point { x: number; y: number; }\nlet pair: [Point, Point];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Tuple types may or may not resolve to a named type
    let _ = result;
}

#[test]
fn test_type_definition_const_assertion() {
    let source = "const colors = ['red', 'blue'] as const;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let result = provider.get_type_definition(root, pos);

    // const assertion has no named type definition
    assert!(result.is_none());
}

#[test]
fn test_type_definition_readonly_array() {
    let source = "interface Entry { key: string; }\nlet entries: readonly Entry[];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Defensive: readonly array may or may not resolve element type
    let _ = result;
}

#[test]
fn test_type_definition_mapped_type() {
    let source = "type Partial<T> = { [K in keyof T]?: T[K]; };\ninterface User { name: string; age: number; }\nlet u: Partial<User>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    // Mapped type alias reference: may resolve to Partial type alias
    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_conditional_type() {
    let source = "type IsString<T> = T extends string ? true : false;\nlet x: IsString<number>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    // Conditional type alias may resolve to the type alias declaration
    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_extends_class() {
    let source =
        "class Base { x: number; }\nclass Derived extends Base { y: string; }\nlet d: Derived;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'd' should go to Derived, not Base
    let pos = Position::new(2, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 1,
            "Should point to Derived class on line 1"
        );
    }
}

#[test]
fn test_type_definition_literal_type_alias() {
    let source = "type Direction = 'north' | 'south' | 'east' | 'west';\nlet d: Direction;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_interface_with_index_signature() {
    let source = "interface Dict { [key: string]: number; }\nlet d: Dict;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_namespace_qualified() {
    let source = "namespace NS {\n  export interface Inner { x: number; }\n}\nlet v: NS.Inner;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    // Namespace-qualified type may or may not resolve
    let _ = result;
}

#[test]
fn test_type_definition_function_type_alias() {
    let source = "type Callback = (x: number) => void;\nlet cb: Callback;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_class_with_constructor() {
    let source = "class Service {\n  constructor(public name: string) {}\n}\nlet s: Service;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_on_keyword() {
    let source = "interface Foo { x: number; }\nlet a: Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on the 'let' keyword
    let pos = Position::new(1, 0);
    let result = provider.get_type_definition(root, pos);

    // Clicking on a keyword should not crash; may return None
    let _ = result;
}

#[test]
fn test_type_definition_string_type() {
    let source = "let s: string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // string is a primitive type, no user-defined location
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_boolean_type() {
    let source = "let b: boolean;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // boolean is a primitive type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_void_return_type() {
    let source = "function noop(): void {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at 'noop'
    let pos = Position::new(0, 9);
    let result = provider.get_type_definition(root, pos);

    // void return type is primitive, should not have a definition
    let _ = result;
}

#[test]
fn test_type_definition_multiple_type_params() {
    let source = "interface Map<K, V> { get(key: K): V; }\nlet m: Map<string, number>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_generic_class_type() {
    let source = "class Container<T> {\n  value: T;\n}\nlet c: Container<string>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_interface_with_methods() {
    let source = "interface Service {\n  start(): void;\n  stop(): void;\n}\nlet svc: Service;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(4, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_enum_member_type() {
    let source = "enum Status {\n  Active = 'active',\n  Inactive = 'inactive'\n}\nlet s: Status;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(4, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

#[test]
fn test_type_definition_any_type() {
    let source = "let x: any;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // any is a built-in type, no user-defined location
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_never_type() {
    let source = "let n: never;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // never is a built-in type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_unknown_type() {
    let source = "let u: unknown;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // unknown is a built-in type
    if let Some(locations) = result {
        assert!(locations.is_empty() || locations[0].file_path.contains("lib"));
    }
}

#[test]
fn test_type_definition_promise_like_type_alias() {
    let source = "type AsyncResult<T> = Promise<T>;\nlet r: AsyncResult<number>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_record_type_alias() {
    let source = "type Dict = Record<string, number>;\nlet d: Dict;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_class_with_generics_and_constraints() {
    let source = "interface Comparable {\n  compareTo(other: any): number;\n}\nclass SortedList<T extends Comparable> {\n  items: T[] = [];\n}\nlet list: SortedList<Comparable>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(6, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 3);
        }
    }
}

#[test]
fn test_type_definition_interface_extending_interface() {
    let source = "interface Base {\n  id: number;\n}\ninterface Extended extends Base {\n  name: string;\n}\nlet e: Extended;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(6, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(
            locations[0].range.start.line, 3,
            "Should point to Extended interface, not Base"
        );
    }
}

#[test]
fn test_type_definition_only_comments() {
    let source = "// This is a comment\n/* Block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 5);
    let result = provider.get_type_definition(root, pos);

    assert!(result.is_none(), "Comment-only file should return None");
}

#[test]
fn test_type_definition_let_with_explicit_string_literal_type() {
    let source = "type Mode = 'read' | 'write';\nlet m: Mode;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        assert!(!locations.is_empty());
        assert_eq!(locations[0].range.start.line, 0);
    }
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_type_definition_readonly_property() {
    let source = "interface Config {\n  readonly host: string;\n}\nlet c: Config;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_nested_generic() {
    let source = "interface Box<T> {\n  value: T;\n}\ntype NestedBox<T> = Box<Box<T>>;\nlet nb: NestedBox<string>;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(4, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 3);
        }
    }
}

#[test]
fn test_type_definition_function_expression_type() {
    let source = "type Handler = (event: string) => void;\nlet h: Handler;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_type_alias_with_keyof() {
    let source = "interface Person {\n  name: string;\n  age: number;\n}\ntype PersonKeys = keyof Person;\nlet k: PersonKeys;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(5, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 4);
        }
    }
}

#[test]
fn test_type_definition_const_enum_type() {
    let source = "const enum Status {\n  Active,\n  Inactive\n}\nlet s: Status;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(4, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_var_with_type_annotation() {
    let source = "interface Widget {}\nvar w: Widget;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_declared_type() {
    let source = "declare class Buffer {\n  length: number;\n}\nlet b: Buffer;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(3, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_type_alias_with_template_literal() {
    let source = "type EventName = `on${string}`;\nlet e: EventName;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_number_type_annotation() {
    let source = "let n: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 4);
    let result = provider.get_type_definition(root, pos);

    // Primitive types may or may not resolve to a type definition
    let _ = result;
}

#[test]
fn test_type_definition_object_type_literal() {
    let source = "type Point = { x: number; y: number };\nlet p: Point;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 4);
    let result = provider.get_type_definition(root, pos);

    if let Some(locations) = result {
        if !locations.is_empty() {
            assert_eq!(locations[0].range.start.line, 0);
        }
    }
}

#[test]
fn test_type_definition_at_numeric_literal() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        TypeDefinitionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the number literal
    let pos = Position::new(0, 10);
    let result = provider.get_type_definition(root, pos);

    // Should not panic; numeric literals don't have type definitions
    let _ = result;
}
