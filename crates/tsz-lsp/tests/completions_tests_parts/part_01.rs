#[test]
fn test_completions_includes_keywords() {
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the end
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);

    assert!(items.is_some(), "Should have completions");

    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        // Should include keywords
        assert!(
            names.contains(&"function"),
            "Should suggest keyword 'function'"
        );
        assert!(names.contains(&"const"), "Should suggest keyword 'const'");
        assert!(names.contains(&"class"), "Should suggest keyword 'class'");
    }
}

#[test]
fn test_completions_global_surface_matches_fourslash_globals() {
    let source = "Button";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(0, 6);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions
        .get_completions(root, position)
        .expect("Should have completions");
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(
        names.contains(&"Array"),
        "Expected `Array` in global completions"
    );
    assert!(
        names.contains(&"globalThis"),
        "Expected `globalThis` in global completions"
    );
    assert!(
        names.contains(&"undefined"),
        "Expected `undefined` in global completions"
    );
    assert!(
        !names.contains(&"Promise"),
        "Expected `Promise` to be excluded from fourslash globals surface"
    );
    assert!(
        !names.contains(&"Map"),
        "Expected `Map` to be excluded from fourslash globals surface"
    );
    assert!(
        !names.contains(&"private"),
        "Expected `private` to be excluded from global keyword list"
    );
}

#[test]
fn test_completions_global_entry_kinds_match_fourslash() {
    let source = "Table";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(0, 5);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions
        .get_completions(root, position)
        .expect("Should have completions");

    let find_kind = |name: &str| {
        items
            .iter()
            .find(|item| item.label == name)
            .map(|item| item.kind)
            .unwrap_or_else(|| panic!("Expected completion `{name}`"))
    };

    assert_eq!(find_kind("Array"), CompletionItemKind::Variable);
    assert_eq!(find_kind("Math"), CompletionItemKind::Variable);
    assert_eq!(find_kind("Intl"), CompletionItemKind::Module);
}

#[test]
fn test_completions_jsdoc_documentation() {
    // Test that JSDoc comments are included in completion items
    let source = "/** This is a test function */\nfunction foo() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the end
    let position = Position::new(2, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);

    assert!(items.is_some(), "Should have completions");

    if let Some(items) = items {
        let foo_item = items.iter().find(|i| i.label == "foo");
        assert!(foo_item.is_some(), "Should suggest 'foo'");

        if let Some(item) = foo_item {
            assert!(
                item.documentation
                    .as_ref()
                    .is_some_and(|d| d.contains("test function")),
                "Should include JSDoc documentation"
            );
        }
    }
}

// =========================================================================
// New tests for improved tsserver-compatible completion entry format
// =========================================================================

#[test]
fn test_completions_sort_text_keywords_after_identifiers() {
    // Keywords should have higher sort_text than identifiers so they
    // appear later in the completion list, matching tsserver behaviour.
    let source = "const abc = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let abc_item = items.iter().find(|i| i.label == "abc").unwrap();
    let kw_item = items.iter().find(|i| i.label == "function").unwrap();

    assert!(
        abc_item.effective_sort_text() < kw_item.effective_sort_text(),
        "Identifiers (sort_text={:?}) should sort before keywords (sort_text={:?})",
        abc_item.effective_sort_text(),
        kw_item.effective_sort_text(),
    );
}

#[test]
fn test_completions_sort_text_present_on_all_items() {
    // Every completion item should have an explicit sort_text value set.
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    for item in &items {
        assert!(
            item.sort_text.is_some(),
            "Item '{}' (kind={:?}) should have explicit sort_text",
            item.label,
            item.kind,
        );
    }
}

#[test]
fn test_completions_function_has_snippet_insert_text() {
    // Function completions should have insert_text with snippet tab-stops
    // e.g. "foo($1)" so the cursor lands inside the parens.
    let source = "function greet(name: string) {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let greet_item = items.iter().find(|i| i.label == "greet").unwrap();

    assert_eq!(
        greet_item.kind,
        CompletionItemKind::Function,
        "greet should be a Function"
    );
    assert_eq!(
        greet_item.insert_text.as_deref(),
        Some("greet($1)"),
        "Function completion should have snippet insert text"
    );
    assert!(
        greet_item.is_snippet,
        "Function completion should be marked as snippet"
    );
}

#[test]
fn test_completions_variable_no_snippet() {
    // Variable completions should NOT have snippet insert_text.
    let source = "const value = 42;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let var_item = items.iter().find(|i| i.label == "value").unwrap();

    assert!(
        matches!(
            var_item.kind,
            CompletionItemKind::Variable | CompletionItemKind::Const
        ),
        "value should be a Variable or Const, got {:?}",
        var_item.kind
    );
    assert!(
        var_item.insert_text.is_none(),
        "Variable completion should not have insert_text"
    );
    assert!(
        !var_item.is_snippet,
        "Variable completion should not be a snippet"
    );
}

#[test]
fn test_completions_keyword_sort_text_value() {
    // All keyword completions should have sort_text == sort_priority::KEYWORD.
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let keyword_items: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionItemKind::Keyword)
        .collect();

    assert!(!keyword_items.is_empty(), "Should have keyword completions");

    for kw in &keyword_items {
        assert_eq!(
            kw.sort_text.as_deref(),
            Some(sort_priority::KEYWORD),
            "Keyword '{}' should have sort_text='{}'",
            kw.label,
            sort_priority::KEYWORD,
        );
    }
}

#[test]
fn test_completions_interface_kind() {
    // Interfaces should be reported as CompletionItemKind::Interface.
    let source = "interface Foo { x: number }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let foo_item = items.iter().find(|i| i.label == "Foo").unwrap();

    assert_eq!(
        foo_item.kind,
        CompletionItemKind::Interface,
        "Foo should be reported as Interface kind"
    );
    assert_eq!(
        foo_item.detail.as_deref(),
        Some("interface"),
        "Interface detail should be 'interface'"
    );
}

