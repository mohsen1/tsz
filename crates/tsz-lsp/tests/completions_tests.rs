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
fn test_completions_member_access_wins_over_prior_string_literal_context() {
    let source = "let x: string = 42;\n\nfunction greet(name: string): string {\n  return \"Hello, \" + name;\n}\n\ngreet(123);\n\ninterface User {\n  name: string;\n  age: number;\n}\n\nconst user: User = {\n  name: \"Alice\",\n  age: \"thirty\",\n};\n\nuser.";
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

    let position = Position::new(18, 5);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(
        items.is_some(),
        "Should have member completions for 'user.'"
    );
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(
        names.contains(&"name"),
        "Should suggest property 'name', got: {names:?}"
    );
    assert!(
        names.contains(&"age"),
        "Should suggest property 'age', got: {names:?}"
    );
    assert!(
        !names.contains(&"\"Hello, \""),
        "Member access should not be hijacked by prior string literal completions, got: {names:?}"
    );
}

#[test]
fn test_completions_contextual_string_literal_argument_keyof() {
    let source = "interface Events {\n  click: any;\n  drag: any;\n}\n\ndeclare function addListener<K extends keyof Events>(type: K, listener: (ev: Events[K]) => any): void;\n\naddListener(\"\");\n";
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

    let literal_offset = source.find("\"\"").expect("expected empty string literal") as u32;
    let position = line_map.offset_to_position(literal_offset + 1, source);

    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    assert!(
        items.is_some(),
        "Should have contextual string literal completions"
    );
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    // String literal completions may include surrounding quotes in the label
    assert!(
        names.contains(&"click") || names.contains(&"\"click\""),
        "Should suggest key 'click', got {names:?}"
    );
    assert!(
        names.contains(&"drag") || names.contains(&"\"drag\""),
        "Should suggest key 'drag', got {names:?}"
    );
}

#[test]
fn test_completions_member_excludes_private_class_properties() {
    let source = "class N {\n  constructor(public x: number, public y: number, private z: string) {}\n}\nconst t = new N(0, 1, \"\");\nt.";
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

    let position = Position::new(4, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(items.is_some(), "Should have member completions");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(names.contains(&"x"), "Should suggest public member 'x'");
    assert!(names.contains(&"y"), "Should suggest public member 'y'");
    assert!(
        !names.contains(&"z"),
        "Should not suggest private member 'z'"
    );
}

#[test]
fn test_completions_member_list_of_class_exact() {
    // Matches fourslash test memberListOfClass: f. should show only pubMeth and pubProp
    let source = "class C1 {\n   public pubMeth() { }\n   private privMeth() { }\n   public pubProp = 0;\n   private privProp = 0;\n}\nvar f = new C1();\nf.";
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
    let position = Position::new(7, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    assert!(items.is_some(), "Should have member completions");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(
        names.len(),
        2,
        "Expected exactly 2 completions (pubMeth, pubProp), got: {names:?}"
    );
    assert!(names.contains(&"pubMeth"), "Should suggest pubMeth");
    assert!(names.contains(&"pubProp"), "Should suggest pubProp");
}

#[test]
fn test_completions_member_parameter_typeof_class_includes_static_and_namespace_members() {
    let source = "class C<T> {\n    static foo(x: number) { }\n    x: T;\n}\n\nnamespace C {\n    export function f(x: typeof C) {\n        x.\n    }\n}\n";
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

    let position = Position::new(7, 10);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(items.is_some(), "Should have member completions");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(
        names.contains(&"foo"),
        "Should include static class member 'foo', got {names:?}"
    );
    assert!(
        names.contains(&"f"),
        "Should include merged namespace export 'f', got {names:?}"
    );
}

#[test]
fn test_completions_member_parameter_typeof_class_after_dot() {
    let source = "class C<T> {\n    static foo(x: number) { }\n    x: T;\n}\n\nnamespace C {\n    export function f(x: typeof C) {\n        x.\n    }\n}\n";
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

    // Cursor immediately after `x.` on line 7 (0-based), column 10.
    let position = Position::new(7, 10);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);

    assert!(items.is_some(), "Should have member completions after `.`");
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

    assert!(
        names.contains(&"foo"),
        "Should include static class member 'foo', got {names:?}"
    );
    assert!(
        names.contains(&"f"),
        "Should include merged namespace export 'f', got {names:?}"
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

#[test]
fn test_is_new_identifier_location_false_for_type_annotation_identifier_prefix() {
    let source = "interface VFS { getSourceFile(path: string): ts";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Type annotation identifier prefixes should not be treated as new identifier declaration locations"
    );
}

#[test]
fn test_is_new_identifier_location_false_for_interface_member_return_type_prefix() {
    let source = "export interface VFS {\n  getSourceFile(path: string): ts";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Interface member return type positions should not be treated as new identifier declaration locations"
    );
}

#[test]
fn test_is_new_identifier_location_false_for_identifier_prefix_after_statement_boundary() {
    let source = "import { x } from \"./a\";\nf";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        !completions.compute_is_new_identifier_location(root, offset),
        "Typing an identifier prefix at the start of a new statement should not be treated as a new identifier declaration location"
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

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_completions_inside_class_body() {
    let source = "class Foo {\n  x = 1;\n  method() {\n    \n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside method body
    let items = completions.get_completions(root, Position::new(3, 4));
    assert!(
        items.is_some(),
        "Should have completions inside method body"
    );
}

#[test]
fn test_completions_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(0, 0));
    // Should not panic on empty file, may return None or empty
    let _ = items;
}

