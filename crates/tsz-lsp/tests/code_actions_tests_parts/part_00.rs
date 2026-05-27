#[test]
fn test_move_to_file_preserves_type_only_imports_in_target_file() {
    let source = concat!(
        "import type { Foo } from \"./types\";\n",
        "import { type Bar, makeBar } from \"./values\";\n",
        "\n",
        "function use(f: Foo, b: Bar) {\n",
        "  return makeBar(b);\n",
        "}\n",
    );

    let action = move_to_file_action(source, "src/a.ts", "use");
    let edit = action.edit.as_ref().expect("expected workspace edit");
    let target_edits = edit
        .changes
        .get("src/use.ts")
        .expect("expected target file edit");
    let target_text = &target_edits[0].new_text;

    assert!(target_text.contains("import type { Foo } from \"./types\";"));
    assert!(target_text.contains("import { type Bar, makeBar } from \"./values\";"));
    assert!(target_text.contains("export function use(f: Foo, b: Bar)"));
}

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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_extract_variable_blocks_tdz_for_loop_initializer() {
    let source = "for (let i = 0; i < limit; i++) { console.log(i); }";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_quickfix_remove_unused_default_import() {
    let source = "import foo, { bar } from \"mod\";\nbar;\n";
    let (parser, root) = parse_test_source(source);
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
fn test_quickfix_preserves_type_only_named_import() {
    let source = "import { type Foo, Bar } from \"mod\";\nlet x: Foo;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "Bar");
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
    assert_eq!(edits[0].new_text, "import { type Foo } from \"mod\";\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_single_line() {
    let source = "const foo = { a: 1 }; foo.b;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let access_offset = source.find("foo.b").unwrap();
    let prop_offset = access_offset + "foo.".len();
    let range = range_for_offset(source, &line_map, prop_offset, 1);

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "const foo = { a: 1, b: undefined }; foo.b;\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_single_line_trailing_comma() {
    let source = "const foo = { a: 1, }; foo.b;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let access_offset = source.find("foo.b").unwrap();
    let prop_offset = access_offset + "foo.".len();
    let range = range_for_offset(source, &line_map, prop_offset, 1);

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "const foo = { a: 1, b: undefined, }; foo.b;\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_element_access() {
    let source = "const foo = { a: 1 }; foo[\"b\"];\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "\"b\"");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "const foo = { a: 1, \"b\": undefined }; foo[\"b\"];\n"
    );
}

#[test]
fn test_quickfix_add_missing_property_object_literal_multiline() {
    let source = "const foo = {\n  a: 1\n};\nfoo.b;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let access_offset = source.find("foo.b").unwrap();
    let prop_offset = access_offset + "foo.".len();
    let range = range_for_offset(source, &line_map, prop_offset, 1);

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "const foo = {\n  a: 1,\n  b: undefined\n};\nfoo.b;\n"
    );
}

#[test]
fn test_quickfix_add_missing_property_to_class() {
    let source = "class Foo {\n  method() {\n    this.bar;\n  }\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let access_offset = source.find("this.bar").unwrap();
    let prop_offset = access_offset + "this.".len();
    let range = range_for_offset(source, &line_map, prop_offset, 3);

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "class Foo {\n  method() {\n    this.bar;\n  }\n  bar: any;\n}\n"
    );
}

#[test]
fn test_quickfix_add_missing_property_to_class_element_access() {
    let source = "class Foo {\n  method() {\n    this[\"bar\"];\n  }\n}\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, "\"bar\"");

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "missing property".to_string(),
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
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "class Foo {\n  method() {\n    this[\"bar\"];\n  }\n  \"bar\": any;\n}\n"
    );
}

#[test]
fn test_quickfix_add_missing_await_requires_await_legal_context() {
    let cases = [
        (
            "async function can await",
            "async function f() {\n  const p = fetchN();\n  return p.toString();\n}\n",
            true,
        ),
        (
            "async generator can await",
            "async function* f() {\n  const p = fetchN();\n  return p.toString();\n}\n",
            true,
        ),
        (
            "non-async function cannot await",
            "function f() {\n  const p = fetchN();\n  return p.toString();\n}\n",
            false,
        ),
        (
            "non-async generator cannot await",
            "function* f() {\n  const p = fetchN();\n  return p.toString();\n}\n",
            false,
        ),
        (
            "nested async arrow inside generator can await",
            "function* f() {\n  const p = fetchN();\n  const run = async () => p.toString();\n}\n",
            true,
        ),
        (
            "class static block cannot await",
            "class C {\n  static {\n    const p = fetchN();\n    p.toString();\n  }\n}\n",
            false,
        ),
        (
            "class field initializer cannot await",
            "class C {\n  field = fetchN().toString();\n}\n",
            false,
        ),
        (
            "nested async arrow inside class field can await",
            "class C {\n  field = async () => fetchN().toString();\n}\n",
            true,
        ),
        (
            "nested async function inside static block can await",
            "class C {\n  static {\n    async function run() {\n      const p = fetchN();\n      p.toString();\n    }\n  }\n}\n",
            true,
        ),
    ];

    for (label, source, should_offer) in cases {
        let actions = add_missing_await_actions(source, "toString");
        assert_eq!(
            has_add_missing_await_action(&actions),
            should_offer,
            "{label}: actions were {actions:#?}"
        );
    }
}

#[test]
fn test_quickfix_add_missing_import_named() {
    let source = "foo();\n";
    let (parser, root) = parse_test_source(source);
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
    assert_eq!(updated, "import { foo } from \"./foo\";\n\nfoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_after_existing_import() {
    let source = "import { bar } from \"./bar\";\nfoo();\n";
    let (parser, root) = parse_test_source(source);
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
        "import { bar } from \"./bar\";\nimport { foo } from \"./foo\";\nfoo();\n"
    );
}

#[test]
fn test_quickfix_add_missing_import_merge_named_same_module() {
    let source = "import { bar } from \"./foo\";\nfoo();\n";
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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
    let (parser, root) = parse_test_source(source);
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

