#[test]
fn test_quickfix_add_missing_import_merge_named_with_default() {
    let source = "import Foo from \"./foo\";\nbar();\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "bar");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'bar'.".to_string(),
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
                "bar".to_string(),
                "bar".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import Foo, { bar } from \"./foo\";\nbar();\n");
}

#[test]
fn test_quickfix_add_missing_import_merge_default_with_named() {
    let source = "import { bar } from \"./foo\";\nFoo();\n";
    let (parser, root) = parse_test_source(source);
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
            import_candidates: vec![ImportCandidate::default(
                "./foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import Foo, { bar } from \"./foo\";\nFoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_merge_default_with_namespace() {
    let source = "import * as ns from \"./foo\";\nFoo();\n";
    let (parser, root) = parse_test_source(source);
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
            import_candidates: vec![ImportCandidate::default(
                "./foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import Foo, * as ns from \"./foo\";\nFoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_default() {
    let source = "Foo();\n";
    let (parser, root) = parse_test_source(source);
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
            import_candidates: vec![ImportCandidate::default(
                "./foo".to_string(),
                "Foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import Foo from \"./foo\";\n\nFoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_namespace() {
    let source = "ns.foo;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "ns");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'ns'.".to_string(),
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
            import_candidates: vec![ImportCandidate::namespace(
                "./foo".to_string(),
                "ns".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import * as ns from \"./foo\";\n\nns.foo;\n");
}

#[test]
fn test_quickfix_add_missing_import_type_position_uses_import_type() {
    let source = "let x: Foo;\n";
    let (parser, root) = parse_test_source(source);
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
        "import type { Foo } from \"./foo\";\n\nlet x: Foo;\n"
    );
}
