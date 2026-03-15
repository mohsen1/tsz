use super::*;
use tsz_binder::BinderState;
use tsz_checker::diagnostics::diagnostic_codes::{
    ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED, ALL_VARIABLES_ARE_UNUSED, CANNOT_FIND_NAME,
    PROPERTY_DOES_NOT_EXIST_ON_TYPE,
};
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

fn range_for_substring(source: &str, line_map: &LineMap, needle: &str) -> Range {
    let start = source.find(needle).expect("substring not found") as u32;
    let end = start + needle.len() as u32;
    let start_pos = line_map.offset_to_position(start, source);
    let end_pos = line_map.offset_to_position(end, source);
    Range::new(start_pos, end_pos)
}

fn range_for_offset(source: &str, line_map: &LineMap, start: usize, len: usize) -> Range {
    let start = start as u32;
    let end = start + len as u32;
    let start_pos = line_map.offset_to_position(start, source);
    let end_pos = line_map.offset_to_position(end, source);
    Range::new(start_pos, end_pos)
}

fn apply_text_edits(source: &str, line_map: &LineMap, edits: &[TextEdit]) -> String {
    let mut result = source.to_string();
    let mut edits_with_offsets: Vec<(usize, usize, &TextEdit)> = edits
        .iter()
        .map(|edit| {
            let start = line_map
                .position_to_offset(edit.range.start, source)
                .unwrap_or(0) as usize;
            let end = line_map
                .position_to_offset(edit.range.end, source)
                .unwrap_or(0) as usize;
            (start, end, edit)
        })
        .collect();

    edits_with_offsets.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, edit) in edits_with_offsets {
        result.replace_range(start..end, &edit.new_text);
    }
    result
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].title, "Extract to constant 'extracted'");
    assert_eq!(actions[0].kind, CodeActionKind::RefactorExtract);
    assert!(actions[0].is_preferred);

    let edit = actions[0].edit.as_ref().unwrap();
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].title, "Extract to constant 'extracted2'");

    let edit = actions[0].edit.as_ref().unwrap();
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);

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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 0);
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}

#[test]
fn test_quickfix_remove_unused_default_import() {
    let source = "import foo, { bar } from \"mod\";\nbar;\n";
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "import { bar } from \"mod\";\n");
}

