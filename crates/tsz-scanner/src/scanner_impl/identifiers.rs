use super::*;

impl ScannerState {
    /// Scan an identifier.
    /// ZERO-ALLOCATION: Identifiers are interned, returning an Atom (u32) for O(1) comparison.
    /// When a unicode escape is encountered mid-identifier, switches to allocation mode.
    pub(crate) fn scan_identifier(&mut self) {
        let start = self.pos;
        // Advance past first character (may be multi-byte)
        self.pos += self.char_len_at(self.pos);

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Check if this is a unicode escape that produces an identifier part
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    // Switch to allocation mode and continue scanning with escapes
                    self.continue_identifier_with_escapes(start);
                    return;
                }
                if self.peek_unicode_escape().is_some() {
                    self.push_invalid_character(self.pos);
                }
                // Invalid escape or not an identifier part - stop here
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
        }

        // Get slice reference instead of allocating new String
        let text_slice = &self.source[start..self.pos];

        // Check if it's a keyword first (common keywords are pre-interned)
        self.token = crate::text_to_keyword(text_slice).unwrap_or(SyntaxKind::Identifier);

        // Intern the identifier for O(1) comparison (reuses existing interned string)
        self.token_atom = self.interner.intern(text_slice);

        // ZERO-ALLOCATION: Don't store token_value for identifiers.
        // get_token_value_ref() will resolve from token_atom or fall back to source slice.
        self.token_value.clear();
    }

    /// Continue scanning an identifier that has a unicode escape mid-identifier.
    /// Called when `scan_identifier()` encounters a valid unicode escape.
    /// This switches to allocation mode since escapes require building a String.
    fn continue_identifier_with_escapes(&mut self, start: usize) {
        // Copy the already-scanned part into a String
        let mut result = String::from(&self.source[start..self.pos]);

        // Continue scanning identifier parts, handling unicode escapes
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Check for unicode escape
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    // Consume the escape and add the character
                    if let Some(c) = char::from_u32(self.scan_unicode_escape_value().unwrap_or(0)) {
                        result.push(c);
                    }
                    continue;
                }
                if self.peek_unicode_escape().is_some() {
                    self.push_invalid_character(self.pos);
                }
                // Invalid escape or not an identifier part - stop here
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += self.char_len_at(self.pos);
        }

        self.token = crate::text_to_keyword(&result).unwrap_or(SyntaxKind::Identifier);
        self.token_atom = self.interner.intern(&result);
        self.token_value.clear();
        self.token_flags |= TokenFlags::UnicodeEscape as u32;
    }

    /// Peek at a unicode escape sequence in identifier text without advancing
    /// the position. Returns the code point if the escape is valid
    /// (`\uXXXX`, or `\u{XXXXX}` for a valid Unicode scalar), None otherwise.
    pub(crate) fn peek_unicode_escape(&self) -> Option<u32> {
        // Must start with \u
        if self.pos + 1 >= self.end {
            return None;
        }
        let bytes = self.source.as_bytes();
        if bytes.get(self.pos + 1).copied() != Some(b'u') {
            return None;
        }
        // \u{XXXXX} form
        if bytes.get(self.pos + 2).copied() == Some(b'{') {
            let start = self.pos + 3;
            let mut end = start;
            while end < self.end && bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
                end += 1;
            }
            if end == start || bytes.get(end).copied() != Some(b'}') {
                return None;
            }
            let hex = &self.source[start..end];
            u32::from_str_radix(hex, 16)
                .ok()
                .filter(|&cp| char::from_u32(cp).is_some())
        } else {
            // \uXXXX form (exactly 4 hex digits)
            if self.pos + 5 >= self.end {
                return None;
            }
            let hex = &self.source[self.pos + 2..self.pos + 6];
            if hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                None
            }
        }
    }

    /// Scan an identifier that starts with a unicode escape sequence (\uXXXX).
    pub(crate) fn scan_identifier_with_escapes(&mut self) {
        let mut result = String::new();

        // Process initial unicode escape
        if let Some(ch) = self.scan_unicode_escape_value()
            && let Some(c) = char::from_u32(ch)
        {
            result.push(c);
        }

        // Continue scanning identifier parts
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Another unicode escape in identifier
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    if let Some(c) = char::from_u32(self.scan_unicode_escape_value().unwrap_or(0)) {
                        result.push(c);
                    }
                    continue;
                }
                if self.peek_unicode_escape().is_some() {
                    self.push_invalid_character(self.pos);
                }
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += self.char_len_at(self.pos);
        }

        self.token = crate::text_to_keyword(&result).unwrap_or(SyntaxKind::Identifier);
        self.token_atom = self.interner.intern(&result);
        self.token_value.clear();
        self.token_flags |= TokenFlags::UnicodeEscape as u32;
    }

    /// Scan continuation of a private identifier that starts with a regular char.
    /// Handles unicode escapes in the continuation (e.g., `#x\u0078`).
    /// Returns true if any unicode escapes were found and `token_value` was set.
    pub(crate) fn scan_private_identifier_rest(&mut self) -> bool {
        // First, scan regular identifier parts
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Found a unicode escape in the continuation
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    // Build the decoded value from the start
                    let prefix = self.substring(self.token_start, self.pos);
                    let mut result = prefix;
                    // Consume the escape
                    if let Some(cp) = self.scan_unicode_escape_value()
                        && let Some(c) = char::from_u32(cp)
                    {
                        result.push(c);
                    }
                    // Continue scanning the rest
                    while self.pos < self.end {
                        let ch2 = self.char_code_unchecked(self.pos);
                        if ch2 == CharacterCodes::BACKSLASH {
                            if let Some(cp2) = self.peek_unicode_escape()
                                && self.is_identifier_part(cp2)
                            {
                                if let Some(cp2) = self.scan_unicode_escape_value()
                                    && let Some(c) = char::from_u32(cp2)
                                {
                                    result.push(c);
                                }
                                continue;
                            }
                            if self.peek_unicode_escape().is_some() {
                                self.push_invalid_character(self.pos);
                            }
                            break;
                        }
                        if !self.is_identifier_part(ch2) {
                            break;
                        }
                        if let Some(c) = char::from_u32(ch2) {
                            result.push(c);
                        }
                        self.pos += self.char_len_at(self.pos);
                    }
                    self.token_value = result;
                    self.token_flags |= TokenFlags::UnicodeEscape as u32;
                    return true;
                }
                if self.peek_unicode_escape().is_some() {
                    self.push_invalid_character(self.pos);
                }
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos);
        }
        false
    }

    /// Scan a private identifier that starts with a unicode escape: `#\u0078`.
    pub(crate) fn scan_private_identifier_with_escapes(&mut self) {
        let mut result = String::from("#");

        // Process initial unicode escape
        if let Some(ch) = self.scan_unicode_escape_value()
            && let Some(c) = char::from_u32(ch)
        {
            result.push(c);
        }

        // Continue scanning identifier parts
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Another unicode escape in identifier
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    if let Some(c) = char::from_u32(self.scan_unicode_escape_value().unwrap_or(0)) {
                        result.push(c);
                    }
                    continue;
                }
                if self.peek_unicode_escape().is_some() {
                    self.push_invalid_character(self.pos);
                }
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += self.char_len_at(self.pos);
        }

        self.token = SyntaxKind::PrivateIdentifier;
        self.token_atom = self.interner.intern(&result);
        self.token_value = result;
        self.token_flags |= TokenFlags::UnicodeEscape as u32;
    }

    /// Consume a unicode escape sequence and return its code point.
    /// Advances self.pos past the escape.
    pub(crate) fn scan_unicode_escape_value(&mut self) -> Option<u32> {
        // Skip the backslash
        self.pos += 1;
        if self.pos >= self.end || self.source.as_bytes()[self.pos] != b'u' {
            return None;
        }
        self.pos += 1; // Skip 'u'

        if self.pos < self.end && self.source.as_bytes()[self.pos] == b'{' {
            // \u{XXXXX} form
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.end
                && self
                    .source
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(u8::is_ascii_hexdigit)
            {
                self.pos += 1;
            }
            let result = u32::from_str_radix(&self.source[start..self.pos], 16).ok();
            if self.pos < self.end && self.source.as_bytes()[self.pos] == b'}' {
                self.pos += 1;
            }
            result
        } else {
            // \uXXXX form (exactly 4 hex digits)
            if self.pos + 4 > self.end {
                return None;
            }
            let hex = &self.source[self.pos..self.pos + 4];
            if hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                self.pos += 4;
                u32::from_str_radix(hex, 16).ok()
            } else {
                None
            }
        }
    }
}
