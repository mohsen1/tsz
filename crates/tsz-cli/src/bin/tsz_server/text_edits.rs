//! Text edit utilities shared by tsserver handlers.

use tsz::lsp::formatting::TextEdit;
use tsz::lsp::position::{LineMap, Range};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NarrowedTextEdit {
    pub(crate) range: Range,
    pub(crate) new_text: String,
}

impl NarrowedTextEdit {
    fn unchanged(edit: &TextEdit) -> Self {
        Self {
            range: edit.range,
            new_text: edit.new_text.clone(),
        }
    }
}

pub(crate) fn narrow_indentation_only_edit(
    source_text: &str,
    line_map: &LineMap,
    edit: &TextEdit,
) -> NarrowedTextEdit {
    let Some(start_off) = line_map.position_to_offset(edit.range.start, source_text) else {
        return NarrowedTextEdit::unchanged(edit);
    };
    let Some(end_off) = line_map.position_to_offset(edit.range.end, source_text) else {
        return NarrowedTextEdit::unchanged(edit);
    };
    if start_off >= end_off {
        return NarrowedTextEdit::unchanged(edit);
    }

    let Some(old_text) = source_text.get(start_off as usize..end_off as usize) else {
        return NarrowedTextEdit::unchanged(edit);
    };
    if old_text.contains('\n') || old_text.contains('\r') {
        return NarrowedTextEdit::unchanged(edit);
    }
    if edit.new_text.contains('\n') || edit.new_text.contains('\r') {
        return NarrowedTextEdit::unchanged(edit);
    }

    let mut prefix = 0usize;
    for ((old_idx, old_ch), (_, new_ch)) in
        old_text.char_indices().zip(edit.new_text.char_indices())
    {
        if old_ch != new_ch {
            break;
        }
        prefix = old_idx + old_ch.len_utf8();
    }

    let old_after_prefix = &old_text[prefix..];
    let new_after_prefix = &edit.new_text[prefix..];

    let mut old_suffix_bytes = 0usize;
    let mut new_suffix_bytes = 0usize;
    let mut old_rev = old_after_prefix.char_indices().rev();
    let mut new_rev = new_after_prefix.char_indices().rev();
    while let (Some((old_idx, old_ch)), Some((new_idx, new_ch))) = (old_rev.next(), new_rev.next())
    {
        if old_ch != new_ch {
            break;
        }
        old_suffix_bytes = old_after_prefix.len() - old_idx;
        new_suffix_bytes = new_after_prefix.len() - new_idx;
    }

    let old_mid_end = old_text.len().saturating_sub(old_suffix_bytes);
    let new_mid_end = edit.new_text.len().saturating_sub(new_suffix_bytes);
    let narrowed_start = start_off + prefix as u32;
    let narrowed_end = start_off + old_mid_end as u32;
    let start_pos = line_map.offset_to_position(narrowed_start, source_text);
    let end_pos = line_map.offset_to_position(narrowed_end, source_text);
    let new_text = edit.new_text[prefix..new_mid_end].to_string();

    if narrowed_start == start_off && narrowed_end == end_off && new_text == edit.new_text {
        return NarrowedTextEdit::unchanged(edit);
    }

    NarrowedTextEdit {
        range: Range::new(start_pos, end_pos),
        new_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz::lsp::position::Position;

    fn edit_for_offsets(source_text: &str, start: u32, end: u32, new_text: &str) -> TextEdit {
        let line_map = LineMap::build(source_text);
        TextEdit::new(
            Range::new(
                line_map.offset_to_position(start, source_text),
                line_map.offset_to_position(end, source_text),
            ),
            new_text.to_string(),
        )
    }

    #[test]
    fn indentation_only_delete_trims_common_prefix() {
        let source_text = "    let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 4, "  ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(
            narrowed.range,
            Range::new(Position::new(0, 2), Position::new(0, 4))
        );
        assert_eq!(narrowed.new_text, "");
    }

    #[test]
    fn mixed_whitespace_insert_trims_common_prefix() {
        let source_text = "\t  let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 3, "\t    ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(
            narrowed.range,
            Range::new(Position::new(0, 3), Position::new(0, 3))
        );
        assert_eq!(narrowed.new_text, "  ");
    }

    #[test]
    fn multiline_old_text_keeps_original_edit() {
        let source_text = "let a = 1;\nlet b = 2;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 20, "let a = 1;\n  let b = 2;");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(narrowed.range, edit.range);
        assert_eq!(narrowed.new_text, edit.new_text);
    }

    #[test]
    fn zero_width_edit_keeps_original_edit() {
        let source_text = "let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 4, 4, "  ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(narrowed.range, edit.range);
        assert_eq!(narrowed.new_text, edit.new_text);
    }
}
