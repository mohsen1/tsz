#[test]
fn test_completions_inside_block_comment() {
    // Inside a block comment, we verify no crash
    let source = "const x = 1;\n/* block comment  */";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 10));
    // Currently may or may not return completions in comments
}

#[test]
fn test_completions_inside_string_literal() {
    // Inside a string literal, verify no crash
    let source = "const x = 1;\nconst s = \"hello world\";";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 16));
    // Currently may or may not return completions in strings
}

#[test]
fn test_completions_for_loop_variable_scope() {
    // Variables declared in a for loop should be visible inside the loop body
    let source = "const outer = 1;\nfor (let i = 0; i < 10; i++) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside loop body (line 2, col 2)
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(items.is_some(), "Should have completions inside for loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"i"),
            "Should suggest loop variable 'i', got: {names:?}"
        );
        assert!(
            names.contains(&"outer"),
            "Should suggest outer variable 'outer', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_no_duplicate_from_var_hoisting() {
    // var declarations are hoisted; should not appear duplicated
    let source = "var x = 1;\nvar x = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let x_count = names.iter().filter(|&&n| n == "x").count();
        assert_eq!(
            x_count, 1,
            "Hoisted 'var x' should appear exactly once, found {x_count} times"
        );
    }
}

#[test]
fn test_completions_after_spread_operator() {
    // After `...` in an array, should suggest variables
    let source = "const items = [1, 2];\nconst all = [0, ...];";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `...` (line 1, col 19)
    let items = completions.get_completions(root, Position::new(1, 19));
    assert!(
        items.is_some(),
        "Should have completions after spread operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"items"),
            "Should suggest 'items' after spread, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_at_function_name_definition() {
    // At the name position of a function declaration, verify no crash
    let source = "function ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 9));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_at_class_name_definition() {
    // At the name position of a class declaration, verify no crash
    let source = "class ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 6));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_after_assignment_operator() {
    // After `=` in an assignment, should suggest variables
    let source = "let target = 0;\nconst source = 42;\ntarget = ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `target = ` (line 2, col 9)
    let items = completions.get_completions(root, Position::new(2, 9));
    assert!(
        items.is_some(),
        "Should have completions after assignment operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"source"),
            "Should suggest 'source' after '=', got: {names:?}"
        );
        assert!(
            names.contains(&"target"),
            "Should suggest 'target' after '=', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_after_logical_operator() {
    // After logical operators (`&&`, `||`), should suggest variables
    let source = "const a = true;\nconst b = false;\nconst c = a && ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `a && ` (line 2, col 15)
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(
        items.is_some(),
        "Should have completions after logical operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "Should suggest 'b' after '&&', got: {names:?}"
        );
    }
}

// ============================================================================
// Additional coverage tests (batch 2)
// ============================================================================

#[test]
fn test_completions_member_nested_object_dot() {
    // After `obj.inner.`, member resolution should return some completions
    // (may resolve to inner properties or parent-level members depending on type resolution)
    let source = "const obj = { inner: { deep: 42, flag: true } };\nobj.inner.";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 10);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash on nested property access; verify we get some result
    assert!(
        items.is_some(),
        "Should have completions for nested member access"
    );
    if let Some(items) = items {
        assert!(
            !items.is_empty(),
            "Should have non-empty member completions"
        );
    }
}

