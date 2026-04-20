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
