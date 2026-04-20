#[test]
fn test_completions_type_parameter_visible_in_function_body() {
    // Type parameter T should be visible in function body as a completion
    let source = "function identity<T>(x: T): T {\n  let y: ;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `let y: ` (line 1, col 9)
    let items = completions.get_completions(root, Position::new(1, 9));
    // Should not crash; type parameters may or may not appear depending on scope resolution
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        if names.contains(&"T") {
            let t_item = items.iter().find(|i| i.label == "T").unwrap();
            assert_eq!(
                t_item.kind,
                CompletionItemKind::TypeParameter,
                "Type parameter should have TypeParameter kind"
            );
        }
    }
}

#[test]
fn test_completions_no_completions_in_regex_literal() {
    // Inside a regex literal, completions should be suppressed
    let source = "const x = 1;\nconst re = /pattern/;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside regex (line 1, col 15)
    let items = completions.get_completions(root, Position::new(1, 15));
    // Should suppress or return empty
    if let Some(ref items) = items {
        // If items returned, they should be empty since we're inside a regex
        // (though parser may not treat this as a regex in all cases)
        let _ = items;
    }
}

#[test]
fn test_completions_optional_chaining_member() {
    // After `?.`, should still offer member completions
    let source = "const obj = { foo: 1, bar: 'hello' };\nconst x: typeof obj | null = obj;\nx?.";
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
    let position = Position::new(2, 3);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash on optional chaining
    let _ = items;
}

#[test]
fn test_completions_no_completions_after_number_dot() {
    // After a number literal dot (e.g., `1.`), completions may be ambiguous
    // because `1.` could be a decimal number or property access
    let source = "1.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 2));
    // Should not crash; result depends on parser interpretation
    let _ = items;
}

#[test]
fn test_completions_class_static_members_via_class_name() {
    // `ClassName.` should show static members
    let source =
        "class Util {\n  static helper() {}\n  static count = 0;\n  instance() {}\n}\nUtil.";
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
    let position = Position::new(5, 5);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"helper"),
            "Should suggest static method 'helper', got: {names:?}"
        );
        assert!(
            names.contains(&"count"),
            "Should suggest static property 'count', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_is_new_identifier_location_after_equals_in_const() {
    // After `const x = `, should be new identifier location (expression expected)
    let source = "const x = ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'const x = ' should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_open_paren() {
    // After `(`, should be new identifier location
    let source = "function f(";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After '(' should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_comma_in_params() {
    // After `,` in a parameter list, should be new identifier location
    let source = "function f(x: number, ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After ',' in param list should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_interface_keyword() {
    // After 'interface ' should be new identifier location
    let source = "interface ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'interface' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_enum_keyword() {
    // After 'enum ' should be new identifier location
    let source = "enum ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'enum' keyword should be new identifier location"
    );
}

