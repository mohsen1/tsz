use super::*;

impl ScannerState {
    /// Scan a string literal.
    pub(crate) fn scan_string(&mut self, quote: u32) {
        self.pos += 1; // Skip opening quote
        let mut result = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == quote {
                self.pos += 1; // Closing quote
                self.token_value = result;
                self.token = SyntaxKind::StringLiteral;
                return;
            }
            if ch == CharacterCodes::BACKSLASH {
                self.pos += 1;
                if self.pos < self.end {
                    self.scan_string_escape(quote, &mut result);
                } else {
                    // Backslash at EOF — incomplete escape sequence.
                    // Mark with UnterminatedAtEof so the parser can emit TS1126
                    // instead of TS1002.
                    self.token_flags |=
                        TokenFlags::Unterminated as u32 | TokenFlags::UnterminatedAtEof as u32;
                    self.token_value = result;
                    self.token = SyntaxKind::StringLiteral;
                    return;
                }
            } else if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                // Unterminated string
                self.token_flags |= TokenFlags::Unterminated as u32;
                self.token_value = result;
                self.token = SyntaxKind::StringLiteral;
                return;
            } else {
                if let Some(c) = char::from_u32(ch) {
                    result.push(c);
                }
                self.pos += self.char_len_at(self.pos); // Advance by character byte length
            }
        }

        // Unterminated string — reached EOF without closing quote.
        // If the string contains an invalid/incomplete escape sequence (e.g., \u{...
        // without closing brace, or bare \u at EOF), tsc emits TS1126 "Unexpected end
        // of text". Otherwise, tsc emits TS1002 "Unterminated string literal".
        self.token_flags |= TokenFlags::Unterminated as u32;
        if (self.token_flags & TokenFlags::ContainsInvalidEscape as u32) != 0 {
            self.token_flags |= TokenFlags::UnterminatedAtEof as u32;
        }
        self.token_value = result;
        self.token = SyntaxKind::StringLiteral;
    }

    fn scan_string_escape(&mut self, quote: u32, result: &mut String) {
        // pos currently points to the char after the backslash; backslash is at pos - 1.
        let backslash_pos = self.pos - 1;
        let escaped = self.char_code_unchecked(self.pos);
        // Get byte length of the escaped character before advancing.
        let escaped_len = self.char_len_at(self.pos);
        self.pos += escaped_len;

        match escaped {
            CharacterCodes::_0 => self.scan_string_escape_zero(backslash_pos, result),
            CharacterCodes::_1
            | CharacterCodes::_2
            | CharacterCodes::_3
            | CharacterCodes::_4
            | CharacterCodes::_5
            | CharacterCodes::_6
            | CharacterCodes::_7 => self.scan_string_escape_octal(backslash_pos, escaped, result),
            CharacterCodes::_8 | CharacterCodes::_9 => {
                // \8 or \9 is not a valid escape sequence — emit TS1488.
                let digit = char::from_u32(escaped).unwrap_or('?');
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: backslash_pos,
                    length: self.pos - backslash_pos,
                    message: diagnostic_messages::ESCAPE_SEQUENCE_IS_NOT_ALLOWED,
                    code: diagnostic_codes::ESCAPE_SEQUENCE_IS_NOT_ALLOWED,
                    args: vec![format!("\\{digit}")],
                });
                if let Some(c) = char::from_u32(escaped) {
                    result.push(c);
                }
            }
            CharacterCodes::LOWER_N => result.push('\n'),
            CharacterCodes::LOWER_R => result.push('\r'),
            CharacterCodes::LOWER_T => result.push('\t'),
            CharacterCodes::LOWER_V => result.push('\x0B'),
            CharacterCodes::LOWER_B => result.push('\x08'),
            CharacterCodes::LOWER_F => result.push('\x0C'),
            CharacterCodes::BACKSLASH => result.push('\\'),
            c if c == quote => result.push(char::from_u32(quote).unwrap_or('\0')),
            CharacterCodes::LOWER_X => self.scan_string_escape_hex(result),
            CharacterCodes::LOWER_U => self.scan_string_escape_unicode(result),
            CharacterCodes::LINE_FEED
            | CharacterCodes::CARRIAGE_RETURN
            | CharacterCodes::LINE_SEPARATOR
            | CharacterCodes::PARAGRAPH_SEPARATOR => {
                // Line continuation - also handle CR+LF as a single line break
                if escaped == CharacterCodes::CARRIAGE_RETURN
                    && self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                {
                    self.pos += 1;
                }
            }
            _ => {
                if let Some(c) = char::from_u32(escaped) {
                    result.push(c);
                }
            }
        }
    }

    fn scan_string_escape_zero(&mut self, backslash_pos: usize, result: &mut String) {
        if self.pos < self.end && is_digit(self.char_code_unchecked(self.pos)) {
            // Legacy octal escape: \0N - scan as octal
            let mut value = 0u32;
            let octal_start = self.pos - 1; // include the '0'
            self.pos = octal_start;
            while self.pos < self.end
                && self.pos < octal_start + 3
                && is_octal_digit(self.char_code_unchecked(self.pos))
            {
                value = value * 8 + (self.char_code_unchecked(self.pos) - CharacterCodes::_0);
                self.pos += 1;
            }
            self.emit_octal_escape_diagnostic(backslash_pos, value);
            if let Some(c) = char::from_u32(value) {
                result.push(c);
            }
        } else {
            result.push('\0');
        }
    }

    fn scan_string_escape_octal(
        &mut self,
        backslash_pos: usize,
        escaped: u32,
        result: &mut String,
    ) {
        // Legacy octal escape: \1 through \7.
        // tsc reads up to 3 digits when the leading digit is 0-3, but only up to
        // 2 digits when the leading digit is 4-7 (so `\477` parses as `\47` + '7').
        let mut value = escaped - CharacterCodes::_0;
        let max_digits = if escaped <= CharacterCodes::_3 { 3 } else { 2 };
        let mut count = 1;
        while count < max_digits
            && self.pos < self.end
            && is_octal_digit(self.char_code_unchecked(self.pos))
        {
            value = value * 8 + (self.char_code_unchecked(self.pos) - CharacterCodes::_0);
            self.pos += 1;
            count += 1;
        }
        self.emit_octal_escape_diagnostic(backslash_pos, value);
        if let Some(c) = char::from_u32(value) {
            result.push(c);
        }
    }

    fn emit_octal_escape_diagnostic(&mut self, backslash_pos: usize, value: u32) {
        // tsc renders the suggestion as `\xNN` with lowercase 2-digit hex.
        let suggestion = format!("\\x{value:02x}");
        self.scanner_diagnostics.push(ScannerDiagnostic {
            pos: backslash_pos,
            length: self.pos - backslash_pos,
            message: diagnostic_messages::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
            code: diagnostic_codes::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
            args: vec![suggestion],
        });
    }

    fn scan_string_escape_hex(&mut self, result: &mut String) {
        // Mirror tsc's `scanHexDigits(/*minCount*/ 2)` loop: consume hex digits
        // one at a time, stop at the first non-hex char, then if fewer than 2
        // were consumed, emit "Hexadecimal digit expected." at the current
        // position. tsc's error anchor is wherever the scan halted — the first
        // non-hex char or the closing quote — not the start of the escape.
        let mut digit_count = 0;
        while digit_count < 2
            && self.pos < self.end
            && is_hex_digit(self.char_code_unchecked(self.pos))
        {
            self.pos += 1;
            digit_count += 1;
        }
        if digit_count < 2 {
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            self.scanner_diagnostics.push(ScannerDiagnostic {
                pos: self.pos,
                length: 0,
                args: Vec::new(),
                message: diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                code: diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            });
            result.push('\\');
            result.push('x');
            return;
        }
        let hex_start = self.pos - 2;
        let hex = self.substring(hex_start, self.pos);
        if let Ok(code) = u32::from_str_radix(&hex, 16)
            && let Some(c) = char::from_u32(code)
        {
            result.push(c);
        }
    }

    fn scan_string_escape_unicode(&mut self, result: &mut String) {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::OPEN_BRACE {
            self.pos += 1;
            let hex_start = self.pos;
            while self.pos < self.end && is_hex_digit(self.char_code_unchecked(self.pos)) {
                self.pos += 1;
            }
            if self.pos < self.end
                && self.char_code_unchecked(self.pos) == CharacterCodes::CLOSE_BRACE
            {
                let hex = self.substring(hex_start, self.pos);
                self.pos += 1;
                if let Ok(code) = u32::from_str_radix(&hex, 16)
                    && let Some(c) = char::from_u32(code)
                {
                    result.push(c);
                    return;
                }
            }
            // Invalid or unterminated \u{...} escape
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            result.push('\\');
            result.push('u');
            return;
        }
        if self.pos + 4 <= self.end {
            let hex = self.substring(self.pos, self.pos + 4);
            if let Ok(code) = u32::from_str_radix(&hex, 16)
                && let Some(c) = char::from_u32(code)
            {
                self.pos += 4;
                result.push(c);
                return;
            }
            // Invalid \uXXXX escape
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            result.push('\\');
            result.push('u');
            return;
        }
        // Incomplete \u escape (not enough chars)
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        result.push('\\');
        result.push('u');
    }
}
