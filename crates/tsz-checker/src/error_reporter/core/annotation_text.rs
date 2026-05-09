//! Annotation text normalization helpers for diagnostic display.

pub(super) fn normalize_inline_object_member_separators(text: &str) -> String {
    if !text.contains(',') {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut angle_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if angle_depth == 0
                && brace_depth == 0
                && bracket_depth == 0
                && paren_depth == 0 =>
            {
                out.push(';');
                while matches!(chars.peek(), Some(next) if next.is_whitespace()) {
                    chars.next();
                }
                out.push(' ');
                continue;
            }
            _ => {}
        }
        out.push(ch);
    }

    out
}
