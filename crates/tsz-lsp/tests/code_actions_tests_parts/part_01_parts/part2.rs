#[test]
fn test_extract_variable_in_function_body() {
    let source = "function f() {\n  const result = a.b + c.d;\n}\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

    let action = actions
        .iter()
        .find(|a| a.title.starts_with("Remove import from"))
        .expect("expected a 'Remove import from <module>' quickfix for TS6192");
    assert_eq!(action.title, "Remove import from './mod'");
    let edits = &action.edit.as_ref().unwrap().changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
    let result = apply_text_edits(source, &line_map, edits);
    assert_eq!(result, "const x = 1;\n");
}

#[test]
fn test_quickfix_unused_import_remove_diag_at_decl_start() {
    // tsserver anchors TS6192 at the start of the import declaration; the
    // diagnostic does not cover any specifier identifier. Issue #4024.
    let source = "import { readFile, writeFile } from \"./b\";\nconsole.log(1);\n";
    let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let import_end = source.find(";\n").unwrap() + 1;
    let range = range_for_offset(source, &line_map, 0, import_end);
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

    let provider = CodeActionProvider::new(arena, &binder, &line_map, "a.ts".to_string(), source);
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    );

    let action = actions
        .iter()
        .find(|a| a.title.starts_with("Remove import from"))
        .expect("expected a 'Remove import from <module>' quickfix for TS6192");
    assert_eq!(action.title, "Remove import from './b'");
    let edits = &action.edit.as_ref().unwrap().changes["a.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
    let result = apply_text_edits(source, &line_map, edits);
    assert_eq!(result, "console.log(1);\n");
}

#[test]
fn test_quickfix_unused_variable_prefix() {
    let source = "const unused = 1;\n";
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_code_actions_on_empty_file() {
    let source = "";
    let (parser, root) = parse_test_source(source);
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
fn test_extract_variable_ternary_full() {
    let source = "const x = a > b ? a : b;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "a > b ? a : b");
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
        "Should offer extract for ternary expression"
    );
}

#[test]
fn test_extract_variable_object_literal() {
    let source = "const x = { a: 1, b: 2 };";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let range = range_for_substring(source, &line_map, "{ a: 1, b: 2 }");
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        },
    );

    // Just verify no crash - object literals may or may not be extractable
    let _ = actions;
}
