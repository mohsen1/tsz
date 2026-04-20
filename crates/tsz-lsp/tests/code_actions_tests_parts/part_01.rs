#[test]
fn test_extract_variable_blocks_tdz_for_loop_initializer() {
    let source = "for (let i = 0; i < limit; i++) { console.log(i); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "i < limit");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_blocks_tdz_in_jsx_tag() {
    let source = "const view = <Widget />;\nconst Widget = () => null;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.tsx".to_string(), source);

    let range = range_for_substring(source, &line_map, "<Widget />");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_blocks_tdz_in_jsx_attribute() {
    let source = "const view = <div value={Value} />;\nconst Value = 1;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.tsx".to_string(), source);

    let range = range_for_substring(source, &line_map, "<div value={Value} />");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_blocks_tdz_in_jsx_child() {
    let source = "const view = <div>{Value}</div>;\nconst Value = 1;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.tsx".to_string(), source);

    let range = range_for_substring(source, &line_map, "<div>{Value}</div>");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_no_action_cross_scope() {
    let source = "const result = ((x) => x + 1)(2);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "x + 1");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::RefactorExtract]),
            import_candidates: Vec::new(),
        },
    );

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_no_action_for_simple_literal() {
    let source = "const x = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = Range {
        start: Position::new(0, 10),
        end: Position::new(0, 12),
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

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_extract_variable_empty_range() {
    let source = "const x = foo.bar.baz;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = Range {
        start: Position::new(0, 10),
        end: Position::new(0, 10),
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

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "should not have extract variable action"
    );
}

#[test]
fn test_organize_imports_sort_only() {
    let source = "import { b } from \"b\";\nimport { a } from \"a\";\nconst x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    };

    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::SourceOrganizeImports]),
            import_candidates: Vec::new(),
        },
    );

    let organize_action = actions
        .iter()
        .find(|a| a.kind == CodeActionKind::SourceOrganizeImports)
        .expect("Should offer organize imports action");
    let edit = organize_action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);

    let new_text = &edits[0].new_text;
    let pos_a = new_text.find("import { a } from \"a\";").unwrap();
    let pos_b = new_text.find("import { b } from \"b\";").unwrap();
    assert!(
        pos_a < pos_b,
        "Imports should be sorted by module specifier"
    );
}

#[test]
fn test_quickfix_remove_unused_named_import() {
    let source = "import { foo, bar } from \"mod\";\nbar;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "foo");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let empty_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        empty_range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "import { bar } from \"mod\";\n");
}

#[test]
fn test_quickfix_remove_unused_named_import_entire_decl() {
    let source = "import { foo } from \"mod\";\nconst x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "foo");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let empty_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        empty_range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}

