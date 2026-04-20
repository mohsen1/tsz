#[test]
fn test_find_references_class_self_reference() {
    let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Foo' usage inside the method (line 2)
    let position = Position::new(2, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for class self name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_expression_name() {
    let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Bar' usage inside the method (line 2)
    let position = Position::new(2, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for class expression name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_static_block_local() {
    let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage inside the static block (line 3)
    let position = Position::new(3, 4);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for static block locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

// =========================================================================
// Tests for ReferenceInfo: isWriteAccess, isDefinition, lineText
// =========================================================================

/// Helper to get detailed references for a symbol at a given position.
fn get_detailed_refs(source: &str, file_name: &str, line: u32, col: u32) -> Vec<ReferenceInfo> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(line, col);

    let find_refs = FindReferences::new(arena, &binder, &line_map, file_name.to_string(), source);
    find_refs
        .find_references_detailed(root, position)
        .unwrap_or_default()
}

#[test]
fn test_detailed_refs_const_declaration_is_write_and_definition() {
    // `const x = 1; x + x;`
    // The declaration of x should be isWriteAccess=true, isDefinition=true
    // The usages of x should be isWriteAccess=false, isDefinition=false
    let source = "const x = 1;\nx + x;";
    let refs = get_detailed_refs(source, "test.ts", 1, 0);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // Find the declaration ref (on line 0, which is "const x = 1;")
    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(
        decl_ref.is_some(),
        "Should have a ref on line 0 (declaration)"
    );
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_write_access,
        "Declaration should be a write access"
    );
    assert!(decl_ref.is_definition, "Declaration should be a definition");

    // Find a usage ref (on line 1, which is "x + x;")
    let usage_refs: Vec<_> = refs
        .iter()
        .filter(|r| r.location.range.start.line == 1)
        .collect();
    assert!(
        !usage_refs.is_empty(),
        "Should have at least one usage ref on line 1"
    );
    for ur in &usage_refs {
        assert!(
            !ur.is_write_access,
            "Read-only usage should not be a write access"
        );
        assert!(!ur.is_definition, "Usage should not be a definition");
    }
}

#[test]
fn test_detailed_refs_assignment_is_write_access() {
    // `let x = 1; x = 2;`
    // The assignment `x = 2` should be isWriteAccess=true, isDefinition=false
    let source = "let x = 1;\nx = 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The ref on line 1 ("x = 2;") is an assignment - should be write
    let assign_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(
        assign_ref.is_some(),
        "Should have a ref on line 1 (assignment)"
    );
    let assign_ref = assign_ref.unwrap();
    assert!(
        assign_ref.is_write_access,
        "Assignment target should be a write access"
    );
    assert!(!assign_ref.is_definition, "Assignment is not a definition");
}

#[test]
fn test_detailed_refs_compound_assignment_is_write_access() {
    // `let x = 0; x += 1;`
    // The compound assignment `x += 1` should be isWriteAccess=true
    let source = "let x = 0;\nx += 1;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let compound_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(
        compound_ref.is_some(),
        "Should have a ref on line 1 (compound assignment)"
    );
    let compound_ref = compound_ref.unwrap();
    assert!(
        compound_ref.is_write_access,
        "Compound assignment target should be a write access"
    );
    assert!(
        !compound_ref.is_definition,
        "Compound assignment is not a definition"
    );
}

#[test]
fn test_detailed_refs_function_declaration_is_definition() {
    // `function foo() {} foo();`
    // The function name at declaration is isDefinition=true, isWriteAccess=true
    // The call site is isDefinition=false, isWriteAccess=false
    let source = "function foo() {}\nfoo();";
    let refs = get_detailed_refs(source, "test.ts", 1, 0);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The declaration on line 0
    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(
        decl_ref.is_some(),
        "Should have a ref on line 0 (declaration)"
    );
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Function declaration name should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Function declaration name should be a write access"
    );

    // The call on line 1
    let call_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(call_ref.is_some(), "Should have a ref on line 1 (call)");
    let call_ref = call_ref.unwrap();
    assert!(
        !call_ref.is_definition,
        "Function call should not be a definition"
    );
    assert!(
        !call_ref.is_write_access,
        "Function call should not be a write access"
    );
}

#[test]
fn test_detailed_refs_class_declaration_is_definition() {
    // `class Foo {} new Foo();`
    let source = "class Foo {}\nnew Foo();";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Class declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Class declaration should be a write access"
    );

    let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(usage_ref.is_some(), "Should have usage ref");
    let usage_ref = usage_ref.unwrap();
    assert!(
        !usage_ref.is_definition,
        "new Foo() should not be a definition"
    );
    assert!(
        !usage_ref.is_write_access,
        "new Foo() should not be a write access"
    );
}

#[test]
fn test_detailed_refs_parameter_is_write_and_definition() {
    // `function foo(x: number) { return x; }`
    // Parameter x declaration is isWriteAccess=true, isDefinition=true
    // Usage of x in body is isWriteAccess=false, isDefinition=false
    let source = "function foo(x: number) {\n  return x;\n}";
    let refs = get_detailed_refs(source, "test.ts", 1, 9);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The parameter declaration (line 0)
    let param_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(param_ref.is_some(), "Should have param ref on line 0");
    let param_ref = param_ref.unwrap();
    assert!(
        param_ref.is_definition,
        "Parameter declaration should be a definition"
    );
    assert!(
        param_ref.is_write_access,
        "Parameter declaration should be a write access"
    );

    // The usage in the body (line 1)
    let body_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(body_ref.is_some(), "Should have body ref on line 1");
    let body_ref = body_ref.unwrap();
    assert!(
        !body_ref.is_definition,
        "Parameter usage should not be a definition"
    );
    assert!(
        !body_ref.is_write_access,
        "Parameter read should not be a write access"
    );
}

#[test]
fn test_detailed_refs_line_text_is_correct() {
    // Verify lineText contains the correct line content
    let source = "const x = 1;\nconsole.log(x);";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have ref on line 0");
    assert_eq!(
        decl_ref.unwrap().line_text,
        "const x = 1;",
        "lineText should be the full line content"
    );

    let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(usage_ref.is_some(), "Should have ref on line 1");
    assert_eq!(
        usage_ref.unwrap().line_text,
        "console.log(x);",
        "lineText should be the full line content"
    );
}

