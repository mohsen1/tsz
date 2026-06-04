impl ParserState {
    fn missing_regex_closing_token(&self, text: &str) -> Option<u8> {
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

        let mut missing = None;
        let mut paren_depth = 0i32;
        in_escape = false;
        in_character_class = false;
        for &ch in &bytes[1..body_end] {
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
                continue;
            }
            if in_character_class {
                if ch == b']' {
                    in_character_class = false;
                }
                continue;
            }
            match ch {
                b'[' => in_character_class = true,
                b'(' => paren_depth += 1,
                b')' if paren_depth > 0 => paren_depth -= 1,
                _ => {}
            }
        }

        if in_character_class {
            missing = Some(b']');
        }
        if missing.is_none() && paren_depth > 0 {
            missing = Some(b')');
        }

        missing
    }
}
