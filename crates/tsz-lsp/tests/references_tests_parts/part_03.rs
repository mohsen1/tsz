#[test]
fn test_detailed_refs_interface_declaration_is_definition() {
    // `interface Foo { x: number; } let a: Foo;`
    let source = "interface Foo {\n  x: number;\n}\nlet a: Foo;";
    let refs = get_detailed_refs(source, "test.ts", 0, 10);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Interface declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Interface declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_enum_declaration_is_definition() {
    // `enum Color { Red } let c = Color.Red;`
    let source = "enum Color {\n  Red\n}\nlet c = Color.Red;";
    let refs = get_detailed_refs(source, "test.ts", 0, 5);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Enum declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Enum declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_type_alias_is_definition() {
    // `type Foo = number; let x: Foo;`
    let source = "type Foo = number;\nlet x: Foo;";
    let refs = get_detailed_refs(source, "test.ts", 0, 5);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Type alias declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Type alias declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_read_in_expression_not_write() {
    // `let x = 1; let y = x + 2;`
    // x in the expression `x + 2` should be isWriteAccess=false
    let source = "let x = 1;\nlet y = x + 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let expr_ref = refs
        .iter()
        .find(|r| r.location.range.start.line == 1 && !r.is_definition);
    assert!(expr_ref.is_some(), "Should have a read usage ref on line 1");
    let expr_ref = expr_ref.unwrap();
    assert!(
        !expr_ref.is_write_access,
        "Read in expression should not be write access"
    );
}

// =========================================================================
// Tests for find_rename_locations
// =========================================================================

#[test]
fn test_rename_locations_simple() {
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let locations = find_refs.find_rename_locations(root, position);

    assert!(locations.is_some(), "Should find rename locations for x");
    let locs = locations.unwrap();
    assert!(
        locs.len() >= 2,
        "Should find at least 2 rename locations (declaration + usages)"
    );

    // Each location should have a line_text
    for loc in &locs {
        assert!(
            !loc.line_text.is_empty(),
            "Rename location should have non-empty line_text"
        );
    }
}

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_find_references_class_name() {
    let source = "class Animal {}\nlet a = new Animal();\nlet b: Animal;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for class");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_interface_name() {
    let source = "interface Foo { x: number; }\nlet a: Foo;\nlet b: Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(refs.is_some(), "Should find references for interface");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_enum_name() {
    let source = "enum Color { Red, Green }\nlet c: Color;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for enum");
}

#[test]
fn test_find_references_no_results_for_unknown_position() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at the semicolon
    let refs = find_refs.find_references(root, Position::new(0, 12));

    assert!(
        refs.is_none(),
        "Should not find references for semicolon position"
    );
}

#[test]
fn test_find_references_parameter_in_function() {
    let source = "function foo(param: number) {\n  return param * 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 13));

    assert!(
        refs.is_some(),
        "Should find references for function parameter"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find param declaration + usage, got {}",
        refs.len()
    );
}

