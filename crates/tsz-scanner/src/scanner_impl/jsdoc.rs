use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl ScannerState {
    // =========================================================================
    // JSDoc Scanning Methods
    // =========================================================================

    /// Scan a `JSDoc` token.
    /// Used when parsing `JSDoc` comments.
    #[wasm_bindgen(js_name = scanJsDocToken)]
    pub fn scan_jsdoc_token(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        self.token_start = self.pos;
        let ch = self.char_code_unchecked(self.pos);

        // Handle newlines
        if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
            self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
            self.pos += 1;
            if ch == CharacterCodes::CARRIAGE_RETURN
                && self.pos < self.end
                && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
            {
                self.pos += 1;
            }
            self.token = SyntaxKind::NewLineTrivia;
            return self.token;
        }

        // Handle whitespace
        if is_white_space_single_line(ch) {
            while self.pos < self.end
                && is_white_space_single_line(self.char_code_unchecked(self.pos))
            {
                self.pos += 1;
            }
            self.token = SyntaxKind::WhitespaceTrivia;
            return self.token;
        }

        if self.scan_jsdoc_punctuation_token(ch) {
            return self.token;
        }

        // Check for identifier
        if self.is_identifier_start(ch) {
            return self.scan_jsdoc_identifier();
        }

        // Unknown character - advance and return Unknown (properly handle multi-byte UTF-8)
        self.scan_jsdoc_unknown_character();
        self.token
    }

    fn scan_jsdoc_punctuation_token(&mut self, ch: u32) -> bool {
        match ch {
            CharacterCodes::AT => {
                self.pos += 1;
                self.token = SyntaxKind::AtToken;
            }
            CharacterCodes::ASTERISK => {
                self.pos += 1;
                self.token = SyntaxKind::AsteriskToken;
            }
            CharacterCodes::OPEN_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBraceToken;
            }
            CharacterCodes::CLOSE_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBraceToken;
            }
            CharacterCodes::OPEN_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBracketToken;
            }
            CharacterCodes::CLOSE_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBracketToken;
            }
            CharacterCodes::LESS_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::LessThanToken;
            }
            CharacterCodes::GREATER_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanToken;
            }
            CharacterCodes::EQUALS => {
                self.pos += 1;
                self.token = SyntaxKind::EqualsToken;
            }
            CharacterCodes::COMMA => {
                self.pos += 1;
                self.token = SyntaxKind::CommaToken;
            }
            CharacterCodes::DOT => {
                self.pos += 1;
                self.token = SyntaxKind::DotToken;
            }
            CharacterCodes::BACKTICK => {
                self.pos += 1;
                while self.pos < self.end
                    && self.char_code_unchecked(self.pos) != CharacterCodes::BACKTICK
                {
                    self.pos += 1;
                }
                if self.pos < self.end {
                    self.pos += 1;
                }
                self.token_value = self.substring(self.token_start, self.pos);
                self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
            }
            _ => return false,
        }
        true
    }

    fn scan_jsdoc_identifier(&mut self) -> SyntaxKind {
        self.pos += self.char_len_at(self.pos);
        while self.pos < self.end && self.is_identifier_part(self.char_code_unchecked(self.pos)) {
            self.pos += self.char_len_at(self.pos);
        }
        self.token_value = self.substring(self.token_start, self.pos);
        self.token = crate::text_to_keyword(&self.token_value).unwrap_or(SyntaxKind::Identifier);
        self.token
    }

    fn scan_jsdoc_unknown_character(&mut self) {
        self.pos += self.char_len_at(self.pos);
        self.token = SyntaxKind::Unknown;
    }

    /// Scan `JSDoc` comment text token.
    /// Used for scanning the text content within `JSDoc` comments.
    #[wasm_bindgen(js_name = scanJsDocCommentTextToken)]
    pub fn scan_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;
        self.token_start = self.pos;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        // Scan until we hit a special character
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);

            // Check for end markers
            match ch {
                // Always stop on newline
                CharacterCodes::LINE_FEED | CharacterCodes::CARRIAGE_RETURN => {
                    break;
                }
                // Stop on @ unless in backticks
                CharacterCodes::AT | CharacterCodes::OPEN_BRACE | CharacterCodes::CLOSE_BRACE
                    if !in_backticks =>
                {
                    break;
                }
                // Stop on backtick - it toggles the mode
                CharacterCodes::BACKTICK => {
                    if self.pos > self.token_start {
                        break; // Return text first
                    }
                    // Return the backtick token
                    self.pos += 1;
                    self.token = SyntaxKind::Unknown; // Use Unknown to signal backtick
                    return self.token;
                }
                _ => {
                    // Properly handle multi-byte UTF-8 characters
                    self.pos += self.char_len_at(self.pos);
                }
            }
        }

        if self.pos > self.token_start {
            self.token_value = self.substring(self.token_start, self.pos);
            // JSDocText token would be returned here, but we use Identifier as a stand-in
            self.token = SyntaxKind::Identifier;
        } else {
            self.token = SyntaxKind::EndOfFileToken;
        }
        self.token
    }

    // =========================================================================
    // Shebang Handling
    // =========================================================================

    /// Scan a shebang (#!) at the start of the file.
    /// Returns the length of the shebang line (including newline), or 0 if no shebang.
    #[wasm_bindgen(js_name = scanShebangTrivia)]
    pub fn scan_shebang_trivia(&mut self) -> usize {
        // Shebang must be at the very start of the file
        if self.pos != 0 {
            return 0;
        }

        // Check for #!
        if self.pos + 1 < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::HASH
            && self.char_code_unchecked(self.pos + 1) == CharacterCodes::EXCLAMATION
        {
            let start = self.pos;
            self.pos += 2;

            // Scan to end of line
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                    break;
                }
                // Use char_len_at to properly handle multi-byte UTF-8 characters
                self.pos += self.char_len_at(self.pos);
            }

            // Include the newline in the shebang
            if self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::CARRIAGE_RETURN {
                    self.pos += 1;
                    if self.pos < self.end
                        && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                    {
                        self.pos += 1;
                    }
                } else if ch == CharacterCodes::LINE_FEED {
                    self.pos += 1;
                }
            }

            return self.pos - start;
        }

        0
    }
}
