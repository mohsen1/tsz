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
fn test_completions_member_parameter_typeof_class_with_marker_comment_after_dot() {
    let source = "class C<T> {\n    static foo(x: number) { }\n    x: T;\n}\n\nnamespace C {\n    export function f(x: typeof C) {\n        x./*1*/\n    }\n}\n";
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

    assert!(
        items.is_some(),
        "Should have marker-adjacent member completions"
    );
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
            "Local variable sort text ({}) should be <= keyword sort text ({})",
            var_sort,
            kw_sort
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
                "Should suggest variables in scope, got: {:?}",
                names
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
