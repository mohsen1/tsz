#[test]
fn test_find_references_catch_clause_parameter() {
    // Catch clause parameter used inside the catch block
    let source =
        "try {\n  throw new Error();\n} catch (err) {\n  console.log(err);\n  throw err;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'err' in catch clause (line 2, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 9));

    assert!(
        refs.is_some(),
        "Should find references for catch clause parameter err"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find catch param + usages in catch block, got {}",
        refs.len()
    );
}

#[test]
fn test_detailed_refs_write_vs_read_locations() {
    // Tests that we correctly distinguish write and read locations:
    // line 0: let x = 1;       (write + definition)
    // line 1: x = 2;           (write, not definition)
    // line 2: console.log(x);  (read)
    // line 3: x++;             (write)
    let source = "let x = 1;\nx = 2;\nconsole.log(x);\nx++;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 3,
        "Should find at least 3 references, got {}",
        refs.len()
    );

    // Declaration (line 0) should be write + definition
    let decl = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl.is_some(), "Should have declaration ref on line 0");
    let decl = decl.unwrap();
    assert!(decl.is_write_access, "Declaration should be write access");
    assert!(decl.is_definition, "Declaration should be definition");

    // Assignment (line 1) should be write but not definition
    let assign = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(assign.is_some(), "Should have assignment ref on line 1");
    let assign = assign.unwrap();
    assert!(assign.is_write_access, "Assignment should be write access");
    assert!(!assign.is_definition, "Assignment should not be definition");

    // Read (line 2) should be neither write nor definition
    let read = refs.iter().find(|r| r.location.range.start.line == 2);
    if let Some(read) = read {
        assert!(
            !read.is_write_access,
            "Read usage should not be write access"
        );
        assert!(!read.is_definition, "Read usage should not be definition");
    }
}

#[test]
fn test_find_references_same_name_different_scopes() {
    // Same variable name 'x' declared in different scopes should NOT
    // cross-reference between scopes.
    let source = "function a() {\n  const x = 1;\n  return x;\n}\nfunction b() {\n  const x = 2;\n  return x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Find references for 'x' in function a (line 1, col 8)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs_a = find_refs.find_references(root, Position::new(1, 8));

    assert!(
        refs_a.is_some(),
        "Should find references for x in function a"
    );
    let refs_a = refs_a.unwrap();

    // References for x in scope a should only appear on lines 1 and 2
    for loc in &refs_a {
        assert!(
            loc.range.start.line <= 2,
            "Reference for x in function a should not appear on line {}, which is in function b",
            loc.range.start.line
        );
    }

    // Find references for 'x' in function b (line 5, col 8)
    let refs_b = find_refs.find_references(root, Position::new(5, 8));

    assert!(
        refs_b.is_some(),
        "Should find references for x in function b"
    );
    let refs_b = refs_b.unwrap();

    // References for x in scope b should only appear on lines 5 and 6
    for loc in &refs_b {
        assert!(
            loc.range.start.line >= 4,
            "Reference for x in function b should not appear on line {}, which is in function a",
            loc.range.start.line
        );
    }

    // The two sets of references should not overlap
    assert!(
        refs_a.len() <= 3 && refs_b.len() <= 3,
        "Each scoped x should have at most 3 refs (decl + usage + return), got a={}, b={}",
        refs_a.len(),
        refs_b.len()
    );
}

#[test]
fn test_find_references_overloaded_function() {
    // Overloaded function: multiple signatures + implementation
    let source = "function process(x: number): number;\nfunction process(x: string): string;\nfunction process(x: any): any {\n  return x;\n}\nprocess(1);\nprocess(\"hello\");";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'process' on the implementation (line 2, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 9));

    assert!(
        refs.is_some(),
        "Should find references for overloaded function process"
    );
    let refs = refs.unwrap();
    // Should find at least the implementation + 2 call sites
    assert!(
        refs.len() >= 3,
        "Should find overload signatures/impl + call sites, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_for_in_loop_variable() {
    // for-in loop variable
    let source = "const obj = { a: 1, b: 2 };\nfor (const key in obj) {\n  console.log(key);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'key' usage inside the loop body (line 2, col 14)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 14));

    assert!(
        refs.is_some(),
        "Should find references for for-in loop variable key"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find loop variable declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_for_of_loop_variable() {
    // for-of loop variable
    let source = "const arr = [1, 2, 3];\nfor (const item of arr) {\n  console.log(item);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'item' usage inside the loop body (line 2, col 14)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 14));

    assert!(
        refs.is_some(),
        "Should find references for for-of loop variable item"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find loop variable declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_detailed_refs_postfix_increment_is_write() {
    // x++ should be detected as a write access
    let source = "let counter = 0;\ncounter++;\nconsole.log(counter);";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The postfix increment (line 1) should be a write access
    let inc_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    if let Some(inc_ref) = inc_ref {
        assert!(
            inc_ref.is_write_access,
            "Postfix increment should be a write access"
        );
        assert!(
            !inc_ref.is_definition,
            "Postfix increment should not be a definition"
        );
    }
}

#[test]
fn test_find_references_array_destructured_variable() {
    // Array destructuring
    let source = "const [first, second] = [1, 2];\nconsole.log(first);\nlet x = second + first;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'first' usage on line 1, col 12
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 12));

    assert!(
        refs.is_some(),
        "Should find references for array-destructured variable first"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find array binding + 2 usages of first, got {}",
        refs.len()
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_find_references_class_name_across_usages() {
    let source = "class Widget {}\nconst w = new Widget();\nlet x: Widget;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Widget' declaration (line 0, col 6)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for class name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find class declaration + new expression + type annotation, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_interface_name_in_type_position() {
    let source = "interface Config { key: string; }\nfunction init(c: Config) {}\nconst cfg: Config = { key: 'a' };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Config' declaration (line 0, col 10)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(refs.is_some(), "Should find references for interface name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find interface decl + 2 type usages, got {}",
        refs.len()
    );
}

