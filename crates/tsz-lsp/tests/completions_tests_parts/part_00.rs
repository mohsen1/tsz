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