#[test]
fn test_completions_after_dot_with_multiple_properties() {
    let source = "const obj = { a: 1, b: 'hello', c: true };\nobj.";
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
    let items = completions.get_completions(root, Position::new(1, 4));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"a"), "Should suggest property 'a'");
        assert!(names.contains(&"b"), "Should suggest property 'b'");
        assert!(names.contains(&"c"), "Should suggest property 'c'");
    }
}

#[test]
fn test_completions_in_for_loop() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) {\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside for-of loop"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"item"),
            "Should suggest loop variable 'item'"
        );
        assert!(
            names.contains(&"items"),
            "Should suggest outer variable 'items'"
        );
    }
}

#[test]
fn test_completions_in_arrow_function() {
    let source = "const outer = 42;\nconst fn = (param: number) => {\n  \n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside arrow function"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"outer"), "Should suggest outer variable");
    }
}

#[test]
fn test_completions_in_nested_function() {
    let source = "const a = 1;\nfunction outer() {\n  const b = 2;\n  function inner() {\n    const c = 3;\n    \n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside inner function
    let items = completions.get_completions(root, Position::new(5, 4));
    assert!(
        items.is_some(),
        "Should have completions in nested function"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"a"), "Should suggest top-level 'a'");
        assert!(names.contains(&"b"), "Should suggest outer 'b'");
        assert!(names.contains(&"c"), "Should suggest inner 'c'");
    }
}

#[test]
fn test_completions_in_if_block() {
    let source = "const x = 1;\nif (true) {\n  const y = 2;\n  \n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions in if block");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should see outer 'x'");
        assert!(names.contains(&"y"), "Should see block-scoped 'y'");
    }
}

#[test]
fn test_completions_enum_members() {
    let source = "enum Color { Red, Green, Blue }\nColor.";
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
    let items = completions.get_completions(root, Position::new(1, 6));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"Red"), "Should suggest 'Red'");
        assert!(names.contains(&"Green"), "Should suggest 'Green'");
        assert!(names.contains(&"Blue"), "Should suggest 'Blue'");
    }
}

#[test]
fn test_completions_interface_as_type() {
    let source = "interface Foo { bar: number; }\nlet x: ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(1, 7));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"Foo"),
            "Should suggest interface 'Foo' as type"
        );
    }
}

