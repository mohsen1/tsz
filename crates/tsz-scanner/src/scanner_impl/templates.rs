use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl ScannerState {
    /// Scan a template literal (simplified).
    pub(crate) fn scan_template_literal(&mut self) {
        self.pos += 1; // Skip backtick
        let mut result = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKTICK {
                self.pos += 1;
                self.token_value = result;
                self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
                return;
            }
            if ch == CharacterCodes::DOLLAR
                && self.char_code_at(self.pos + 1) == Some(CharacterCodes::OPEN_BRACE)
            {
                self.pos += 2;
                self.token_value = result;
                self.token = SyntaxKind::TemplateHead;
                return;
            }
            if ch == CharacterCodes::BACKSLASH {
                // Scan escaped character after the backslash.
                self.pos += 1;
                let escaped = self.scan_template_escape_sequence();
                result.push_str(&escaped);
            } else {
                if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                    self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
                }
                if let Some(c) = char::from_u32(ch) {
                    result.push(c);
                }
                self.pos += self.char_len_at(self.pos); // Advance by character byte length
            }
        }

        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = result;
        self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
    }

    /// Re-scan the current `}` token as the continuation of a template literal.
    /// Called by the parser when it determines that a `}` is closing a template expression.
    ///
    /// # Arguments
    /// * `is_tagged_template` - If true, invalid escape sequences should not report errors
    ///   (tagged templates can have invalid escapes that get passed to the tag function as raw).
    ///   For now, we don't report errors anyway, so this parameter affects nothing.
    #[wasm_bindgen(js_name = reScanTemplateToken)]
    pub fn re_scan_template_token(&mut self, _is_tagged_template: bool) -> SyntaxKind {
        // Reset position to token start and scan the template continuation
        // Make sure token_start is within bounds
        if self.token_start >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }
        self.pos = self.token_start;
        self.token = self.scan_template_and_set_token_value(false);
        self.token
    }

    /// Re-scan template head or no-substitution template.
    /// Used when the parser needs to rescan the start of a template.
    #[wasm_bindgen(js_name = reScanTemplateHeadOrNoSubstitutionTemplate)]
    pub fn re_scan_template_head_or_no_substitution_template(&mut self) -> SyntaxKind {
        self.pos = self.token_start;
        self.token = self.scan_template_and_set_token_value(true);
        self.token
    }

    /// Internal helper to scan a template literal part and set the token value.
    ///
    /// # Arguments
    /// * `started_with_backtick` - true if this is the start of a template (head or no-substitution),
    ///   false if this is a continuation after a `}` (middle or tail).
    fn scan_template_and_set_token_value(&mut self, started_with_backtick: bool) -> SyntaxKind {
        // Move past the opening character (backtick for head, } for middle/tail)
        // Safety check: ensure we don't move past the end
        if self.pos >= self.end {
            self.token_flags |= TokenFlags::Unterminated as u32;
            self.token_value = String::new();
            return if started_with_backtick {
                SyntaxKind::NoSubstitutionTemplateLiteral
            } else {
                SyntaxKind::TemplateTail
            };
        }
        self.pos += 1;
        let mut start = self.pos;
        let mut contents = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);

            // End of template: backtick
            if ch == CharacterCodes::BACKTICK {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 1;
                self.token_value = contents;
                return if started_with_backtick {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            }

            // Template expression: ${
            if ch == CharacterCodes::DOLLAR
                && self.pos + 1 < self.end
                && self.char_code_unchecked(self.pos + 1) == CharacterCodes::OPEN_BRACE
            {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 2;
                self.token_value = contents;
                return if started_with_backtick {
                    SyntaxKind::TemplateHead
                } else {
                    SyntaxKind::TemplateMiddle
                };
            }

            // Escape sequence
            if ch == CharacterCodes::BACKSLASH {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 1; // Advance past the backslash
                let escaped = self.scan_template_escape_sequence();
                contents.push_str(&escaped);
                // Reset start to current position after the escape
                start = self.pos;
                continue;
            }

            // CR normalization (CR or CRLF -> LF)
            if ch == CharacterCodes::CARRIAGE_RETURN {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 1;
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                {
                    self.pos += 1;
                }
                contents.push('\n');
                // Reset start to current position after the CR
                start = self.pos;
                continue;
            }

            // Advance by full UTF-8 codepoint width so multi-byte chars (e.g. µ) don't
            // move the scanner into a non-char-boundary byte index.
            self.pos += self.char_len_at(self.pos);
        }

        // Unterminated template
        contents.push_str(&self.substring(start, self.pos));
        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = contents;
        if started_with_backtick {
            SyntaxKind::NoSubstitutionTemplateLiteral
        } else {
            SyntaxKind::TemplateTail
        }
    }

    /// Scan an escape sequence in a template literal.
    /// Returns the resulting string and advances self.pos.
    ///
    /// `self.pos` is expected to point at the escaped character (just after `\`).
    fn scan_template_escape_sequence(&mut self) -> String {
        if self.pos >= self.end {
            return String::from("\\");
        }

        let ch = self.char_code_unchecked(self.pos);
        // Use char_len_at for proper UTF-8 handling of multi-byte chars
        let ch_len = self.char_len_at(self.pos);
        self.pos += ch_len;

        match ch {
            CharacterCodes::_0 => self.scan_template_escape_digit_zero(),
            CharacterCodes::_1
            | CharacterCodes::_2
            | CharacterCodes::_3
            | CharacterCodes::_4
            | CharacterCodes::_5
            | CharacterCodes::_6
            | CharacterCodes::_7
            | CharacterCodes::_8
            | CharacterCodes::_9 => self.scan_template_escape_octal_digit(ch),
            CharacterCodes::LOWER_N => String::from("\n"),
            CharacterCodes::LOWER_R => String::from("\r"),
            CharacterCodes::LOWER_T => String::from("\t"),
            CharacterCodes::LOWER_V => String::from("\x0B"),
            CharacterCodes::LOWER_B => String::from("\x08"),
            CharacterCodes::LOWER_F => String::from("\x0C"),
            CharacterCodes::SINGLE_QUOTE => String::from("'"),
            CharacterCodes::DOUBLE_QUOTE => String::from("\""),
            CharacterCodes::BACKTICK => String::from("`"),
            CharacterCodes::BACKSLASH => String::from("\\"),
            CharacterCodes::DOLLAR => String::from("$"),
            CharacterCodes::LINE_FEED
            | CharacterCodes::LINE_SEPARATOR
            | CharacterCodes::PARAGRAPH_SEPARATOR => String::new(),
            CharacterCodes::CARRIAGE_RETURN => self.scan_template_escape_cr(),
            CharacterCodes::LOWER_X => self.scan_template_hex_escape(),
            CharacterCodes::LOWER_U => self.scan_template_unicode_escape(),
            _ => Self::scan_template_unknown_escape(ch),
        }
    }

    fn scan_template_escape_digit_zero(&mut self) -> String {
        if self.pos < self.end && is_digit(self.char_code_unchecked(self.pos)) {
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            return String::from("\\0");
        }
        String::from("\0")
    }

    fn scan_template_escape_octal_digit(&mut self, ch: u32) -> String {
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        let digit = char::from_u32(ch).unwrap_or('?');
        format!("\\{digit}")
    }

    fn scan_template_escape_cr(&mut self) -> String {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED {
            self.pos += 1;
        }
        String::new()
    }

    fn scan_template_hex_escape(&mut self) -> String {
        if self.pos + 2 <= self.end {
            let hex = self.substring(self.pos, self.pos + 2);
            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                self.pos += 2;
                if let Some(c) = char::from_u32(code) {
                    return c.to_string();
                }
            }
        }
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        "\\x".to_string()
    }

    fn scan_template_unicode_escape(&mut self) -> String {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::OPEN_BRACE {
            return self.scan_template_brace_unicode_escape();
        }

        if self.pos + 4 <= self.end {
            let hex = self.substring(self.pos, self.pos + 4);
            if hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                self.pos += 4;
                return Self::decode_fixed_unicode_code_unit(code);
            }
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            return String::from("\\u");
        }

        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        String::from("\\u")
    }

    /// Decode the cooked value of a fixed four-hex-digit `\uXXXX` escape.
    ///
    /// A fixed four-hex-digit escape always denotes a single UTF-16 code unit in
    /// the BMP range `0x0000..=0xFFFF`, so it is always a *valid* escape — even
    /// when the value is a lone surrogate code unit (`0xD800..=0xDFFF`). `tsc`
    /// treats such escapes as valid with a real cooked value, so they must not
    /// set `ContainsInvalidEscape` (which would force tagged-template lowering).
    ///
    /// Lone surrogate code units have no standalone Unicode scalar value, so
    /// they cannot be stored verbatim in a UTF-8 cooked string; they are cooked
    /// to the replacement character. The cooked text only feeds the downlevel
    /// cooked array, which this classification now correctly avoids emitting.
    fn decode_fixed_unicode_code_unit(code: u32) -> String {
        char::from_u32(code)
            .unwrap_or(char::REPLACEMENT_CHARACTER)
            .to_string()
    }

    fn scan_template_brace_unicode_escape(&mut self) -> String {
        self.pos += 1;
        let hex_start = self.pos;
        while self.pos < self.end && is_hex_digit(self.char_code_unchecked(self.pos)) {
            self.pos += 1;
        }
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::CLOSE_BRACE
        {
            let hex = self.substring(hex_start, self.pos);
            self.pos += 1;
            if let Ok(code) = u32::from_str_radix(&hex, 16)
                && let Some(c) = char::from_u32(code)
            {
                return c.to_string();
            }
        }
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        String::from("\\u")
    }

    fn scan_template_unknown_escape(ch: u32) -> String {
        if let Some(c) = char::from_u32(ch) {
            c.to_string()
        } else {
            String::new()
        }
    }
}
