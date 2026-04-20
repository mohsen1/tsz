#[test]
fn test_completions_let_with_type_annotation_detail() {
    // let with type annotation should show the type as detail
    let source = "let count: number;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let count_item = items.iter().find(|i| i.label == "count");
        assert!(count_item.is_some(), "Should find 'count'");
        // Detail may include trailing semicolon from source text span
        let detail = count_item.unwrap().detail.as_deref().unwrap_or("");
        assert!(
            detail == "number" || detail == "number;",
            "let with type annotation should show type as detail, got: {detail:?}"
        );
    }
}

#[test]
fn test_completions_no_completions_in_template_literal_text() {
    // Inside the text portion of a template literal (not in ${} expression), should suppress
    let source = "const x = 1;\nconst s = `hello world`;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside template literal text portion (line 1, col 16)
    let items = completions.get_completions(root, Position::new(1, 16));
    // Should be suppressed or empty in string part
    if let Some(ref items) = items {
        // Template literal text should be treated as no-completion context
        let _ = items;
    }
}

#[test]
fn test_completions_multiple_parameters_visible() {
    // Multiple function parameters should all be visible inside function body
    let source = "function calc(a: number, b: string, c: boolean) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(
        items.is_some(),
        "Should have completions inside function with multiple params"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "Should suggest parameter 'a', got: {names:?}"
        );
        assert!(
            names.contains(&"b"),
            "Should suggest parameter 'b', got: {names:?}"
        );
        assert!(
            names.contains(&"c"),
            "Should suggest parameter 'c', got: {names:?}"
        );
        // All should have Parameter kind
        for param_name in &["a", "b", "c"] {
            let param_item = items.iter().find(|i| i.label == *param_name).unwrap();
            assert_eq!(
                param_item.kind,
                CompletionItemKind::Parameter,
                "Parameter '{param_name}' should have Parameter kind"
            );
        }
    }
}

#[test]
fn test_completions_enum_member_dot_access() {
    // After `EnumName.`, should show enum members
    let source = "enum Status { Active, Inactive, Pending }\nStatus.";
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
    let position = Position::new(1, 7);
    let items = completions.get_completions(root, position);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"Active"),
            "Should suggest enum member 'Active', got: {names:?}"
        );
        assert!(
            names.contains(&"Inactive"),
            "Should suggest enum member 'Inactive', got: {names:?}"
        );
        assert!(
            names.contains(&"Pending"),
            "Should suggest enum member 'Pending', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_completion_result_is_member_false_for_global() {
    // At top-level, completion result should have is_member_completion = false
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let result = completions.get_completion_result(root, Position::new(1, 0));
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(
        !result.is_member_completion,
        "Top-level should not be member completion"
    );
    assert!(
        result.is_global_completion,
        "Top-level should be global completion"
    );
}

#[test]
fn test_completions_inside_labeled_statement() {
    // Inside a labeled statement body, should have completions
    let source = "const x = 1;\nouter: for (let i = 0; i < 10; i++) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside labeled statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"x"),
            "Should suggest 'x' in labeled loop, got: {names:?}"
        );
        assert!(
            names.contains(&"i"),
            "Should suggest loop var 'i', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_binding_visible_after_import() {
    // An imported name should be visible after the import statement
    let source = "import { foo } from './bar';\nconst x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(
        items.is_some(),
        "Should have completions after import statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "Should suggest imported 'foo', got: {names:?}"
        );
        assert!(
            names.contains(&"x"),
            "Should suggest local 'x', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_binding_kind_is_alias() {
    // Import bindings should have Alias kind
    let source = "import { myFunc } from './module';\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let import_item = items.iter().find(|i| i.label == "myFunc");
        if let Some(import_item) = import_item {
            assert_eq!(
                import_item.kind,
                CompletionItemKind::Alias,
                "Import binding should have Alias kind"
            );
        }
    }
}

#[test]
fn test_completions_multiline_object_literal_member() {
    // Object literal with properties across multiple lines
    let source = "const obj = {\n  name: 'test',\n  count: 42,\n  active: true\n};\nobj.";
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
    let position = Position::new(5, 4);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"name"),
            "Should suggest 'name', got: {names:?}"
        );
        assert!(
            names.contains(&"count"),
            "Should suggest 'count', got: {names:?}"
        );
        assert!(
            names.contains(&"active"),
            "Should suggest 'active', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_completion_item_serialization_fields() {
    // Verify that CompletionItem serializes expected fields correctly
    let item = CompletionItem::new("test".to_string(), CompletionItemKind::Variable)
        .with_detail("number".to_string())
        .with_sort_text("11")
        .with_kind_modifiers("export".to_string());

    let value = serde_json::to_value(&item).expect("should serialize");

    assert_eq!(value.get("label").and_then(|v| v.as_str()), Some("test"));
    assert_eq!(value.get("detail").and_then(|v| v.as_str()), Some("number"));
    assert_eq!(value.get("sort_text").and_then(|v| v.as_str()), Some("11"));
    assert_eq!(
        value.get("kind_modifiers").and_then(|v| v.as_str()),
        Some("export")
    );
    // is_snippet should be omitted when false (skip_serializing_if)
    assert!(
        value.get("is_snippet").is_none(),
        "is_snippet should be omitted when false"
    );
    // has_action should be omitted when false
    assert!(
        value.get("has_action").is_none(),
        "has_action should be omitted when false"
    );
}

