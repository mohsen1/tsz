#[test]
fn test_completions_sort_order_locals_before_keywords() {
    // Local declarations should sort before keywords
    let source = "const myVar = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let var_item = items.iter().find(|i| i.label == "myVar");
        let kw_item = items.iter().find(|i| i.label == "if");
        assert!(var_item.is_some(), "Should find 'myVar'");
        assert!(kw_item.is_some(), "Should find keyword 'if'");
        // Local declarations have sort text "11" (LOCATION_PRIORITY), keywords have "15"
        let var_sort = var_item.unwrap().effective_sort_text();
        let kw_sort = kw_item.unwrap().effective_sort_text();
        assert!(
            var_sort <= kw_sort,
            "Local variable sort text ({var_sort}) should be <= keyword sort text ({kw_sort})"
        );
    }
}

#[test]
fn test_completions_template_literal_expression() {
    // Completions inside template literal expression `${|}`
    // Line 1: "const greeting = `hello ${ }`;"
    //          0123456789...                   col 26 = '$', col 27 = '{', col 28 = ' '
    let source = "const name = 'world';\nconst greeting = `hello ${ }`;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the ${ } - try at the space between { and }
    let items = completions.get_completions(root, Position::new(1, 28));
    // Template literal expression completion may or may not be supported,
    // just verify no crash and check if we get items
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // If completions are returned, they should include variables in scope
        if !names.is_empty() {
            assert!(
                names.contains(&"name") || names.contains(&"greeting"),
                "Should suggest variables in scope, got: {names:?}"
            );
        }
    }
    // Test passes regardless - we're mainly testing it doesn't crash
}

#[test]
fn test_completions_namespace_members() {
    // After namespace dot, should offer namespace members
    let source =
        "namespace MyNS {\n  export const val = 1;\n  export function greet() {}\n}\nMyNS.";
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
    let items = completions.get_completions(root, Position::new(4, 5));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val"),
            "Should suggest namespace member 'val'"
        );
        assert!(
            names.contains(&"greet"),
            "Should suggest namespace member 'greet'"
        );
    }
}

// ============================================================================
// New coverage tests for completions module
// ============================================================================

#[test]
fn test_completions_after_new_keyword() {
    // After `new `, should suggest classes and constructable symbols in scope
    let source = "class MyClass { constructor() {} }\nclass Other {}\nnew ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 4));
    assert!(items.is_some(), "Should have completions after 'new '");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"MyClass"),
            "Should suggest 'MyClass' after 'new', got: {names:?}"
        );
        assert!(
            names.contains(&"Other"),
            "Should suggest 'Other' after 'new', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_object_literal_shorthand_property() {
    // Inside an object literal, should suggest variables for shorthand properties
    let source = "const foo = 1;\nconst bar = 2;\nconst obj = { };";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the braces of { }
    let items = completions.get_completions(root, Position::new(2, 14));
    assert!(
        items.is_some(),
        "Should have completions inside object literal"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "Should suggest 'foo' for shorthand property, got: {names:?}"
        );
        assert!(
            names.contains(&"bar"),
            "Should suggest 'bar' for shorthand property, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_ternary_expression() {
    // Completions in the consequent and alternate of a ternary expression
    let source = "const flag = true;\nconst a = 1;\nconst b = 2;\nconst result = flag ? ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `? ` in the ternary
    let items = completions.get_completions(root, Position::new(3, 22));
    assert!(
        items.is_some(),
        "Should have completions in ternary consequent"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "Should suggest 'a' in ternary, got: {names:?}"
        );
        assert!(
            names.contains(&"b"),
            "Should suggest 'b' in ternary, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_after_typeof_operator() {
    // After `typeof `, should suggest variables in scope
    let source = "const myVar = 42;\nconst myStr = 'hello';\nlet t = typeof ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `typeof `
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(items.is_some(), "Should have completions after 'typeof '");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"myVar"),
            "Should suggest 'myVar' after typeof, got: {names:?}"
        );
        assert!(
            names.contains(&"myStr"),
            "Should suggest 'myStr' after typeof, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_generic_type_arguments() {
    // After `Array<`, should suggest type names in scope
    let source = "interface Foo {}\ntype Bar = {};\nlet x: Array<>;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside `Array<|>`
    let items = completions.get_completions(root, Position::new(2, 14));
    // May or may not produce items, but should not crash
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Type names should appear
        if names.contains(&"Foo") || names.contains(&"Bar") {
            // Good - type names are suggested in type argument position
        }
    }
}

#[test]
fn test_completions_in_type_annotation_position() {
    // After `: ` in a variable declaration, should suggest types
    let source = "interface MyInterface {}\ntype MyType = string;\nlet x: ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `let x: `
    let items = completions.get_completions(root, Position::new(2, 7));
    assert!(
        items.is_some(),
        "Should have completions in type annotation position"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"MyInterface") || names.contains(&"MyType"),
            "Should suggest type names in type position, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_switch_case_expression() {
    // Inside a switch case, should suggest variables in scope
    let source = "const val = 1;\nconst opt = 2;\nswitch (val) {\n  case : break;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `case ` (line 3, col 7)
    let items = completions.get_completions(root, Position::new(3, 7));
    assert!(
        items.is_some(),
        "Should have completions in switch case expression"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val") || names.contains(&"opt"),
            "Should suggest variables in case expression, got: {names:?}"
        );
    }
}

