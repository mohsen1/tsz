#[test]
fn test_completions_no_completions_at_definition_location() {
    // After 'const ' we're defining a new identifier, so no completions
    let source = "const ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 6));
    // Should be suppressed at definition location
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions at variable definition location"
        );
    }
}

#[test]
fn test_completions_class_kind() {
    // Class declarations should have Class kind
    let source = "class MyClass {}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let class_item = items.iter().find(|i| i.label == "MyClass");
        assert!(class_item.is_some(), "Should find 'MyClass'");
        assert_eq!(
            class_item.unwrap().kind,
            CompletionItemKind::Class,
            "Class should have Class kind"
        );
    }
}

#[test]
fn test_completions_interface_kind_with_helper() {
    // Interface declarations should have Interface kind
    let source = "interface MyInterface { x: number; }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let iface_item = items.iter().find(|i| i.label == "MyInterface");
        assert!(iface_item.is_some(), "Should find 'MyInterface'");
        assert_eq!(
            iface_item.unwrap().kind,
            CompletionItemKind::Interface,
            "Interface should have Interface kind"
        );
    }
}

#[test]
fn test_completions_enum_kind_with_helper() {
    // Enum declarations should have Enum kind
    let source = "enum MyEnum { A, B }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let enum_item = items.iter().find(|i| i.label == "MyEnum");
        assert!(enum_item.is_some(), "Should find 'MyEnum'");
        assert_eq!(
            enum_item.unwrap().kind,
            CompletionItemKind::Enum,
            "Enum should have Enum kind"
        );
    }
}

#[test]
fn test_completions_type_alias_kind_with_helper() {
    // Type alias declarations should have TypeAlias kind
    let source = "type MyType = string | number;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let type_item = items.iter().find(|i| i.label == "MyType");
        assert!(type_item.is_some(), "Should find 'MyType'");
        assert_eq!(
            type_item.unwrap().kind,
            CompletionItemKind::TypeAlias,
            "Type alias should have TypeAlias kind"
        );
    }
}

#[test]
fn test_completion_result_commit_characters() {
    // Global completions (non-member, non-new-identifier) should have default commit characters
    let source = "const x = 1;\nfunction foo() {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let result = completions.get_completion_result(root, Position::new(2, 2));
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    // Inside function body is NOT a new identifier location (just typing expressions)
    // so commit characters should be present
    if !result.is_new_identifier_location {
        assert!(
            result.default_commit_characters.is_some(),
            "Non-new-identifier completions should have commit characters"
        );
        let chars = result.default_commit_characters.unwrap();
        assert!(
            chars.contains(&".".to_string()),
            "Commit chars should include '.'"
        );
        assert!(
            chars.contains(&",".to_string()),
            "Commit chars should include ','"
        );
        assert!(
            chars.contains(&";".to_string()),
            "Commit chars should include ';'"
        );
    }
}

#[test]
fn test_is_new_identifier_location_after_class_keyword() {
    // After 'class ' keyword, should be new identifier location
    let source = "class ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'class' keyword should be new identifier location"
    );
}

#[test]
fn test_is_new_identifier_location_after_function_keyword() {
    // After 'function ' keyword, should be new identifier location
    let source = "function ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'function' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_import_meta_dot() {
    // After "import.meta.", should get meta property completions
    let source = "import.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 7));
    // Should offer "meta" as a completion for import.
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"meta"),
            "Should suggest 'meta' after 'import.'"
        );
    }
}

#[test]
fn test_completions_with_strict_mode() {
    // Test the with_strict constructor
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::with_strict(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
        true,
    );
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions in strict mode");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should suggest 'x' in strict mode");
    }
}

