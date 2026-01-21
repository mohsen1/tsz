use super::*;
use crate::binder::BinderState;
use crate::checker::types::diagnostics::diagnostic_codes::{
    CANNOT_FIND_NAME, PROPERTY_DOES_NOT_EXIST_ON_TYPE, UNUSED_IMPORT, UNUSED_VARIABLE,
};
use crate::lsp::position::LineMap;
use crate::parser::ParserState;

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

    edits_with_offsets.sort_by(|a, b| b.0.cmp(&a.0));
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
    let edits = edit.changes.get("test.tsx").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.tsx").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_IMPORT),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_IMPORT),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_IMPORT),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_IMPORT),
        source: None,
        message: "unused import".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import { foo } from \"./foo\";\nfoo();\n");
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import { bar, foo } from \"./foo\";\nfoo();\n");
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import Foo from \"./foo\";\nFoo();\n");
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(updated, "import * as ns from \"./foo\";\nns.foo;\n");
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import type { Foo } from \"./foo\";\nlet x: Foo;\n"
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import { Foo } from \"./foo\";\ntype T = typeof Foo;\n"
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import { Foo } from \"./foo\";\nclass Bar extends Foo {}\n"
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
    let edits = edit.changes.get("test.ts").unwrap();
    let updated = apply_text_edits(source, &line_map, edits);
    assert_eq!(
        updated,
        "import type { Foo } from \"./foo\";\nclass Bar implements Foo {}\n"
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
        code: Some(UNUSED_VARIABLE),
        source: None,
        message: "'x' is declared but its value is never read.".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_VARIABLE),
        source: None,
        message: "'unused' is declared but its value is never read.".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_VARIABLE),
        source: None,
        message: "'unused' is declared but its value is never read.".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
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
        code: Some(UNUSED_VARIABLE),
        source: None,
        message: "'Unused' is declared but its value is never read.".to_string(),
        related_information: None,
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
    let edits = edit.changes.get("test.ts").unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "");
}
