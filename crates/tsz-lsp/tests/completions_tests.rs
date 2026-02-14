use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_completions_simple() {
    // const x = 1;
    // const y = 2;
    // |  <- cursor here
    let source = "const x = 1;\nconst y = 2;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the end (line 2, column 0)
    let position = Position::new(2, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);

    assert!(items.is_some(), "Should have completions");

    if let Some(items) = items {
        // Should suggest both x and y
        assert!(items.len() >= 2, "Should have at least 2 completions");

        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should suggest 'x'");
        assert!(names.contains(&"y"), "Should suggest 'y'");
    }
}

#[test]
fn test_completions_with_scope() {
    // const x = 1;
    // function foo() {
    //   const y = 2;
    //   |  <- cursor here (should see both x and y)
    // }
    let source = "const x = 1;\nfunction foo() {\n  const y = 2;\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position inside the function (line 3, column 2)
    let position = Position::new(3, 2);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);

    assert!(items.is_some(), "Should have completions");

    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        // Should see both x (outer scope) and y (inner scope)
        assert!(names.contains(&"x"), "Should suggest 'x' from outer scope");
        assert!(names.contains(&"y"), "Should suggest 'y' from inner scope");
        assert!(
            names.contains(&"foo"),
            "Should suggest 'foo' (the function itself)"
        );
    }
}

#[test]
fn test_completions_shadowing() {
    // const x = 1;
    // function foo() {
    //   const x = 2;
    //   |  <- cursor here (should see inner x, not outer x)
    // }
    let source = "const x = 1;\nfunction foo() {\n  const x = 2;\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position inside the function (line 3, column 2)
    let position = Position::new(3, 2);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);

    assert!(items.is_some(), "Should have completions");

    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        // Should only suggest 'x' once (the inner one shadows the outer one)
        let x_count = names.iter().filter(|&&n| n == "x").count();
        assert_eq!(
            x_count, 1,
            "Should suggest 'x' only once (inner shadows outer)"
        );
    }
}

#[test]
fn test_completions_member_object_literal() {
    let source = "const obj = { foo: 1, bar: \"hi\" };\nobj.";
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

    let position = Position::new(1, 4);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(items.is_some(), "Should have member completions");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(names.contains(&"foo"), "Should suggest object member 'foo'");
    assert!(names.contains(&"bar"), "Should suggest object member 'bar'");
}

#[test]
fn test_completions_member_string_literal() {
    let source = "const s = \"hello\";\ns.";
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

    let position = Position::new(1, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(items.is_some(), "Should have member completions");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(
        names.contains(&"length"),
        "Should suggest string member 'length'"
    );
}

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

    assert_eq!(
        var_item.kind,
        CompletionItemKind::Variable,
        "value should be a Variable"
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

#[test]
fn test_completions_enum_kind() {
    // Enums should be reported as CompletionItemKind::Enum.
    let source = "enum Color { Red, Green, Blue }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let color_item = items.iter().find(|i| i.label == "Color").unwrap();

    assert_eq!(
        color_item.kind,
        CompletionItemKind::Enum,
        "Color should be reported as Enum kind"
    );
    assert_eq!(
        color_item.detail.as_deref(),
        Some("enum"),
        "Enum detail should be 'enum'"
    );
}

#[test]
fn test_completions_type_alias_kind() {
    // Type aliases should be reported as CompletionItemKind::TypeAlias.
    let source = "type MyStr = string;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let mystr_item = items.iter().find(|i| i.label == "MyStr").unwrap();

    assert_eq!(
        mystr_item.kind,
        CompletionItemKind::TypeAlias,
        "MyStr should be reported as TypeAlias kind"
    );
    assert_eq!(
        mystr_item.detail.as_deref(),
        Some("type"),
        "Type alias detail should be 'type'"
    );
}

#[test]
fn test_completions_class_kind_preserved() {
    // Classes should still be reported as CompletionItemKind::Class.
    let source = "class Animal {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    let animal_item = items.iter().find(|i| i.label == "Animal").unwrap();

    assert_eq!(
        animal_item.kind,
        CompletionItemKind::Class,
        "Animal should be reported as Class kind"
    );
    assert_eq!(
        animal_item.detail.as_deref(),
        Some("class"),
        "Class detail should be 'class'"
    );
}

#[test]
fn test_completions_member_sort_text() {
    // Member completions should all have sort_text set to the member priority.
    let source = "const obj = { foo: 1, bar: \"hi\" };\nobj.";
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

    let position = Position::new(1, 4);
    let mut cache = None;
    let items = completions
        .get_completions_with_cache(root, position, &mut cache)
        .unwrap();

    for item in &items {
        assert_eq!(
            item.sort_text.as_deref(),
            Some(sort_priority::MEMBER),
            "Member completion '{}' should have MEMBER sort priority",
            item.label,
        );
    }
}

#[test]
fn test_completions_default_sort_text_function() {
    // default_sort_text should return correct categories for each kind.
    // Variables, functions, and parameters use LOCATION_PRIORITY ("11")
    // matching tsc's LocationPriority for most items in scope.
    assert_eq!(
        default_sort_text(CompletionItemKind::Variable),
        sort_priority::LOCATION_PRIORITY
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Function),
        sort_priority::LOCATION_PRIORITY
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Parameter),
        sort_priority::LOCATION_PRIORITY
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Property),
        sort_priority::MEMBER
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Method),
        sort_priority::MEMBER
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Class),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Interface),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Enum),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::TypeAlias),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Module),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::TypeParameter),
        sort_priority::TYPE_DECLARATION
    );
    assert_eq!(
        default_sort_text(CompletionItemKind::Keyword),
        sort_priority::KEYWORD
    );
}

