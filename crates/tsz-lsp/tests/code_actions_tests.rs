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
include!("code_actions_tests_parts/part_00.rs");
include!("code_actions_tests_parts/part_01.rs");
include!("code_actions_tests_parts/part_02.rs");
include!("code_actions_tests_parts/part_03.rs");
include!("code_actions_tests_parts/part_04.rs");
include!("code_actions_tests_parts/part_05.rs");
include!("code_actions_tests_parts/part_06.rs");
include!("code_actions_tests_parts/part_07.rs");
include!("code_actions_tests_parts/part_08.rs");
include!("code_actions_tests_parts/part_09.rs");
