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

                // Scan and validate regex flags (g, i, m, s, u, v, y, d).
                // `RegexFlagScan` mirrors tsc's per-position verdict: a
                // `u`/`v` conflict is checked and reported AS EACH FLAG IS
                // ACCEPTED (not once after the whole run), and wins over a
                // plain duplicate at that same position.
                let mut flag_scan = crate::regex_flags::RegexFlagScan::new();

                while self.pos < self.end {
                    let ch = self.char_code_unchecked(self.pos);
                    if !is_regex_flag(ch) && !is_identifier_part(ch) {
                        break;
                    }

                    if is_regex_flag(ch) {
                        // `is_regex_flag` only matches the ASCII flag letters, so this is lossless.
                        let kind = match flag_scan.advance(ch as u8) {
                            crate::regex_flags::RegexFlagVerdict::Accepted => None,
                            crate::regex_flags::RegexFlagVerdict::Duplicate => {
                                Some(RegexFlagErrorKind::Duplicate)
                            }
                            crate::regex_flags::RegexFlagVerdict::Conflict => {
                                Some(RegexFlagErrorKind::IncompatibleFlags)
                            }
                        };
                        if let Some(kind) = kind {
                            self.regex_flag_errors.push(RegexFlagError {
                                kind,
                                pos: self.pos,
                            });
                        }
                    } else {
                        // Invalid flag character (identifier char but not a valid flag)
                        self.regex_flag_errors.push(RegexFlagError {
                            kind: RegexFlagErrorKind::InvalidFlag,
                            pos: self.pos,
                        });
                    }

                    // Use char_len_at for proper UTF-8 handling (handles non-ASCII flags)
                    self.pos += self.char_len_at(self.pos);
                }
            }

            self.token_value = self.substring(self.token_start, self.pos);
            self.token = SyntaxKind::RegularExpressionLiteral;
        }
        self.token
    }
}
