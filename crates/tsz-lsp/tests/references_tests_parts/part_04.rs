#[test]
fn test_find_references_in_nested_scope() {
    let source = "const x = 1;\nfunction foo() {\n  const y = x;\n  return y + x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Find references for 'x' on line 0
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for x");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + 2 usages in nested scope, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_type_alias() {
    let source = "type ID = string;\nlet userId: ID;\nlet groupId: ID;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 0));

    assert!(refs.is_none(), "Should not find references in empty file");
}

#[test]
fn test_rename_locations_function() {
    let source = "function greet() {}\ngreet();\ngreet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let locs = find_refs.find_rename_locations(root, Position::new(0, 9));

    assert!(locs.is_some(), "Should find rename locations for function");
    let locs = locs.unwrap();
    assert!(
        locs.len() >= 3,
        "Should find declaration + 2 calls, got {}",
        locs.len()
    );
}

// =========================================================================
// Additional coverage tests for navigation/references module
// =========================================================================

#[test]
fn test_find_references_type_alias_usage() {
    // Type alias declared once, used in multiple annotation positions
    let source = "type Pair<A, B> = [A, B];\nlet p: Pair<number, string>;\nfunction take(x: Pair<boolean, boolean>) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Pair' declaration (line 0, col 5)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias Pair");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + 2 type annotation usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_generic_type_parameter() {
    // Generic type parameter T used in parameter and return type
    let source = "function identity<T>(value: T): T {\n  return value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'T' in the type parameter list (line 0, col 18)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 18));

    assert!(
        refs.is_some(),
        "Should find references for generic type parameter T"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find T declaration + usages in param/return type, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_namespace_member() {
    // Namespace with an exported member used outside
    let source = "namespace Shapes {\n  export const PI = 3.14;\n}\nlet x = Shapes.PI;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Shapes' on line 0
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(
        refs.is_some(),
        "Should find references for namespace Shapes"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find namespace declaration + qualified usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_enum_member_access() {
    // Enum member referenced via qualified access
    let source =
        "enum Direction {\n  Up,\n  Down,\n}\nlet d = Direction.Up;\nif (d === Direction.Down) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Direction' on line 0, col 5
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for enum Direction");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find enum declaration + qualified member accesses, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_destructured_variable() {
    // Destructured variable used in multiple places
    let source = "const { alpha, beta } = obj;\nalpha + beta;\nconsole.log(alpha);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'alpha' usage on line 1, col 0
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 0));

    assert!(
        refs.is_some(),
        "Should find references for destructured variable alpha"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find binding + 2 usages of alpha, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_rest_parameter() {
    // Rest parameter used inside the function body
    let source = "function sum(...nums: number[]) {\n  return nums.reduce((a, b) => a + b, 0);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'nums' in the parameter (line 0, col 16)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));

    assert!(
        refs.is_some(),
        "Should find references for rest parameter nums"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find rest param declaration + body usage, got {}",
        refs.len()
    );
}

