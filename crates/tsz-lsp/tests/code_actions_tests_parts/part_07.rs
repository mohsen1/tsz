#[test]
fn test_quickfix_remove_unused_type_alias() {
    // Test removal of an unused type alias declaration
    let source = "type Unused = string;\ntype Used = number;\nconst x: Used = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Unused");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_VARIABLES_ARE_UNUSED),
        source: None,
        message: "'Unused' is declared but its value is never read.".to_string(),
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
    assert_eq!(actions[0].title, "Remove unused declaration 'Unused'");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}

#[test]
fn test_quickfix_remove_unused_interface() {
    // Test removal of an unused interface declaration
    let source = "interface Unused { x: number; }\nconst y = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Unused");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_VARIABLES_ARE_UNUSED),
        source: None,
        message: "'Unused' is declared but its value is never read.".to_string(),
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
    assert_eq!(actions[0].title, "Remove unused declaration 'Unused'");
}

#[test]
fn test_multiple_overlapping_diagnostics() {
    // Multiple diagnostics at the same location should produce multiple quick fixes
    let source = "import { foo } from \"mod\";\nlet unused = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let import_range = range_for_substring(source, &line_map, "foo");
    let diag_import = LspDiagnostic {
        range: import_range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED),
        source: None,
        message: "All imports in import declaration are unused.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let var_range = range_for_substring(source, &line_map, "unused");
    let diag_var = LspDiagnostic {
        range: var_range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_VARIABLES_ARE_UNUSED),
        source: None,
        message: "'unused' is declared but its value is never read.".to_string(),
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
            diagnostics: vec![diag_import, diag_var],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    // Should have at least 2 quick fixes: one for the import, one for the variable
    assert!(
        actions.len() >= 2,
        "Should produce at least 2 quick fixes for 2 diagnostics, got {}",
        actions.len()
    );

    let titles: Vec<&str> = actions.iter().map(|a| a.title.as_str()).collect();
    assert!(
        titles.iter().any(|t| t.contains("import")),
        "Should have an import removal action"
    );
    assert!(
        titles.iter().any(|t| t.contains("unused")),
        "Should have an unused variable removal action"
    );
}

#[test]
fn test_quickfix_add_missing_import_with_type_only_candidate() {
    // Import a type-only candidate in a type position
    let source = "const x: MyType = {};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "MyType");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'MyType'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let mut candidate = ImportCandidate::named(
        "./types".to_string(),
        "MyType".to_string(),
        "MyType".to_string(),
    );
    candidate.is_type_only = true;

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let empty_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let actions = provider.provide_code_actions(
        root,
        empty_range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: vec![candidate],
        },
    );

    // Should produce an import action for MyType
    let import_action = actions.iter().find(|a| a.title.contains("MyType"));
    assert!(
        import_action.is_some(),
        "Should produce an import action for MyType"
    );
    let edit = import_action.unwrap().edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert!(!edits.is_empty());
    // The import text should contain the type import
    let combined: String = edits.iter().map(|e| e.new_text.as_str()).collect();
    assert!(
        combined.contains("import"),
        "Should generate an import statement"
    );
}

#[test]
fn test_code_action_only_filter_quickfix() {
    // When `only` is set to QuickFix, no refactoring actions should appear
    let source = "const x = foo.bar.baz + 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Select an expression that would normally produce an extract variable action
    let range = range_for_substring(source, &line_map, "foo.bar.baz");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    // No refactoring actions should be returned when only QuickFix is requested
    for action in &actions {
        assert_ne!(
            action.kind,
            CodeActionKind::RefactorExtract,
            "Should not return refactoring actions when only QuickFix is requested"
        );
    }
}

#[test]
fn test_code_action_only_filter_refactor() {
    // When `only` is set to Refactor, no quickfix actions should appear
    let source = "import { foo } from \"mod\";\n";
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
        message: "All imports in import declaration are unused.".to_string(),
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
            only: Some(vec![CodeActionKind::Refactor]),
            import_candidates: Vec::new(),
        },
    );

    // No quickfix actions should be returned when only Refactor is requested
    for action in &actions {
        assert_ne!(
            action.kind,
            CodeActionKind::QuickFix,
            "Should not return quickfix actions when only Refactor is requested"
        );
    }
}

#[test]
fn test_quickfix_no_action_for_irrelevant_diagnostic_code() {
    // A diagnostic with an unrecognized code should not produce any quick fix
    let source = "const x = 1;\n";
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
        code: Some(9999),
        source: None,
        message: "Some unknown error.".to_string(),
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

    assert!(
        actions.is_empty(),
        "Should produce no quick fixes for an unrecognized diagnostic code"
    );
}

#[test]
fn test_quickfix_diagnostic_without_code() {
    // A diagnostic with no error code should produce no quick fixes
    let source = "const x = 1;\n";
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
        code: None,
        source: None,
        message: "Some error without code.".to_string(),
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

    assert!(
        actions.is_empty(),
        "Should produce no quick fixes for a diagnostic without a code"
    );
}

#[test]
fn test_organize_imports_no_action_single_import() {
    // A single import should not produce an organize imports action
    // (nothing to sort)
    let source = "import { foo } from \"mod\";\nfoo();\n";
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
            only: Some(vec![CodeActionKind::SourceOrganizeImports]),
            import_candidates: Vec::new(),
        },
    );

    // A single import has nothing to sort — no organize imports action expected
    assert!(
        !actions
            .iter()
            .any(|a| a.kind == CodeActionKind::SourceOrganizeImports),
        "A single import should not need organize imports"
    );
}

#[test]
fn test_organize_imports_sorts_multiple_imports() {
    // Multiple imports out of order should produce an organize imports action
    let source = "import { z } from \"z-mod\";\nimport { a } from \"a-mod\";\na();\nz();\n";
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
    assert!(!edits.is_empty());
    // After applying edits, "a-mod" should come before "z-mod"
    let result = apply_text_edits(source, &line_map, edits);
    let a_pos = result.find("a-mod").expect("a-mod should be present");
    let z_pos = result.find("z-mod").expect("z-mod should be present");
    assert!(
        a_pos < z_pos,
        "After organizing, 'a-mod' should come before 'z-mod'"
    );
}