#[test]
fn test_completions_has_action_default_false() {
    // By default, completions should have has_action = false.
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
            !item.has_action,
            "Item '{}' should not have has_action set (reserved for auto-imports)",
            item.label,
        );
    }
}

#[test]
fn test_completions_source_default_none() {
    // By default, source and source_display should be None
    // (they are only set for auto-import completions from the Project layer).
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
            item.source.is_none(),
            "Item '{}' should not have source set (only for auto-imports)",
            item.label,
        );
        assert!(
            item.source_display.is_none(),
            "Item '{}' should not have source_display set",
            item.label,
        );
    }
}

#[test]
fn test_completions_effective_sort_text_uses_explicit() {
    // When sort_text is explicitly set, effective_sort_text returns it.
    let mut item = CompletionItem::new("test".to_string(), CompletionItemKind::Variable);
    item.sort_text = Some("99".to_string());
    assert_eq!(item.effective_sort_text(), "99");
}

#[test]
fn test_completions_effective_sort_text_uses_default() {
    // When sort_text is None, effective_sort_text returns the default.
    let item = CompletionItem::new("test".to_string(), CompletionItemKind::Keyword);
    assert_eq!(
        item.effective_sort_text(),
        sort_priority::KEYWORD,
        "Default sort text for keyword should be KEYWORD priority"
    );
}

#[test]
fn test_completions_builder_methods() {
    // Test all the new builder methods on CompletionItem.
    let item = CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
        .with_detail("function".to_string())
        .with_documentation("A foo function".to_string())
        .with_sort_text("0")
        .with_insert_text("foo($1)".to_string())
        .as_snippet()
        .with_has_action()
        .with_source("./module".to_string())
        .with_source_display("module".to_string())
        .with_kind_modifiers("export".to_string())
        .with_replacement_span(10, 13);

    assert_eq!(item.label, "foo");
    assert_eq!(item.kind, CompletionItemKind::Function);
    assert_eq!(item.detail.as_deref(), Some("function"));
    assert_eq!(item.documentation.as_deref(), Some("A foo function"));
    assert_eq!(item.sort_text.as_deref(), Some("0"));
    assert_eq!(item.insert_text.as_deref(), Some("foo($1)"));
    assert!(item.is_snippet);
    assert!(item.has_action);
    assert_eq!(item.source.as_deref(), Some("./module"));
    assert_eq!(item.source_display.as_deref(), Some("module"));
    assert_eq!(item.kind_modifiers.as_deref(), Some("export"));
    assert_eq!(item.replacement_span, Some((10, 13)));
}

