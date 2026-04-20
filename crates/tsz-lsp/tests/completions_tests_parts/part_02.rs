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
fn test_completion_item_serializes_source_display_camel_case() {
    let item = CompletionItem::new("Foo".to_string(), CompletionItemKind::Variable)
        .with_source("./lib/foo".to_string())
        .with_source_display("./lib/foo".to_string());

    let value = serde_json::to_value(&item).expect("serialize completion item");
    assert_eq!(
        value
            .get("sourceDisplay")
            .and_then(serde_json::Value::as_str),
        Some("./lib/foo")
    );
    assert!(
        value.get("source_display").is_none(),
        "sourceDisplay should serialize in tsserver camelCase"
    );
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