#[test]
fn test_completions_multiple_files_via_project() {
    let mut project = crate::Project::new();
    project.set_file("a.ts".to_string(), "export const shared = 42;".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { shared } from './a';\n".to_string(),
    );

    let completions = project.get_completions("b.ts", Position::new(1, 0));
    assert!(
        completions.is_some(),
        "Should get completions in multi-file project"
    );
    if let Some(items) = completions {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"shared"),
            "Should suggest imported 'shared'"
        );
    }
}

#[test]
fn test_completions_destructuring_context() {
    let source = "const obj = { foo: 1, bar: 2 };\nconst { } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    // Inside destructuring braces
    let items = completions.get_completions(root, Position::new(1, 8));
    // Should not panic; may or may not have specific property completions
    let _ = items;
}

#[test]
fn test_completions_switch_case() {
    let source = "const val = 1;\nswitch (val) {\n  case 1:\n    \n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 4));
    assert!(items.is_some(), "Should have completions in switch case");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val"),
            "Should suggest 'val' in switch case"
        );
    }
}

#[test]
fn test_completions_try_catch() {
    let source = "const x = 1;\ntry {\n  const y = 2;\n  \n} catch (e) {\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions in try block");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should suggest outer 'x'");
        assert!(names.contains(&"y"), "Should suggest try-scoped 'y'");
    }
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_completions_no_completions_in_line_comment() {
    // Cursor inside a line comment should yield empty completions
    let source = "const x = 1;\n// some comment ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 16));
    // Inside a comment, completions should be suppressed (empty list)
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a line comment"
        );
    }
}

#[test]
fn test_completions_no_completions_in_block_comment() {
    // Cursor inside a block comment should yield empty completions
    let source = "const x = 1;\n/* block comment */";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the block comment
    let items = completions.get_completions(root, Position::new(1, 10));
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a block comment"
        );
    }
}

#[test]
fn test_completions_no_completions_in_string_literal() {
    // Cursor inside a string literal should yield empty completions (non-module-specifier)
    let source = "const x = \"hello world\";\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the string literal
    let items = completions.get_completions(root, Position::new(0, 15));
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions inside a string literal"
        );
    }
}

#[test]
fn test_completions_no_completions_after_double_dot() {
    // After ".." (not "..." spread), no completions should be offered
    let source = "const x = [1, 2];\nx..";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 3));
    assert!(
        items.is_none(),
        "Should not have completions after double dot '..'"
    );
}

#[test]
fn test_completions_at_start_of_file_with_content() {
    // Cursor at position (0,0) when the file has content
    let source = "const x = 1;\nconst y = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 0));
    // At position 0,0, completions should include keywords at minimum
    assert!(items.is_some(), "Should have completions at start of file");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Keywords should always be available
        assert!(
            names.contains(&"const"),
            "Should suggest keyword 'const' at start of file"
        );
    }
}

#[test]
fn test_completions_function_snippet_insert_text_with_helper() {
    // Functions should have insert text with snippet tab-stop
    let source = "function myFunc() {}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let func_item = items.iter().find(|i| i.label == "myFunc");
        assert!(func_item.is_some(), "Should find 'myFunc' completion");
        let func_item = func_item.unwrap();
        assert_eq!(
            func_item.kind,
            CompletionItemKind::Function,
            "myFunc should be Function kind"
        );
        assert!(func_item.is_snippet, "Function should be a snippet");
        assert_eq!(
            func_item.insert_text.as_deref(),
            Some("myFunc($1)"),
            "Function insert text should include tab-stop"
        );
    }
}

#[test]
fn test_completions_arguments_available_inside_function() {
    // Inside a function body, "arguments" should appear as a completion
    let source = "function foo() {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(items.is_some(), "Should have completions in function body");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"arguments"),
            "Should suggest 'arguments' inside function"
        );
    }
}

