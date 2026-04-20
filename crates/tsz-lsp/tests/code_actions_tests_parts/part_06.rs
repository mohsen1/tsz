#[test]
fn test_codefix_registry_fix_all_description_present() {
    // All fixes should have fix_all_description
    for code in &[2304u32, 2339, 2552, 6133, 1308, 4114, 2420, 7027] {
        let fixes = CodeFixRegistry::fixes_for_error_code(*code);
        for (fix_name, fix_id, _desc, fix_all_desc) in &fixes {
            assert!(
                !fix_name.is_empty(),
                "fix_name should not be empty for code {code}"
            );
            assert!(
                !fix_id.is_empty(),
                "fix_id should not be empty for code {code}"
            );
            assert!(
                !fix_all_desc.is_empty(),
                "fix_all_description should not be empty for code {code}"
            );
        }
    }
}

#[test]
fn test_codefix_registry_strict_class_init_2564() {
    // Property has no initializer and is not definitely assigned (2564)
    let fixes = CodeFixRegistry::fixes_for_error_code(2564);
    assert!(
        fixes.len() >= 2,
        "Should return multiple fixes for error 2564"
    );
    let fix_names: Vec<&str> = fixes.iter().map(|f| f.0).collect();
    assert!(fix_names.contains(&"addMissingPropertyDefiniteAssignmentAssertions"));
    assert!(fix_names.contains(&"addMissingPropertyUndefinedType"));
}

// =============================================================================
// Additional Coverage Tests
// =============================================================================

#[test]
fn test_extract_variable_call_expression() {
    // Extract a function call expression
    let source = "const x = getValue() + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "getValue() + 1");
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
        .find(|a| a.kind == CodeActionKind::RefactorExtract)
        .expect("expected extract action");
    let edit = action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);
    // Declaration should be inserted before the statement
    assert!(edits[0].new_text.contains("const extracted ="));
    // Original expression should be replaced with the variable name
    assert_eq!(edits[1].new_text, "extracted");
}

#[test]
fn test_extract_variable_conditional_expression() {
    // Extract a conditional (ternary) expression
    let source = "const x = a ? b() : c();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "a ? b() : c()");
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
        .find(|a| a.kind == CodeActionKind::RefactorExtract)
        .expect("expected extract action");
    let edit = action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);
    assert!(edits[0].new_text.contains("const extracted ="));
}

#[test]
fn test_extract_variable_nested_in_function() {
    // Extract an expression inside a function body
    let source = "function foo() {\n  const x = a.b + c.d;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "a.b + c.d");
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
        .find(|a| a.kind == CodeActionKind::RefactorExtract)
        .expect("expected extract action");
    let edit = action.edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 2);
    // The declaration should have the same indentation as the inner statement
    assert!(edits[0].new_text.contains("const extracted = a.b + c.d;"));
}

#[test]
fn test_code_actions_empty_file() {
    // An empty file should produce no code actions for any request
    let source = "";
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

    assert!(
        !actions
            .iter()
            .any(|a| a.title.starts_with("Extract to constant")),
        "Empty file should produce no extract variable actions"
    );
}

#[test]
fn test_code_actions_at_file_start() {
    // Code actions at the very start of the file (offset 0)
    let source = "import { foo } from \"mod\";\nfoo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Request at position (0, 0) with empty range (no selection)
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

    // With no diagnostics and no selection, we may only get organize imports
    // (if imports are unsorted). For a single import, there's nothing to sort.
    // The key thing is: no panic at file boundary.
    for action in &actions {
        assert!(action.edit.is_some() || action.data.is_some());
    }
}

#[test]
fn test_code_actions_at_file_end() {
    // Code actions at the very end of the file
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the very end of the file
    let range = Range::new(Position::new(1, 0), Position::new(1, 0));
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    // No panic at file end boundary. Actions may or may not exist.
    let _ = actions;
}

#[test]
fn test_quickfix_add_missing_const() {
    // Trigger add_missing_const_quickfix with error code 2304
    let source = "x = 42;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "x");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'x'.".to_string(),
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

    // Should produce an "Add 'const'" quick fix
    let const_action = actions.iter().find(|a| a.title.contains("const"));
    assert!(
        const_action.is_some(),
        "Should produce an 'Add const' quick fix for 'x = 42'"
    );
    let edit = const_action.unwrap().edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "const ");
}

#[test]
fn test_quickfix_add_missing_const_skips_existing_declaration() {
    // Should NOT produce add_missing_const when line already starts with const/let/var
    let source = "const x = unknownVar;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "unknownVar");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'unknownVar'.".to_string(),
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

    // The add_missing_const quickfix should NOT appear because the line starts with "const"
    let const_action = actions.iter().find(|a| a.title.contains("Add 'const'"));
    assert!(
        const_action.is_none(),
        "Should not produce 'Add const' when line already has a declaration keyword"
    );
}

