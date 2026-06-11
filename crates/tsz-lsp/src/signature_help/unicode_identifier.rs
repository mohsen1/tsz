use tsz_scanner::{is_ecmascript_identifier_part, is_ecmascript_identifier_start};

pub(super) fn identifier_before_offset(text: &str, offset: usize) -> Option<String> {
    if offset == 0 || offset > text.len() {
        return None;
    }
    let mut end = offset;
    while let Some((idx, ch)) = previous_char_before(text, end) {
        if !ch.is_whitespace() {
            break;
        }
        end = idx;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while let Some((idx, ch)) = previous_char_before(text, start) {
        if !is_ecmascript_identifier_part(ch) {
            break;
        }
        start = idx;
    }
    if start >= end {
        return None;
    }
    let first = text[start..end].chars().next()?;
    is_ecmascript_identifier_start(first).then(|| text[start..end].to_string())
}

fn previous_char_before(text: &str, offset: usize) -> Option<(usize, char)> {
    text.get(..offset)?.char_indices().next_back()
}
