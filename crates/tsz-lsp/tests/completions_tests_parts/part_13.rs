#[test]
fn test_completions_is_new_identifier_location_after_type_keyword() {
    // After 'type ' should be new identifier location
    let source = "type ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'type' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_class_body_member_position() {
    // Inside class body at member position, constructor keyword should be offered
    let source = "class Foo {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    // Should offer constructor keyword in class body
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"constructor"),
            "Should suggest 'constructor' in class body, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_no_member_completions_on_standalone_dot() {
    // A standalone `.` at start of file should not offer completions
    let source = ".";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 1));
    assert!(
        items.is_none(),
        "Standalone '.' should not produce completions"
    );
}

#[test]
fn test_completions_in_do_while_body() {
    // Inside do-while body should have completions
    let source = "const x = 1;\ndo {\n  const y = 2;\n  \n} while (true);";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(
        items.is_some(),
        "Should have completions inside do-while body"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"x"),
            "Should suggest outer 'x', got: {names:?}"
        );
        assert!(
            names.contains(&"y"),
            "Should suggest block 'y', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_new_target_in_function() {
    // After `new.` inside a function, should offer `target`
    let source = "function F() {\n  new.\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 6));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"target"),
            "Should suggest 'target' after 'new.' inside function, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_deprecated_globals_sort_last() {
    // Deprecated globals like `escape` and `unescape` should sort after non-deprecated items
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let escape_item = items.iter().find(|i| i.label == "escape");
        assert!(escape_item.is_some(), "Should find deprecated 'escape'");
        let escape_item = escape_item.unwrap();
        assert!(
            escape_item
                .sort_text
                .as_deref()
                .is_some_and(|s| s.starts_with('z')),
            "Deprecated global should have sort_text starting with 'z', got: {:?}",
            escape_item.sort_text
        );
        assert!(
            escape_item
                .kind_modifiers
                .as_deref()
                .is_some_and(|m| m.contains("deprecated")),
            "Deprecated global should have 'deprecated' in kind_modifiers, got: {:?}",
            escape_item.kind_modifiers
        );
    }
}

#[test]
fn test_completions_global_functions_have_snippets() {
    // Global functions like `parseInt` should have snippet insert text
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let parse_item = items.iter().find(|i| i.label == "parseInt");
        assert!(parse_item.is_some(), "Should find 'parseInt'");
        let parse_item = parse_item.unwrap();
        assert_eq!(
            parse_item.kind,
            CompletionItemKind::Function,
            "parseInt should be Function kind"
        );
        assert!(parse_item.is_snippet, "Global function should have snippet");
        assert_eq!(
            parse_item.insert_text.as_deref(),
            Some("parseInt($1)"),
            "Global function should have snippet insert text"
        );
    }
}

#[test]
fn test_completions_const_detail_shows_literal_value() {
    // const with numeric literal initializer should show value as detail
    let source = "const MAX_SIZE = 100;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let max_item = items.iter().find(|i| i.label == "MAX_SIZE");
        assert!(max_item.is_some(), "Should find 'MAX_SIZE'");
        let max_item = max_item.unwrap();
        assert_eq!(
            max_item.detail.as_deref(),
            Some("100"),
            "const with numeric literal should show value as detail"
        );
    }
}

#[test]
fn test_completions_const_string_detail() {
    // const with string literal initializer should show the quoted string as detail
    let source = "const GREETING = \"hello\";\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let greet_item = items.iter().find(|i| i.label == "GREETING");
        assert!(greet_item.is_some(), "Should find 'GREETING'");
        let greet_item = greet_item.unwrap();
        assert_eq!(
            greet_item.detail.as_deref(),
            Some("\"hello\""),
            "const with string literal should show quoted string as detail"
        );
    }
}

#[test]
fn test_completions_const_boolean_detail() {
    // const with boolean literal initializer should show value as detail
    let source = "const IS_DEBUG = true;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let debug_item = items.iter().find(|i| i.label == "IS_DEBUG");
        assert!(debug_item.is_some(), "Should find 'IS_DEBUG'");
        assert_eq!(
            debug_item.unwrap().detail.as_deref(),
            Some("true"),
            "const with boolean literal should show value as detail"
        );
    }
}