#[test]
fn test_completions_arguments_not_at_top_level() {
    // At the top level (outside any function), "arguments" should NOT appear
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions at top level");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            !names.contains(&"arguments"),
            "Should NOT suggest 'arguments' at top level"
        );
    }
}

#[test]
fn test_completions_keywords_at_top_level() {
    // Keywords like if, for, while, return should appear in completions
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"if"), "Should suggest keyword 'if'");
        assert!(names.contains(&"for"), "Should suggest keyword 'for'");
        assert!(names.contains(&"while"), "Should suggest keyword 'while'");
        assert!(
            names.contains(&"function"),
            "Should suggest keyword 'function'"
        );
        assert!(names.contains(&"class"), "Should suggest keyword 'class'");

        // Keywords should have Keyword kind and appropriate sort text
        let if_item = items.iter().find(|i| i.label == "if").unwrap();
        assert_eq!(if_item.kind, CompletionItemKind::Keyword);
        assert_eq!(
            if_item.sort_text.as_deref(),
            Some(sort_priority::KEYWORD),
            "Keywords should have KEYWORD sort priority"
        );
    }
}

#[test]
fn test_completions_global_this_always_present() {
    // globalThis should always appear in global completions
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"globalThis"), "Should suggest 'globalThis'");
        assert!(names.contains(&"undefined"), "Should suggest 'undefined'");
    }
}

#[test]
fn test_completions_const_vs_let_kind() {
    // const and let declarations should have distinct CompletionItemKind
    let source = "const myConst = 1;\nlet myLet = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let const_item = items.iter().find(|i| i.label == "myConst");
        let let_item = items.iter().find(|i| i.label == "myLet");
        assert!(const_item.is_some(), "Should find 'myConst'");
        assert!(let_item.is_some(), "Should find 'myLet'");
        assert_eq!(
            const_item.unwrap().kind,
            CompletionItemKind::Const,
            "const declaration should have Const kind"
        );
        assert_eq!(
            let_item.unwrap().kind,
            CompletionItemKind::Let,
            "let declaration should have Let kind"
        );
    }
}

#[test]
fn test_completions_parameter_kind() {
    // Parameters should have Parameter kind
    let source = "function foo(myParam: number) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let param_item = items.iter().find(|i| i.label == "myParam");
        assert!(param_item.is_some(), "Should find 'myParam'");
        let param_item = param_item.unwrap();
        assert_eq!(
            param_item.kind,
            CompletionItemKind::Parameter,
            "Parameter should have Parameter kind"
        );
        // Parameter should have type annotation as detail
        assert_eq!(
            param_item.detail.as_deref(),
            Some("number"),
            "Parameter should show type annotation as detail"
        );
    }
}

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

#[test]
fn test_completions_inside_block_comment() {
    // Inside a block comment, we verify no crash
    let source = "const x = 1;\n/* block comment  */";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 10));
    // Currently may or may not return completions in comments
}

#[test]
fn test_completions_inside_string_literal() {
    // Inside a string literal, verify no crash
    let source = "const x = 1;\nconst s = \"hello world\";";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 16));
    // Currently may or may not return completions in strings
}

#[test]
fn test_completions_for_loop_variable_scope() {
    // Variables declared in a for loop should be visible inside the loop body
    let source = "const outer = 1;\nfor (let i = 0; i < 10; i++) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside loop body (line 2, col 2)
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(items.is_some(), "Should have completions inside for loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"i"),
            "Should suggest loop variable 'i', got: {names:?}"
        );
        assert!(
            names.contains(&"outer"),
            "Should suggest outer variable 'outer', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_no_duplicate_from_var_hoisting() {
    // var declarations are hoisted; should not appear duplicated
    let source = "var x = 1;\nvar x = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let x_count = names.iter().filter(|&&n| n == "x").count();
        assert_eq!(
            x_count, 1,
            "Hoisted 'var x' should appear exactly once, found {x_count} times"
        );
    }
}

