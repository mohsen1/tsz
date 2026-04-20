#[test]
fn test_completions_in_return_statement() {
    // Inside a return statement, should suggest variables in scope
    let source = "function compute() {\n  const result = 42;\n  return ;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `return ` (line 2, col 9)
    let items = completions.get_completions(root, Position::new(2, 9));
    assert!(
        items.is_some(),
        "Should have completions in return statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"result"),
            "Should suggest 'result' in return statement, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_with_multiple_function_overloads() {
    // Functions declared multiple times (overloads) should appear once
    let source = "function greet(name: string): string;\nfunction greet(name: string, greeting: string): string;\nfunction greet(name: string, greeting?: string): string { return ''; }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions after overloads");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let greet_count = names.iter().filter(|&&n| n == "greet").count();
        assert!(
            greet_count <= 1,
            "Overloaded function 'greet' should appear at most once, found {greet_count} times"
        );
    }
}

#[test]
fn test_completions_in_catch_clause() {
    // Inside a catch block, should have access to the error variable and outer scope
    let source = "const outer = 1;\ntry {\n  const inner = 2;\n} catch (err) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside catch block (line 4, col 2)
    let items = completions.get_completions(root, Position::new(4, 2));
    assert!(
        items.is_some(),
        "Should have completions inside catch clause"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"outer"),
            "Should suggest 'outer' in catch block, got: {names:?}"
        );
        // The catch parameter 'err' should also be visible
        assert!(
            names.contains(&"err"),
            "Should suggest catch parameter 'err', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_module_declaration() {
    // Inside a module/namespace, should see module-scoped declarations
    let source = "namespace Outer {\n  export const a = 1;\n  namespace Inner {\n    const b = 2;\n    \n  }\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside Inner namespace (line 4, col 4)
    let items = completions.get_completions(root, Position::new(4, 4));
    assert!(
        items.is_some(),
        "Should have completions inside nested namespace"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "Should suggest inner variable 'b', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_type_position() {
    // Inside `import("...")`, completions should be suppressed or not crash
    let source = "type T = import(\"\");";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the import type string literal - should not crash
    let _items = completions.get_completions(root, Position::new(0, 17));
    // Main goal: no panic. Import specifier positions are typically suppressed.
}

#[test]
fn test_completions_computed_property_name() {
    // Inside computed property `[|]`, should suggest variables
    let source = "const key = 'name';\nconst sym = Symbol();\nconst obj = { []: 1 };";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside `[]` in object literal (line 2, col 15)
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(
        items.is_some(),
        "Should have completions inside computed property brackets"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"key"),
            "Should suggest 'key' in computed property, got: {names:?}"
        );
        assert!(
            names.contains(&"sym"),
            "Should suggest 'sym' in computed property, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_inside_array_literal() {
    // Inside an array literal, should suggest variables in scope
    let source = "const alpha = 1;\nconst beta = 2;\nconst arr = [ ];";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside array literal (line 2, col 14)
    let items = completions.get_completions(root, Position::new(2, 14));
    assert!(
        items.is_some(),
        "Should have completions inside array literal"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"alpha"),
            "Should suggest 'alpha' in array literal, got: {names:?}"
        );
        assert!(
            names.contains(&"beta"),
            "Should suggest 'beta' in array literal, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_binary_expression_rhs() {
    // After binary operator, should suggest variables
    let source = "const x = 10;\nconst y = 20;\nconst sum = x + ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `x + ` (line 2, col 16)
    let items = completions.get_completions(root, Position::new(2, 16));
    assert!(
        items.is_some(),
        "Should have completions after binary operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"y"),
            "Should suggest 'y' after '+', got: {names:?}"
        );
        assert!(
            names.contains(&"x"),
            "Should suggest 'x' after '+', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_binary_expression_lhs() {
    // At the beginning of a binary expression (before operator), should suggest variables
    let source = "const p = 5;\nconst q = 10;\nconst r =  + q;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position at `r = |` (line 2, col 10)
    let items = completions.get_completions(root, Position::new(2, 10));
    assert!(
        items.is_some(),
        "Should have completions at binary expression LHS"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"p"),
            "Should suggest 'p' at LHS of binary expr, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_inside_line_comment() {
    // Inside a line comment, we verify the completion engine handles it without crash
    let source = "const x = 1;\n// some comment ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 15));
    // Currently may or may not return completions in comments
    // The main test is that it doesn't crash
}

