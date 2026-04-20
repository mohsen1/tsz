#[test]
fn test_extract_variable_property_access() {
    let source = "const x = foo.bar.baz + 1;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.tsx".to_string(), source);

    let range = Range {
        start: Position::new(0, 10),
        end: Position::new(0, 21),
    };

    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    let action = actions
        .iter()
        .find(|a| a.title == "Extract to constant 'extracted'")
        .expect("expected extract action");
    assert_eq!(action.kind, CodeActionKind::RefactorExtract);
    assert!(action.is_preferred);

    let edit = action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.tsx"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted = foo.bar.baz;"));
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_avoids_name_collision() {
    let source = "const extracted = 1;\nconst value = foo.bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "foo.bar");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    let action = actions
        .iter()
        .find(|a| a.title == "Extract to constant 'extracted2'")
        .expect("expected extract action");

    let edit = action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted2 = foo.bar;"));
    assert_eq!(edits[1].new_text, "extracted2");
}

#[test]
fn test_extract_variable_parenthesizes_comma_expression() {
    let source = "const value = (foo(), bar());";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "foo(), bar()");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(
        edits[0]
            .new_text
            .contains("const extracted = (foo(), bar());")
    );
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_parenthesizes_comma_expression_with_parens() {
    let source = "const value = (foo(), bar());";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "(foo(), bar())");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(
        edits[0]
            .new_text
            .contains("const extracted = (foo(), bar());")
    );
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_preserves_parenthesized_replacement() {
    let source = "const value = (foo + bar) * baz;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "(foo + bar)");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted = (foo + bar);"));
    assert_eq!(edits[1].new_text, "(extracted)");
}

#[test]
fn test_extract_variable_preserves_parenthesized_conditional_replacement() {
    let source = "const value = (foo ? bar : baz) + qux;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "(foo ? bar : baz)");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(
        edits[0]
            .new_text
            .contains("const extracted = (foo ? bar : baz);")
    );
    assert_eq!(edits[1].new_text, "(extracted)");
}

#[test]
fn test_extract_variable_call_expression_span() {
    let source = "const value = foo() * bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "foo()");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted = foo();"));
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_array_literal_span() {
    let source = "const value = [foo] + bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "[foo]");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted = [foo];"));
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_object_literal_span() {
    let source = "const value = { foo: 1 } + bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "{ foo: 1 }");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);

    assert!(edits[0].new_text.contains("const extracted = { foo: 1 };"));
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_jsx_child_wraps_expression() {
    let source = "const view = <div><Foo /></div>;\n";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.tsx".to_string(), source);

    let range = range_for_substring(source, &line_map, "<Foo />");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty());

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.tsx"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "const extracted = <Foo />;\nconst view = <div>{extracted}</div>;\n"
    );
}

