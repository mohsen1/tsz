//! Project-level LSP tests.

use super::*;
use crate::project::FileRename;
use tsz_common::position::LineMap;

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

fn range_for_substring(source: &str, line_map: &LineMap, needle: &str) -> Range {
    let start = source.find(needle).expect("substring not found") as u32;
    let end = start + needle.len() as u32;
    let start_pos = line_map.offset_to_position(start, source);
    let end_pos = line_map.offset_to_position(end, source);
    Range::new(start_pos, end_pos)
}
include!("project_tests_parts/part_00.rs");
include!("project_tests_parts/part_01.rs");
include!("project_tests_parts/part_02.rs");
include!("project_tests_parts/part_03.rs");
include!("project_tests_parts/part_04.rs");
include!("project_tests_parts/part_05.rs");
include!("project_tests_parts/part_06.rs");
include!("project_tests_parts/part_07.rs");
include!("project_tests_parts/part_08.rs");
include!("project_tests_parts/part_09.rs");
include!("project_tests_parts/part_10.rs");
include!("project_tests_parts/part_11.rs");
include!("project_tests_parts/part_12.rs");
include!("project_tests_parts/part_13.rs");
include!("project_tests_parts/part_14.rs");
include!("project_tests_parts/part_15.rs");
include!("project_tests_parts/part_16.rs");
include!("project_tests_parts/part_17.rs");
include!("project_tests_parts/part_18.rs");
include!("project_tests_parts/part_19.rs");
include!("project_tests_parts/part_20.rs");
