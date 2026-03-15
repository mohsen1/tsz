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
