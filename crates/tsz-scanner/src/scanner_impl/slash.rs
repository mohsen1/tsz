use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl ScannerState {
    // =========================================================================
    // Rescan methods - for context-sensitive parsing
    //
    // Many rescan modes live in the `rescan` sibling module so the mode-shifting
    // surface is isolated from the main `scan()` loop. Template, slash, and JSX
    // rescans remain here because they share many private scanning helpers with
    // the main scan path.
    // =========================================================================

    /// Re-scan the current `/` or `/=` token as a regex literal.
    /// This is used by the parser when it determines the context requires a regex.
    #[wasm_bindgen(js_name = reScanSlashToken)]
    pub fn re_scan_slash_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::SlashToken || self.token == SyntaxKind::SlashEqualsToken {
            // Start scanning from after the initial /
            let start_of_regex_body = self.token_start + 1;
            self.pos = start_of_regex_body;
            let mut in_escape = false;
            let mut in_character_class = false;

            // Scan until we find the closing /
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);

                // Unterminated regex if we hit a newline
                if is_line_break(ch) {
                    self.token_flags |= TokenFlags::Unterminated as u32;
                    break;
                }

                if in_escape {
                    // After backslash, just consume the next character
                    in_escape = false;
                } else if ch == CharacterCodes::SLASH && !in_character_class {
                    // Found the closing /
                    break;
                } else if ch == CharacterCodes::OPEN_BRACKET {
                    in_character_class = true;
                } else if ch == CharacterCodes::BACKSLASH {
                    in_escape = true;
                } else if ch == CharacterCodes::CLOSE_BRACKET {
                    in_character_class = false;
                }
                // Use char_len_at to properly advance past multi-byte UTF-8 characters
                self.pos += self.char_len_at(self.pos);
            }

            // If we reached EOF without finding closing /, mark as unterminated
            if self.pos >= self.end && (self.token_flags & TokenFlags::Unterminated as u32) == 0 {
                self.token_flags |= TokenFlags::Unterminated as u32;
            }

            if (self.token_flags & TokenFlags::Unterminated as u32) == 0 {
                // Consume the closing /
                self.pos += 1;

                // Scan and validate regex flags (g, i, m, s, u, v, y, d)
                // Track seen flags as a bitmask for duplicate detection
                let mut seen_flags: u8 = 0;
                let mut has_u = false;
                let mut has_v = false;

                while self.pos < self.end {
                    let ch = self.char_code_unchecked(self.pos);
                    if !is_regex_flag(ch) && !is_identifier_part(ch) {
                        break;
                    }

                    // Check for valid flags and detect errors
                    let flag_bit = match ch {
                        CharacterCodes::LOWER_G => Some(0),
                        CharacterCodes::LOWER_I => Some(1),
                        CharacterCodes::LOWER_M => Some(2),
                        CharacterCodes::LOWER_S => Some(3),
                        CharacterCodes::LOWER_U => {
                            has_u = true;
                            Some(4)
                        }
                        CharacterCodes::LOWER_V => {
                            has_v = true;
                            Some(5)
                        }
                        CharacterCodes::LOWER_Y => Some(6),
                        CharacterCodes::LOWER_D => Some(7),
                        _ => None,
                    };

                    if let Some(bit) = flag_bit {
                        let mask = 1 << bit;
                        if seen_flags & mask != 0 {
                            // Duplicate flag - emit error for each duplicate
                            self.regex_flag_errors.push(RegexFlagError {
                                kind: RegexFlagErrorKind::Duplicate,
                                pos: self.pos,
                            });
                        }
                        seen_flags |= mask;
                    } else if is_identifier_part(ch) {
                        // Invalid flag character (identifier char but not a valid flag)
                        self.regex_flag_errors.push(RegexFlagError {
                            kind: RegexFlagErrorKind::InvalidFlag,
                            pos: self.pos,
                        });
                    }

                    // Use char_len_at for proper UTF-8 handling (handles non-ASCII flags)
                    self.pos += self.char_len_at(self.pos);
                }

                // Check for incompatible u and v flags
                if has_u && has_v {
                    // Emit error at the end of flags (similar to TypeScript)
                    self.regex_flag_errors.push(RegexFlagError {
                        kind: RegexFlagErrorKind::IncompatibleFlags,
                        pos: self.pos,
                    });
                }
            }

            self.token_value = self.substring(self.token_start, self.pos);
            self.token = SyntaxKind::RegularExpressionLiteral;
        }
        self.token
    }
}
