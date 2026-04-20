#[test]
fn test_quickfix_add_missing_import_value_skips_type_only_candidate() {
    let source = "Foo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'Foo'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let mut candidate =
        ImportCandidate::named("./foo".to_string(), "Foo".to_string(), "Foo".to_string());
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

    // No import action should be generated for type-only candidates in value position
    // (fix_all actions may still appear for code 2304)
    assert!(
        !actions.iter().any(|a| a.title.starts_with("Import '")),
        "should not generate import action for type-only candidate in value position"
    );
}

#[test]
fn test_quickfix_add_missing_import_type_query_uses_value_import() {
    let source = "type T = typeof Foo;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'Foo'.".to_string(),
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
            import_candidates: vec![ImportCandidate::named(
                "./foo".to_string(),
                "Foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import { Foo } from \"./foo\";\n\ntype T = typeof Foo;\n"
    );
}

#[test]
fn test_quickfix_add_missing_import_class_extends_uses_value_import() {
    let source = "class Bar extends Foo {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'Foo'.".to_string(),
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
            import_candidates: vec![ImportCandidate::named(
                "./foo".to_string(),
                "Foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import { Foo } from \"./foo\";\n\nclass Bar extends Foo {}\n"
    );
}

#[test]
fn test_quickfix_add_missing_import_class_implements_uses_import_type() {
    let source = "class Bar implements Foo {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'Foo'.".to_string(),
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
            import_candidates: vec![ImportCandidate::named(
                "./foo".to_string(),
                "Foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import type { Foo } from \"./foo\";\n\nclass Bar implements Foo {}\n"
    );
}

#[test]
fn test_quickfix_remove_unused_variable_let() {
    let source = "let x = 1;\nlet y = 2;\nconsole.log(y);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "x");
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(ALL_VARIABLES_ARE_UNUSED),
        source: None,
        message: "'x' is declared but its value is never read.".to_string(),
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

#[test]
fn test_quickfix_remove_unused_variable_const() {
    let source = "const unused = 1;\nconst used = 2;\nconsole.log(used);\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "unused");
    let diag = LspDiagnostic {
        range,
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

#[test]
fn test_quickfix_remove_unused_function() {
    let source = "function unused() {}\nfunction used() {}\nused();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "unused");
    let diag = LspDiagnostic {
        range,
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
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    assert_eq!(actions[0].title, "Remove unused declaration 'unused'");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}

#[test]
fn test_quickfix_remove_unused_class() {
    let source = "class Unused {}\nclass Used {}\nnew Used();\n";
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

// =============================================================================
// CodeFixRegistry Tests
// =============================================================================

#[test]
fn test_codefix_registry_spelling_error_2552() {
    // Cannot find name '{0}'. Did you mean '{1}'? (2552)
    let fixes = CodeFixRegistry::fixes_for_error_code(2552);
    assert!(!fixes.is_empty(), "Should return fixes for error 2552");
    assert_eq!(fixes[0].0, "spelling");
    assert_eq!(fixes[0].1, "fixSpelling");
}

#[test]
fn test_codefix_registry_spelling_error_2551() {
    // Property '{0}' does not exist on type '{1}'. Did you mean '{2}'? (2551)
    let fixes = CodeFixRegistry::fixes_for_error_code(2551);
    assert!(!fixes.is_empty(), "Should return fixes for error 2551");
    assert_eq!(fixes[0].0, "spelling");
    assert_eq!(fixes[0].1, "fixSpelling");
}

