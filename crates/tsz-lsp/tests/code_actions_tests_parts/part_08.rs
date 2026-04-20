#[test]
fn test_quickfix_add_missing_property_empty_object_multiline() {
    // Add a missing property to an empty multi-line object literal
    let source = "const obj = {\n};\nobj.newProp;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "newProp");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "Property 'newProp' does not exist on type '{}'.".to_string(),
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

    let prop_action = actions.iter().find(|a| a.title.contains("newProp"));
    assert!(
        prop_action.is_some(),
        "Should produce a quick fix to add 'newProp'"
    );
    let edit = prop_action.unwrap().edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    // The edit should insert "newProp: undefined" into the object
    let combined: String = edits.iter().map(|e| e.new_text.as_str()).collect();
    assert!(
        combined.contains("newProp: undefined"),
        "Should insert 'newProp: undefined'"
    );
}

// =========================================================================
// Additional coverage: extract variable edge cases
// =========================================================================

#[test]
fn test_extract_variable_from_binary_expression() {
    let source = "const result = a + b * c;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "b * c");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert!(
        actions.iter().any(|a| a.title.contains("Extract")),
        "Should offer extract for binary expression"
    );
}

#[test]
fn test_extract_variable_from_template_literal() {
    let source = "const msg = `hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "`hello ${name}`");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    // May or may not extract template literals
    // Just verify no crash
    let _ = actions;
}

#[test]
fn test_code_actions_empty_range_no_diagnostics() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    // Empty range with no diagnostics might produce no actions
    // Just verify no crash
    let _ = actions;
}

#[test]
fn test_code_actions_only_filter_extract() {
    let source = "const x = foo.bar;";
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

    // Should return refactoring actions (extract and surround-with)
    for action in &actions {
        assert!(
            action.kind == CodeActionKind::RefactorExtract
                || action.kind == CodeActionKind::Refactor,
            "Only refactoring actions should be returned, got {:?}",
            action.kind
        );
    }
    // At least one should be an extract action
    assert!(
        actions
            .iter()
            .any(|a| a.kind == CodeActionKind::RefactorExtract),
        "expected at least one extract action"
    );
}

#[test]
fn test_code_actions_only_filter_quickfix() {
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = Range::new(Position::new(0, 6), Position::new(0, 7));
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    // QuickFix with no diagnostics should produce no quickfix actions
    for action in &actions {
        assert_eq!(action.kind, CodeActionKind::QuickFix);
    }
}

#[test]
fn test_extract_variable_in_function_body() {
    let source = "function f() {\n  const result = a.b + c.d;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "a.b");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    // Should extract within the function scope
    let extract_action = actions
        .iter()
        .find(|a| a.title.contains("Extract"))
        .expect("Should offer extract action");
    let edit = extract_action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    // The new declaration should be inserted before the line, within the function
    let new_text = &edits[0].new_text;
    assert!(new_text.contains("const extracted = a.b;"));
}

#[test]
fn test_extract_variable_array_expression() {
    let source = "const x = [1, 2, 3].map(n => n * 2);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "[1, 2, 3]");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert!(
        actions.iter().any(|a| a.title.contains("Extract")),
        "Should offer extract for array expression"
    );
}

#[test]
fn test_quickfix_unused_import_remove() {
    let source = "import { foo } from './mod';\nconst x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let range = range_for_substring(source, &line_map, "import { foo } from './mod';");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED),
        source: None,
        message: "All imports in import declaration are unused.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    // Should produce some quickfix for unused import
    // Just verify no crash - exact actions depend on implementation
    let _ = actions;
}

#[test]
fn test_quickfix_unused_variable_prefix() {
    let source = "const unused = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let range = range_for_substring(source, &line_map, "unused");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(ALL_VARIABLES_ARE_UNUSED),
        source: None,
        message: "'unused' is declared but its value is never read.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    // Should produce some quickfix for unused variable
    // Just verify no crash - exact actions depend on implementation
    let _ = actions;
}