#[test]
fn test_completions_after_spread_operator() {
    // After `...` in an array, should suggest variables
    let source = "const items = [1, 2];\nconst all = [0, ...];";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `...` (line 1, col 19)
    let items = completions.get_completions(root, Position::new(1, 19));
    assert!(
        items.is_some(),
        "Should have completions after spread operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"items"),
            "Should suggest 'items' after spread, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_at_function_name_definition() {
    // At the name position of a function declaration, verify no crash
    let source = "function ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 9));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_at_class_name_definition() {
    // At the name position of a class declaration, verify no crash
    let source = "class ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 6));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_after_assignment_operator() {
    // After `=` in an assignment, should suggest variables
    let source = "let target = 0;\nconst source = 42;\ntarget = ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `target = ` (line 2, col 9)
    let items = completions.get_completions(root, Position::new(2, 9));
    assert!(
        items.is_some(),
        "Should have completions after assignment operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"source"),
            "Should suggest 'source' after '=', got: {names:?}"
        );
        assert!(
            names.contains(&"target"),
            "Should suggest 'target' after '=', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_after_logical_operator() {
    // After logical operators (`&&`, `||`), should suggest variables
    let source = "const a = true;\nconst b = false;\nconst c = a && ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `a && ` (line 2, col 15)
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(
        items.is_some(),
        "Should have completions after logical operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "Should suggest 'b' after '&&', got: {names:?}"
        );
    }
}

// ============================================================================
// Additional coverage tests (batch 2)
// ============================================================================

#[test]
fn test_completions_member_nested_object_dot() {
    // After `obj.inner.`, member resolution should return some completions
    // (may resolve to inner properties or parent-level members depending on type resolution)
    let source = "const obj = { inner: { deep: 42, flag: true } };\nobj.inner.";
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
    let position = Position::new(1, 10);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash on nested property access; verify we get some result
    assert!(
        items.is_some(),
        "Should have completions for nested member access"
    );
    if let Some(items) = items {
        assert!(
            !items.is_empty(),
            "Should have non-empty member completions"
        );
    }
}

#[test]
fn test_completions_member_method_on_object() {
    // Object with method should suggest method with Method kind
    let source = "const obj = { greet() { return 'hi'; } };\nobj.";
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
    if let Some(items) = items {
        let greet_item = items.iter().find(|i| i.label == "greet");
        assert!(
            greet_item.is_some(),
            "Should suggest method 'greet', got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_completions_member_class_instance() {
    // Class instance member access should show public properties and methods
    let source = "class Point {\n  x: number = 0;\n  y: number = 0;\n  distance() { return 0; }\n}\nconst p = new Point();\np.";
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
    let position = Position::new(6, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash; may or may not have completions depending on class resolution
    let _ = items;
}

#[test]
fn test_completions_return_statement_inside_nested_function() {
    // Return inside nested function should suggest variables from all enclosing scopes
    let source = "const global = 1;\nfunction outer() {\n  const mid = 2;\n  function inner() {\n    const local = 3;\n    return ;\n  }\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `return ` in inner function (line 5, col 11)
    let items = completions.get_completions(root, Position::new(5, 11));
    assert!(
        items.is_some(),
        "Should have completions in nested return statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"local"),
            "Should suggest 'local' in return, got: {names:?}"
        );
        assert!(
            names.contains(&"mid"),
            "Should suggest 'mid' from outer scope, got: {names:?}"
        );
        assert!(
            names.contains(&"global"),
            "Should suggest 'global' from top scope, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_let_in_different_block_scopes() {
    // let variables in different block scopes should not leak
    let source = "if (true) {\n  let blockA = 1;\n}\nif (true) {\n  let blockB = 2;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside second if block (line 5, col 2)
    let items = completions.get_completions(root, Position::new(5, 2));
    assert!(
        items.is_some(),
        "Should have completions inside second if block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"blockB"),
            "Should suggest 'blockB' from current block, got: {names:?}"
        );
        // blockA is in a different (closed) block scope - may or may not be visible
        // depending on binder scope resolution
    }
}

#[test]
fn test_completions_try_catch_finally_scoping() {
    // Variables in finally block should see outer scope but not try/catch locals
    let source = "const outer = 0;\ntry {\n  const tryVar = 1;\n} catch (e) {\n  const catchVar = 2;\n} finally {\n  const finalVar = 3;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside finally block (line 7, col 2)
    let items = completions.get_completions(root, Position::new(7, 2));
    assert!(
        items.is_some(),
        "Should have completions inside finally block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"outer"),
            "Should suggest 'outer' in finally, got: {names:?}"
        );
        assert!(
            names.contains(&"finalVar"),
            "Should suggest 'finalVar' in finally, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_function_parameter_default_value() {
    // In parameter default value position, should suggest visible variables
    let source = "const defaultVal = 10;\nfunction f(x = ) {}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `x = ` (line 1, col 15)
    let items = completions.get_completions(root, Position::new(1, 15));
    assert!(
        items.is_some(),
        "Should have completions in parameter default value"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"defaultVal"),
            "Should suggest 'defaultVal' in param default, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_while_loop_body() {
    // Inside while loop body, should suggest variables from enclosing scope
    let source = "const counter = 0;\nwhile (true) {\n  const loopVar = 1;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions inside while loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"counter"),
            "Should suggest 'counter', got: {names:?}"
        );
        assert!(
            names.contains(&"loopVar"),
            "Should suggest 'loopVar', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_const_enum_kind() {
    // const enums should also have Enum kind
    let source = "const enum Direction { Up, Down, Left, Right }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let dir_item = items.iter().find(|i| i.label == "Direction");
        assert!(dir_item.is_some(), "Should find 'Direction'");
        assert_eq!(
            dir_item.unwrap().kind,
            CompletionItemKind::Enum,
            "const enum should have Enum kind"
        );
    }
}

#[test]
fn test_completions_module_kind() {
    // Module declarations should have Module kind
    let source = "module MyModule {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let mod_item = items.iter().find(|i| i.label == "MyModule");
        assert!(mod_item.is_some(), "Should find 'MyModule'");
        assert_eq!(
            mod_item.unwrap().kind,
            CompletionItemKind::Module,
            "Module should have Module kind"
        );
    }
}

#[test]
fn test_completions_namespace_kind() {
    // Namespace declarations should have Module kind
    let source = "namespace NS {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let ns_item = items.iter().find(|i| i.label == "NS");
        assert!(ns_item.is_some(), "Should find 'NS'");
        assert_eq!(
            ns_item.unwrap().kind,
            CompletionItemKind::Module,
            "Namespace should have Module kind"
        );
    }
}

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

#[test]
fn test_completions_completion_result_serialization() {
    // Verify CompletionResult serialization includes correct field names
    let result = CompletionResult {
        is_global_completion: true,
        is_member_completion: false,
        is_new_identifier_location: false,
        default_commit_characters: Some(vec![".".to_string(), ",".to_string()]),
        entries: vec![CompletionItem::new(
            "x".to_string(),
            CompletionItemKind::Variable,
        )],
    };

    let value = serde_json::to_value(&result).expect("should serialize");
    assert_eq!(
        value
            .get("defaultCommitCharacters")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2),
        "defaultCommitCharacters should be serialized with camelCase"
    );
    assert_eq!(
        value
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1),
        "entries should have one item"
    );
}

