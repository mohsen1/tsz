use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl ScannerState {
    // =========================================================================
    // JSX Scanning Methods
    // =========================================================================

    /// Scan a JSX identifier.
    /// In JSX, identifiers can contain hyphens (like `data-testid`).
    #[wasm_bindgen(js_name = scanJsxIdentifier)]
    pub fn scan_jsx_identifier(&mut self) -> SyntaxKind {
        if crate::token_is_identifier_or_keyword(self.token) {
            let start = self.token_start;
            let mut decoded = if (self.token_flags & TokenFlags::UnicodeEscape as u32) != 0 {
                Some(self.get_token_value_ref().to_string())
            } else {
                None
            };
            let mut extended = false;
            // Continue scanning to include any hyphenated parts.
            // JSX identifiers can be like: foo-bar-baz, class-id, etc.
            // Keywords like `class` can also start JSX attribute names.
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::MINUS {
                    extended = true;
                    // In JSX, hyphens are allowed in identifiers
                    self.pos += 1;
                    if let Some(text) = decoded.as_mut() {
                        text.push('-');
                    }
                    // After a JSX hyphen, continuation may start with any identifier part.
                    if self.pos >= self.end {
                        continue;
                    }
                    let part_start = self.char_code_unchecked(self.pos);
                    if part_start == CharacterCodes::BACKSLASH {
                        if let Some(code_point) = self.peek_unicode_escape()
                            && self.is_identifier_part(code_point)
                        {
                            let text =
                                decoded.get_or_insert_with(|| self.source[start..self.pos].into());
                            if let Some(c) =
                                char::from_u32(self.scan_unicode_escape_value().unwrap_or(0))
                            {
                                text.push(c);
                            }
                            while self.pos < self.end {
                                let ch = self.char_code_unchecked(self.pos);
                                if ch == CharacterCodes::BACKSLASH {
                                    if let Some(code_point) = self.peek_unicode_escape()
                                        && self.is_identifier_part(code_point)
                                    {
                                        if let Some(c) = char::from_u32(
                                            self.scan_unicode_escape_value().unwrap_or(0),
                                        ) {
                                            text.push(c);
                                        }
                                        continue;
                                    }
                                    break;
                                }
                                if !self.is_identifier_part(ch) {
                                    break;
                                }
                                if let Some(c) = char::from_u32(ch) {
                                    text.push(c);
                                }
                                self.pos += self.char_len_at(self.pos);
                            }
                        }
                    } else if self.is_identifier_part(part_start) {
                        loop {
                            let ch = self.char_code_unchecked(self.pos);
                            if !self.is_identifier_part(ch) {
                                break;
                            }
                            if let Some(text) = decoded.as_mut()
                                && let Some(c) = char::from_u32(ch)
                            {
                                text.push(c);
                            }
                            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                            if self.pos >= self.end {
                                break;
                            }
                        }
                    }
                } else {
                    break;
                }
            }
            if extended {
                if let Some(text) = decoded {
                    self.token_atom = self.interner.intern(&text);
                    self.token_flags |= TokenFlags::UnicodeEscape as u32;
                } else {
                    // ZERO-ALLOCATION: Intern directly from source slice, clear token_value
                    self.token_atom = self.interner.intern(&self.source[start..self.pos]);
                }
                self.token_value.clear();
                // After extending with hyphens, the token becomes an Identifier
                self.token = SyntaxKind::Identifier;
            }
        }
        self.token
    }

    /// Re-scan the current token as a JSX token.
    /// Used when the parser enters JSX context and needs to rescan.
    /// Must reset to `full_start_pos` (before trivia), not `token_start` (after trivia),
    /// so that JSX text nodes include leading whitespace/newlines.
    /// Matches tsc: `pos = tokenStart = fullStartPos;`
    #[wasm_bindgen(js_name = reScanJsxToken)]
    pub fn re_scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        // Remove any scanner diagnostics emitted at positions >= full_start_pos.
        // The previous scan (in normal JS mode) may have produced false diagnostics
        // (e.g., TS1351 for `7x` in JSX text content). Since we're rescanning this
        // range in JSX mode, those diagnostics are invalid.
        let rescan_start = self.full_start_pos;
        self.scanner_diagnostics.retain(|d| d.pos < rescan_start);
        self.pos = self.full_start_pos;
        self.scan_jsx_token(allow_multiline_jsx_text)
    }

    /// Scan a JSX token (text, open element, close element, etc.)
    fn scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_start = self.pos;
        // Clear stale atom from any prior identifier scan so get_token_value_ref()
        // returns the freshly scanned JSX text, not the old interned identifier.
        self.token_atom = Atom::NONE;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        // A merge-conflict marker (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`)
        // can appear inside JSX child content. Check before the LESS_THAN
        // angle-bracket path: a 7-`<` run at line start is a conflict marker,
        // not the start of a nested JSX tag. Mirrors the regular-mode handler
        // which checks `is_conflict_marker_trivia()` first for `<`/`=`/`>`/`|`.
        if self.is_conflict_marker_trivia() {
            self.scan_conflict_marker_trivia();
            if self.skip_trivia {
                return self.scan_jsx_token(allow_multiline_jsx_text);
            }
            self.token = SyntaxKind::ConflictMarkerTrivia;
            return self.token;
        }

        let ch = self.char_code_unchecked(self.pos);

        // Check for JSX opening/closing angle brackets
        if ch == CharacterCodes::LESS_THAN {
            // Check for </
            if self.char_code_at(self.pos + 1) == Some(CharacterCodes::SLASH) {
                self.pos += 2;
                self.token = SyntaxKind::LessThanSlashToken;
                return self.token;
            }
            self.pos += 1;
            self.token = SyntaxKind::LessThanToken;
            return self.token;
        }

        if ch == CharacterCodes::OPEN_BRACE {
            self.pos += 1;
            self.token = SyntaxKind::OpenBraceToken;
            return self.token;
        }

        // Scan JSX text
        let mut text = String::new();
        while self.pos < self.end {
            let c = self.char_code_unchecked(self.pos);

            // Stop on JSX special characters
            if c == CharacterCodes::OPEN_BRACE || c == CharacterCodes::LESS_THAN {
                break;
            }

            // TS1382: bare `>` in JSX text
            if c == CharacterCodes::GREATER_THAN {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: self.pos,
                    length: 1,
                    message: diagnostic_messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                    code: diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                    args: Vec::new(),
                });
            }

            // TS1381: bare `}` in JSX text
            if c == CharacterCodes::CLOSE_BRACE {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: self.pos,
                    length: 1,
                    message: diagnostic_messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                    code: diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                    args: Vec::new(),
                });
            }

            // Handle newlines in JSX text
            if is_line_break(c) {
                if !allow_multiline_jsx_text {
                    break;
                }
                self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
            }

            if let Some(char) = char::from_u32(c) {
                text.push(char);
            }
            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
        }

        if !text.is_empty() {
            self.token_value = text;
            self.token = SyntaxKind::JsxText;
            return self.token;
        }

        self.token = SyntaxKind::Unknown;
        self.token
    }

    /// Scan a JSX attribute value (string literal or expression).
    #[wasm_bindgen(js_name = scanJsxAttributeValue)]
    pub fn scan_jsx_attribute_value(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;

        // Skip whitespace
        while self.pos < self.end && is_white_space_single_line(self.char_code_unchecked(self.pos))
        {
            self.pos += 1;
        }

        self.token_start = self.pos;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        let ch = self.char_code_unchecked(self.pos);

        // String literal
        if ch == CharacterCodes::DOUBLE_QUOTE || ch == CharacterCodes::SINGLE_QUOTE {
            self.scan_jsx_string_literal(ch);
            return self.token;
        }

        self.scan()
    }

    /// Scan a JSX string literal (used for attribute values).
    /// Unlike regular strings, JSX strings don't support escape sequences.
    fn scan_jsx_string_literal(&mut self, quote: u32) {
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
            // JSX strings don't process escape sequences - they're literal
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            // Advance by the character's UTF-8 byte length so multi-byte chars
            // are not re-decoded from a continuation-byte offset.
            self.pos += self.char_len_at(self.pos);
        }

        // Unterminated string
        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = result;
        self.token = SyntaxKind::StringLiteral;
    }

    /// Re-scan a JSX attribute value from the current token position.
    #[wasm_bindgen(js_name = reScanJsxAttributeValue)]
    pub fn re_scan_jsx_attribute_value(&mut self) -> SyntaxKind {
        self.pos = self.token_start;
        self.scan_jsx_attribute_value()
    }
}