#[test]
fn test_quickfix_preserves_type_only_named_import() {
    let source = "import { type Foo, Bar } from \"mod\";\nlet x: Foo;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "import { type Foo } from \"mod\";\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_single_line() {
    let source = "const foo = { a: 1 }; foo.b;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "const foo = { a: 1, b: undefined }; foo.b;\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_single_line_trailing_comma() {
    let source = "const foo = { a: 1, }; foo.b;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "const foo = { a: 1, b: undefined, }; foo.b;\n");
}

#[test]
fn test_quickfix_add_missing_property_object_literal_element_access() {
    let source = "const foo = { a: 1 }; foo[\"b\"];\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "class Foo {\n  method() {\n    this[\"bar\"];\n  }\n  \"bar\": any;\n}\n"
    );
}

#[test]
fn test_quickfix_add_missing_import_named() {
    let source = "foo();\n";
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import { foo } from \"./foo\";\n\nfoo();\n");
}

#[test]
fn test_quickfix_add_missing_import_after_existing_import() {
    let source = "import { bar } from \"./bar\";\nfoo();\n";
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["test.ts"];
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import type { Foo } from \"./foo\";\n\nlet x: Foo;\n"
    );
}

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

    assert_eq!(actions.len(), 0);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

#[test]
fn test_codefix_registry_import_error_2304() {
    // Cannot find name '{0}'. (2304)
    let fixes = CodeFixRegistry::fixes_for_error_code(2304);
    assert!(!fixes.is_empty(), "Should return fixes for error 2304");
    let fix_names: Vec<&str> = fixes.iter().map(|f| f.0).collect();
    assert!(fix_names.contains(&"import"), "Should contain import fix");
}

#[test]
fn test_codefix_registry_unused_identifier_6133() {
    // '{0}' is declared but its value is never read. (6133)
    let fixes = CodeFixRegistry::fixes_for_error_code(6133);
    assert!(!fixes.is_empty(), "Should return fixes for error 6133");
    assert_eq!(fixes[0].0, "unusedIdentifier");
    assert_eq!(fixes[0].1, "unusedIdentifier_delete");
}

#[test]
fn test_codefix_registry_unused_identifier_6196() {
    // '{0}' is declared but never used. (6196)
    let fixes = CodeFixRegistry::fixes_for_error_code(6196);
    assert!(!fixes.is_empty(), "Should return fixes for error 6196");
    assert_eq!(fixes[0].0, "unusedIdentifier");
}

#[test]
fn test_codefix_registry_add_missing_member_2339() {
    // Property '{0}' does not exist on type '{1}'. (2339)
    let fixes = CodeFixRegistry::fixes_for_error_code(2339);
    assert!(!fixes.is_empty(), "Should return fixes for error 2339");
    let fix_names: Vec<&str> = fixes.iter().map(|f| f.0).collect();
    assert!(
        fix_names.contains(&"addMissingMember"),
        "Should contain addMissingMember fix"
    );
}

#[test]
fn test_codefix_registry_await_in_sync_1308() {
    // 'await' expressions are only allowed within async functions (1308)
    let fixes = CodeFixRegistry::fixes_for_error_code(1308);
    assert!(!fixes.is_empty(), "Should return fixes for error 1308");
    assert_eq!(fixes[0].0, "fixAwaitInSyncFunction");
    assert_eq!(fixes[0].1, "fixAwaitInSyncFunction");

    // Also check 1359 variant
    let fixes_1359 = CodeFixRegistry::fixes_for_error_code(1359);
    assert!(!fixes_1359.is_empty(), "Should return fixes for error 1359");
    assert_eq!(fixes_1359[0].0, "fixAwaitInSyncFunction");
}

#[test]
fn test_codefix_registry_override_modifier_4114() {
    // This member cannot have an 'override' modifier (4114)
    let fixes = CodeFixRegistry::fixes_for_error_code(4114);
    assert!(!fixes.is_empty(), "Should return fixes for error 4114");
    assert_eq!(fixes[0].0, "fixOverrideModifier");
}

#[test]
fn test_codefix_registry_class_implements_interface_2420() {
    // Class '{0}' incorrectly implements interface '{1}'. (2420)
    let fixes = CodeFixRegistry::fixes_for_error_code(2420);
    assert!(!fixes.is_empty(), "Should return fixes for error 2420");
    assert_eq!(fixes[0].0, "fixClassIncorrectlyImplementsInterface");
    assert_eq!(fixes[0].1, "fixClassIncorrectlyImplementsInterface");
}

#[test]
fn test_codefix_registry_unreachable_code_7027() {
    // Unreachable code detected (7027)
    let fixes = CodeFixRegistry::fixes_for_error_code(7027);
    assert!(!fixes.is_empty(), "Should return fixes for error 7027");
    assert_eq!(fixes[0].0, "fixUnreachableCode");
}

#[test]
fn test_codefix_registry_unknown_error_returns_empty() {
    // Unknown error code should return empty
    let fixes = CodeFixRegistry::fixes_for_error_code(99999);
    assert!(
        fixes.is_empty(),
        "Should return no fixes for unknown error code"
    );
}

#[test]
fn test_codefix_registry_supported_error_codes_not_empty() {
    let codes = CodeFixRegistry::supported_error_codes();
    assert!(!codes.is_empty(), "Should return supported error codes");
    assert!(
        codes.contains(&2304),
        "Should contain 2304 (Cannot find name)"
    );
    assert!(
        codes.contains(&2339),
        "Should contain 2339 (Property does not exist)"
    );
    assert!(
        codes.contains(&6133),
        "Should contain 6133 (Unused identifier)"
    );
    assert!(codes.contains(&2552), "Should contain 2552 (Did you mean)");
}

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

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, CodeActionKind::RefactorExtract);
    let edit = actions[0].edit.as_ref().unwrap();
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, CodeActionKind::RefactorExtract);
    let edit = actions[0].edit.as_ref().unwrap();
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
            only: None,
            import_candidates: Vec::new(),
        },
    );

    assert_eq!(actions.len(), 1);
    let edit = actions[0].edit.as_ref().unwrap();
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
        actions.is_empty(),
        "Empty file should produce no code actions"
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

    assert_eq!(actions.len(), 1);
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

    assert_eq!(actions.len(), 1);
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

    // A single import has nothing to sort
    assert!(
        actions.is_empty(),
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

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, CodeActionKind::SourceOrganizeImports);
    let edit = actions[0].edit.as_ref().unwrap();
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
