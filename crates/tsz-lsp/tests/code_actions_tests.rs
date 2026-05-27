use super::*;
use tsz_binder::BinderState;
use tsz_checker::diagnostics::diagnostic_codes::{
    ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED, ALL_VARIABLES_ARE_UNUSED, CANNOT_FIND_NAME,
    PROPERTY_DOES_NOT_EXIST_ON_TYPE,
};
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

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

fn add_missing_await_actions(source: &str, diagnostic_needle: &str) -> Vec<CodeAction> {
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let range = range_for_substring(source, &line_map, diagnostic_needle);
    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        source: None,
        message: "Property 'toString' does not exist on type 'Promise<number>'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    provider.provide_code_actions(
        root,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
        CodeActionContext {
            diagnostics: vec![diag],
            only: Some(vec![CodeActionKind::QuickFix]),
            import_candidates: Vec::new(),
        },
    )
}

fn has_add_missing_await_action(actions: &[CodeAction]) -> bool {
    actions
        .iter()
        .any(|action| action.title == "Add missing 'await'")
}

fn move_to_file_action(source: &str, file_name: &str, needle: &str) -> CodeAction {
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CodeActionProvider::new(arena, &binder, &line_map, file_name.to_string(), source);

    let range = range_for_substring(source, &line_map, needle);
    let actions = provider.provide_code_actions(
        root,
        range,
        CodeActionContext {
            diagnostics: Vec::new(),
            only: Some(vec![CodeActionKind::Refactor]),
            import_candidates: Vec::new(),
        },
    );

    actions
        .into_iter()
        .find(|action| action.title.starts_with("Move "))
        .expect("expected move-to-file action")
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of code_actions_tests tests.
include!("code_actions_tests_parts/part_00.rs");
include!("code_actions_tests_parts/part_01.rs");
include!("code_actions_tests_parts/part_02.rs");
