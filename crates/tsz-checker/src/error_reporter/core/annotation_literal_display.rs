//! Annotation text normalization for diagnostic displays.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(crate) fn normalize_annotation_literal_property_display_text(text: &str) -> String {
        let quoted = Self::normalize_single_quoted_string_literal_types(text);
        Self::add_undefined_to_optional_object_property_display(&quoted)
    }

    pub(crate) fn normalize_single_quoted_string_literal_types(text: &str) -> String {
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

    pub(crate) fn add_undefined_to_optional_object_property_display(text: &str) -> String {
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

    pub(crate) fn normalize_inline_object_type_literal_spacing(text: &str) -> String {
        if !text.contains('{') || text.contains("`${") {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut object_stack: Vec<bool> = Vec::new();
        let mut i = 0usize;
        while i < chars.len() {
            match chars[i] {
                '{' => {
                    object_stack.push(false);
                    out.push('{');
                    if chars
                        .get(i + 1)
                        .is_some_and(|next| !next.is_whitespace() && *next != '}')
                    {
                        out.push(' ');
                    }
                }
                ':' if !object_stack.is_empty() => {
                    if let Some(has_colon) = object_stack.last_mut() {
                        *has_colon = true;
                    }
                    out.push(':');
                }
                '}' => {
                    let object_has_property = object_stack.pop().unwrap_or(false);
                    if object_has_property {
                        while out.ends_with(char::is_whitespace) {
                            out.pop();
                        }
                        if !out.ends_with(';') && !out.ends_with('{') {
                            out.push(';');
                        }
                        if !out.ends_with(' ') {
                            out.push(' ');
                        }
                    }
                    out.push('}');
                }
                ch => out.push(ch),
            }
            i += 1;
        }

        out
    }

    pub(in crate::error_reporter) fn format_declared_annotation_for_diagnostic(
        &self,
        annotation_text: &str,
    ) -> String {
        let mut formatted = annotation_text.trim().to_string();
        formatted = Self::normalize_single_quoted_string_literal_types(&formatted);
        if !self.ctx.compiler_options.exact_optional_property_types {
            formatted = Self::add_undefined_to_optional_object_property_display(&formatted);
        }
        if self.ctx.compiler_options.exact_optional_property_types && formatted.contains("?:") {
            formatted = Self::normalize_inline_object_type_literal_spacing(&formatted);
        }
        if formatted.contains(':') {
            formatted = formatted.replace(" }", "; }");
            while formatted.contains(";; }") {
                formatted = formatted.replace(";; }", "; }");
            }
        }
        formatted
    }

    pub(in crate::error_reporter) fn annotation_text_is_plain_type_reference(
        annotation_text: &str,
    ) -> bool {
        let text = annotation_text.trim();
        !text.is_empty()
            && text.split('.').all(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
                    && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            })
    }
}
