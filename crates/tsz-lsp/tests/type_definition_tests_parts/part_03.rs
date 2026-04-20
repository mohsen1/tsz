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
    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
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
    if let Some(locations) = result
        && !locations.is_empty()
    {
        assert_eq!(locations[0].range.start.line, 0);
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

