//! Unterminated-regex closing-token recovery for
//! `state_expressions_literals_regex`.
//!
//! Relocated from the parent module file to keep it under the per-file
//! line ceiling; pure move, no logic change.

use crate::parser::state::ParserState;

impl ParserState {
    pub(super) fn missing_regex_closing_token(&self, text: &str) -> Option<u8> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'/' {
            return None;
        }

        // Mirror the regex scan state for body extraction.
        let mut in_escape = false;
        let mut in_character_class = false;
        let mut body_end = bytes.len();

        for (i, ch) in bytes.iter().enumerate().skip(1) {
            let ch = *ch;
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
            } else if ch == b'[' && !in_character_class {
                in_character_class = true;
            } else if ch == b']' && in_character_class {
                in_character_class = false;
            } else if ch == b'/' && !in_character_class {
                body_end = i;
                break;
            }
        }

        if body_end <= 1 {
            return None;
        }

        // Under `v`, a nested `[` in a class opens ANOTHER class needing its own `]`.
        let unicode_sets_mode = bytes[body_end + 1..].contains(&b'v');
        let (mut paren_depth, mut class_depth) = (0i32, 0i32);
        let mut i = 1usize;
        while i < body_end {
            let ch = bytes[i];
            if ch == b'\\' {
                // `\q{...}` denotes a `ClassStringDisjunction`, whose
                // interior has no class-nesting grammar of its own — an
                // unescaped `[` in there is reserved content (TS1508, see
                // `scan_class_string_disjunction_body`), not a nested class
                // open, and must not perturb this depth count. Mirrors the
                // semantic walker's `b'q'` arm in
                // `scan_character_class_escape`, which skips this same span.
                if unicode_sets_mode
                    && bytes.get(i + 1) == Some(&b'q')
                    && bytes.get(i + 2) == Some(&b'{')
                {
                    i += 3;
                    while i < body_end && bytes[i] != b'}' {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
                i += 2;
                continue;
            }
            if class_depth > 0 {
                if ch == b']' {
                    class_depth -= 1;
                } else if unicode_sets_mode && ch == b'[' {
                    class_depth += 1;
                }
                i += 1;
                continue;
            }
            match ch {
                b'[' => class_depth = 1,
                b'(' => paren_depth += 1,
                b')' if paren_depth > 0 => paren_depth -= 1,
                _ => {}
            }
            i += 1;
        }
        if class_depth > 0 {
            Some(b']')
        } else if paren_depth > 0 {
            Some(b')')
        } else {
            None
        }
    }
}
