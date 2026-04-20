#[test]
fn test_quickfix_add_missing_import_merge_named_same_module() {
    let source = "import { bar } from \"./foo\";\nfoo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
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
                "foo".to_string(),
                "foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import { bar, foo } from \"./foo\";\nfoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_merge_named_ignore_case_true() {
    let source = "import { A, B, C } from \"./exports1\";\na;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "a");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'a'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let actions = provider.provide_code_actions(
        root,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: vec![ImportCandidate::named(
                "./exports1".to_string(),
                "a".to_string(),
                "a".to_string(),
            )],
        },
    );

    let edit = actions[0].edit.as_ref().unwrap();
    let updated = apply_text_edits(source, &line_map, &edit.changes["test.ts"]);
    assert_eq!(updated, "import { a, A, B, C } from \"./exports1\";\na;\n");
}

#[test]
fn test_quickfix_add_missing_import_merge_named_ignore_case_false() {
    let source = "import { E } from \"./exports2\";\nd;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "d");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'd'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source)
            .with_organize_imports_ignore_case(false);

    let actions = provider.provide_code_actions(
        root,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: vec![ImportCandidate::named(
                "./exports2".to_string(),
                "d".to_string(),
                "d".to_string(),
            )],
        },
    );

    let edit = actions[0].edit.as_ref().unwrap();
    let updated = apply_text_edits(source, &line_map, &edit.changes["test.ts"]);
    assert_eq!(updated, "import { E, d } from \"./exports2\";\nd;\n");
}

#[test]
fn test_quickfix_add_missing_import_merge_named_multiline() {
    let source = "import {\n  bar\n} from \"./foo\";\nfoo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "foo");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
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
                "foo".to_string(),
                "foo".to_string(),
            )],
        },
    );

    assert!(!actions.is_empty(), "expected at least one quickfix action");
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import {\n  bar,\n  foo\n} from \"./foo\";\nfoo();\n"
    );
}

#[test]
fn test_quickfix_add_missing_import_merge_named_with_default() {
    let source = "import Foo from \"./foo\";\nbar();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        "import type { Foo } from \"./foo\";\n\nlet x: Foo;\n"
    );
}