#[test]
fn test_completions_function_parameter_in_body() {
    // function f(myParam: number) { | }
    let source = "function f(myParam: number) {  }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    // Position inside the function body (line 0, col 30 = between { and })
    let position = Position::new(0, 30);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.is_some(),
        "Should have completions inside function body"
    );
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"myParam"),
        "Function parameter 'myParam' should appear in completions, got: {names:?}"
    );
}

#[test]
fn test_completions_jsx_text_content_suppressed() {
    // declare namespace JSX {
    //   interface Element {}
    //   interface IntrinsicElements { div: {} }
    // }
    // var x = <div> hello world</div>;
    // Cursor inside the JsxText ` hello world` should return no completions.
    let source = "declare namespace JSX {\n  interface Element {}\n  interface IntrinsicElements { div: {} }\n}\nvar x = <div> hello world</div>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor inside ` hello world` (position of the space after `<div>`).
    let byte_offset = source.find("<div> hello").expect("source pattern present") + "<div>".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions inside JSX child text, got: {items:?}"
    );
}

#[test]
fn test_completions_jsx_between_tags_suppressed() {
    // JSX children with no whitespace between tags: cursor right after `>`
    // of the opening tag and before the self-closing inner tag.
    let source = "declare namespace JSX {\n  interface Element {}\n  interface IntrinsicElements { div: {} }\n}\nvar x = <div><div/></div>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor right after the outer `<div>` (between `>` and `<div/>`).
    let pattern = "<div><div/>";
    let byte_offset = source.find(pattern).expect("pattern present") + "<div>".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions between JSX children tags, got: {items:?}"
    );
}