#[test]
fn test_completions_items_sorted_by_sort_text_then_label() {
    // Items should be ordered first by sort_text, then alphabetically
    // by label within each sort_text group.
    let source = "const banana = 1;\nfunction apple() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(2, 0);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position).unwrap();

    // User-declared identifiers (apple, banana) with sort_text "10" should
    // appear before keywords with sort_text "15".
    // Note: global variables (Array, Object, etc.) also have sort_text "15"
    // and are interleaved with keywords, so we only check local declarations.
    let local_items: Vec<_> = items
        .iter()
        .filter(|i| i.effective_sort_text() < sort_priority::GLOBALS_OR_KEYWORDS)
        .collect();
    let kw_items: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionItemKind::Keyword)
        .collect();

    assert!(
        !local_items.is_empty(),
        "Should have local declarations (apple, banana)"
    );
    assert!(!kw_items.is_empty(), "Should have keyword completions");

    if let (Some(last_local), Some(first_kw)) = (local_items.last(), kw_items.first()) {
        let last_local_pos = items
            .iter()
            .position(|i| i.label == last_local.label)
            .unwrap();
        let first_kw_pos = items
            .iter()
            .position(|i| i.label == first_kw.label)
            .unwrap();
        assert!(
            last_local_pos < first_kw_pos,
            "All local declarations should appear before all keywords in the sorted list"
        );
    }
}

// =========================================================================
// Tests for isNewIdentifierLocation
// =========================================================================

fn make_completions_provider(
    source: &str,
) -> (
    tsz_parser::NodeIndex,
    tsz_parser::parser::node::NodeArena,
    BinderState,
    LineMap,
    String,
) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.into_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);
    let line_map = LineMap::build(source);
    (root, arena, binder, line_map, source.to_string())
}

#[test]
fn test_is_new_identifier_location_after_const() {
    // TypeScript returns false for `const |` - it's a declaration keyword but
    // the default in TS is false unless specific AST conditions are met
    let source = "const ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Should NOT be new identifier location after 'const ' (TypeScript default is false)"
    );
}

#[test]
fn test_is_new_identifier_location_after_import() {
    let source = "import ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "Should be new identifier location after 'import '"
    );
}

#[test]
fn test_is_new_identifier_location_after_namespace() {
    let source = "namespace ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "Should be new identifier location after 'namespace '"
    );
}

#[test]
fn test_is_new_identifier_location_after_module() {
    let source = "module ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "Should be new identifier location after 'module '"
    );
}

#[test]
fn test_is_new_identifier_location_after_as() {
    // `x as <type>` is a type assertion - selecting existing type, not new identifier
    let source = "var y = x as ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Should NOT be new identifier location after 'as' in type assertion"
    );
}

#[test]
fn test_is_new_identifier_location_not_after_return() {
    // TypeScript returns false for `return |` - it falls through to the default
    let source = "function f() { return ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Should NOT be new identifier location after 'return '"
    );
}

#[test]
fn test_is_new_identifier_location_not_in_normal_expression() {
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Should NOT be new identifier location at end of file"
    );
}

#[test]
fn test_completion_result_struct_member_completion() {
    // Member completions should have is_member_completion = true and is_new_identifier_location = false
    let source = "const obj = { foo: 1 };
obj.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        &arena,
        &binder,
        &line_map,
        &interner,
        &src,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 4);
    let result = completions.get_completion_result(root, position);
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(result.is_member_completion, "Should be member completion");
    assert!(
        !result.is_global_completion,
        "Should not be global completion"
    );
    assert!(
        !result.is_new_identifier_location,
        "Member completion should not be new identifier location"
    );
}

#[test]
fn test_completion_result_struct_global_completion() {
    let source = "const x = 1;
";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let position = Position::new(1, 0);
    let result = completions.get_completion_result(root, position);
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(result.is_global_completion, "Should be global completion");
    assert!(
        !result.is_member_completion,
        "Should not be member completion"
    );
    assert!(!result.entries.is_empty(), "Should have entries");
}
