//! Annotation text normalization for diagnostic displays.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(crate) fn normalize_annotation_literal_property_display_text(text: &str) -> String {
        let quoted = Self::normalize_single_quoted_string_literal_types(text);
        Self::add_undefined_to_optional_object_property_display(&quoted)
    }

    fn normalize_single_quoted_string_literal_types(text: &str) -> String {
        if !text.contains('\'') {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] != '\'' {
                out.push(chars[i]);
                i += 1;
                continue;
            }

            let start = i;
            i += 1;
            let mut literal = String::new();
            let mut escaped = false;
            while i < chars.len() {
                let ch = chars[i];
                i += 1;
                if escaped {
                    literal.push(ch);
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    literal.push(ch);
                    escaped = true;
                    continue;
                }
                if ch == '\'' {
                    out.push('"');
                    out.push_str(&literal);
                    out.push('"');
                    break;
                }
                literal.push(ch);
            }
            if i == chars.len() && chars.last().copied() != Some('\'') {
                out.extend(chars[start..].iter());
                break;
            }
        }
        out
    }

    fn add_undefined_to_optional_object_property_display(text: &str) -> String {
        if !text.contains("?:") {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;
        let mut brace_depth = 0usize;
        let mut paren_depth = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            match ch {
                '{' => {
                    brace_depth += 1;
                    out.push(ch);
                    i += 1;
                }
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    out.push(ch);
                    i += 1;
                }
                '(' => {
                    paren_depth += 1;
                    out.push(ch);
                    i += 1;
                }
                ')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    out.push(ch);
                    i += 1;
                }
                '?' if brace_depth > 0 && paren_depth == 0 && chars.get(i + 1) == Some(&':') => {
                    out.push('?');
                    out.push(':');
                    i += 2;

                    let value_start = i;
                    let mut nested_angle = 0usize;
                    let mut nested_bracket = 0usize;
                    let mut nested_paren = 0usize;
                    while i < chars.len() {
                        match chars[i] {
                            '<' => nested_angle += 1,
                            '>' => nested_angle = nested_angle.saturating_sub(1),
                            '[' => nested_bracket += 1,
                            ']' => nested_bracket = nested_bracket.saturating_sub(1),
                            '(' => nested_paren += 1,
                            ')' => nested_paren = nested_paren.saturating_sub(1),
                            ';' | ',' | '}'
                                if nested_angle == 0
                                    && nested_bracket == 0
                                    && nested_paren == 0 =>
                            {
                                break;
                            }
                            _ => {}
                        }
                        i += 1;
                    }

                    let value: String = chars[value_start..i].iter().collect();
                    let trimmed_end = value.trim_end();
                    let trailing_ws = &value[trimmed_end.len()..];
                    out.push_str(trimmed_end);
                    if !trimmed_end.contains("undefined") {
                        out.push_str(" | undefined");
                    }
                    out.push_str(trailing_ws);
                }
                _ => {
                    out.push(ch);
                    i += 1;
                }
            }
        }

        out
    }
}