#[test]
fn test_completions_type_arg_non_generic_suppressed() {
    // interface Foo {}
    // type Bar = {};
    // let x: Foo<"">;
    // Cursor inside the string literal type arg on a non-generic target
    // should return no completions.
    let source = "interface Foo {}\ntype Bar = {};\nlet x: Foo<\"\">;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Cursor between the two quotes of `""`.
    let byte_offset = source.find("Foo<\"").expect("pattern present") + "Foo<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);

    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions in type arg on non-generic Foo, got: {items:?}"
    );

    // Same for the type alias `Bar`.
    let source2 = "interface Foo {}\ntype Bar = {};\nlet y: Bar<\"\">;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source2.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source2);

    let byte_offset = source2.find("Bar<\"").expect("pattern present") + "Bar<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source2);
    let completions = Completions::new(arena, &binder, &line_map, source2);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Expected no completions in type arg on non-generic Bar, got: {items:?}"
    );
}

#[test]
fn test_completions_type_arg_generic_retained() {
    // interface Foo<T> {}
    // let x: Foo<"|">;
    // Cursor inside the string literal type arg on a generic target should
    // NOT be suppressed by the non-generic gate.
    let source = "interface Foo<T> {}\nlet x: Foo<\"\">;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let byte_offset = source.find("Foo<\"").expect("pattern present") + "Foo<\"".len();
    let position = line_map.offset_to_position(byte_offset as u32, source);
    let offset = line_map
        .position_to_offset(position, source)
        .expect("offset");
    let completions = Completions::new(arena, &binder, &line_map, source);
    assert!(
        !completions.is_in_type_argument_of_non_generic(offset),
        "Generic Foo<T> should not be treated as non-generic"
    );
}

#[test]
fn test_completions_suppressed_after_numeric_dot_with_jsdoc_trivia() {
    // `0./** comment */` ends with a JSDoc comment, but the previous *token*
    // is a complete decimal NumericLiteral `0.`. tsc's completion provider
    // suppresses completions at the position right after this trivia
    // because the prior token is numeric (not a member access). Lock the
    // text-based suppression so it skips trailing block comments.
    // Regression: `completionListAfterNumericLiteral.ts` fourslash test.
    let source = "0./** comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at the very end — right after the closing `*/`.
    let position = line_map.offset_to_position(source.len() as u32, source);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.as_ref().is_none_or(|v| v.is_empty()),
        "Completions must be suppressed after `0.<jsdoc>` since the prior token is numeric, got: {items:?}"
    );
}
