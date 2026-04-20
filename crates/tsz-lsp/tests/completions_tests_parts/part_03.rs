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
fn test_is_new_identifier_location_false_after_object_property_colon() {
    let source = "const value = { foo: ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Object property value position after ':' should not be treated as new identifier declaration location"
    );
}

