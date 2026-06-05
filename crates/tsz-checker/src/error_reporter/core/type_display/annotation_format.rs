//! Annotation-text normalization helpers for diagnostic type display.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Convert `Array<T>` to `T[]` and `ReadonlyArray<T>` to `readonly T[]`
    /// in annotation text to match tsc's diagnostic display.
    ///
    /// Do not normalize when the generic array appears directly in a type
    /// parameter `extends` clause; tsc preserves `Array<T>` there.
    pub(crate) fn normalize_array_generic_to_shorthand(text: &str) -> String {
        if !text.contains("Array<") {
            return text.to_string();
        }
        let is_extends_constraint_position = |s: &str, start: usize| -> bool {
            let prefix_start = start.saturating_sub(32);
            let prefix = &s[prefix_start..start];
            prefix.trim_end().ends_with("extends")
        };
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;

        while i < text.len() {
            let slice = &text[i..];

            // Process ReadonlyArray<T> first to avoid matching inner Array<T>.
            if slice.starts_with("ReadonlyArray<")
                && (i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric())
                && let Some(inner) = Self::extract_balanced_angle_bracket_content(text, i + 14)
            {
                let end = i + 14 + inner.len() + 1; // "ReadonlyArray<" + inner + ">"
                if is_extends_constraint_position(text, i) {
                    out.push_str(&text[i..end]);
                } else {
                    let needs_parens = inner.contains("=>") || inner.contains(" | ");
                    if needs_parens {
                        out.push_str(&format!("readonly ({inner})[]"));
                    } else {
                        out.push_str(&format!("readonly {inner}[]"));
                    }
                }
                i = end;
                continue;
            }

            if slice.starts_with("Array<")
                && (i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric())
                && let Some(inner) = Self::extract_balanced_angle_bracket_content(text, i + 6)
            {
                let end = i + 6 + inner.len() + 1; // "Array<" + inner + ">"
                if is_extends_constraint_position(text, i) {
                    out.push_str(&text[i..end]);
                } else {
                    let needs_parens = inner.contains("=>") || inner.contains(" | ");
                    if needs_parens {
                        out.push_str(&format!("({inner})[]"));
                    } else {
                        out.push_str(&format!("{inner}[]"));
                    }
                }
                i = end;
                continue;
            }

            if let Some(ch) = slice.chars().next() {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        }

        out
    }

    /// Extract content between balanced angle brackets starting at `pos`.
    /// `pos` should point to the character right after the opening `<`.
    /// Returns the inner content (without brackets) if balanced.
    pub(crate) fn extract_balanced_angle_bracket_content(text: &str, pos: usize) -> Option<String> {
        let bytes = text.as_bytes();
        let mut depth = 1;
        let mut i = pos;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[pos..i].to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Normalize inline object type braces in annotation text to match TSC's
    /// formatting: `{prop: type}` -> `{ prop: type; }`.
    pub(super) fn normalize_inline_object_braces(text: &str) -> String {
        let mut result = String::with_capacity(text.len() + 8);
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;
        while i < len {
            if chars[i] == '{' {
                // Find the matching closing brace.
                let mut depth = 1;
                let mut j = i + 1;
                while j < len && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth > 0 {
                    result.extend(chars[i..].iter());
                    break;
                }
                // j now points past the closing `}`.
                let inner_start = i + 1;
                let inner_end = j - 1;
                let inner: String = chars[inner_start..inner_end].iter().collect();
                let trimmed = inner.trim();

                if trimmed.is_empty() {
                    result.push_str("{}");
                } else {
                    let normalized_inner =
                        super::super::annotation_text::normalize_inline_object_member_separators(
                            trimmed,
                        );
                    let needs_semicolon = !normalized_inner.ends_with(';')
                        && !normalized_inner.ends_with("};")
                        && normalized_inner.contains(':');
                    result.push_str("{ ");
                    result.push_str(&normalized_inner);
                    if needs_semicolon {
                        result.push(';');
                    }
                    result.push_str(" }");
                }
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }
}
