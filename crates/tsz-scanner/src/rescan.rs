//! Contextual rescan operations for the scanner.
//!
//! A *rescan* is a parser-driven re-interpretation of the most recently scanned
//! token: the parser has discovered enough additional context to know that the
//! same source span should be tokenized differently. For example, after the
//! main scanner emits `>` as `GreaterThanToken` in a generic-argument context,
//! the parser invokes [`ScannerState::re_scan_greater_token`] to widen the
//! token to `>>`, `>>=`, etc.
//!
//! Each rescan in this module documents its precondition (which kind the
//! current token must be) and is a no-op when the precondition is not met.

use crate::SyntaxKind;
use crate::char_codes::CharacterCodes;
use crate::scanner_impl::{ScannerState, is_digit};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl ScannerState {
    /// Re-scan the current `>` token to see if it should be `>=`, `>>`, `>>>`, `>>=`, or `>>>=`.
    /// This is used by the parser for type arguments and bitwise operators.
    ///
    /// Precondition: current token is `GreaterThanToken`. If not, this is a
    /// no-op and the existing token is returned.
    #[wasm_bindgen(js_name = reScanGreaterToken)]
    pub fn re_scan_greater_token(&mut self) -> SyntaxKind {
        if self.token != SyntaxKind::GreaterThanToken {
            return self.token;
        }
        let next_char = self.char_code_unchecked(self.pos);
        if next_char == CharacterCodes::GREATER_THAN {
            let next_next = self.char_code_unchecked(self.pos + 1);
            if next_next == CharacterCodes::GREATER_THAN {
                let next_next_next = self.char_code_unchecked(self.pos + 2);
                if next_next_next == CharacterCodes::EQUALS {
                    self.pos += 3;
                    self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken;
                    return self.token;
                }
                self.pos += 2;
                self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanToken;
                return self.token;
            }
            if next_next == CharacterCodes::EQUALS {
                self.pos += 2;
                self.token = SyntaxKind::GreaterThanGreaterThanEqualsToken;
                return self.token;
            }
            self.pos += 1;
            self.token = SyntaxKind::GreaterThanGreaterThanToken;
            return self.token;
        }
        if next_char == CharacterCodes::EQUALS {
            self.pos += 1;
            self.token = SyntaxKind::GreaterThanEqualsToken;
            return self.token;
        }
        self.token
    }

    /// Re-scan the current `*=` token as `*` followed by `=`.
    /// Used when parsing computed property names.
    ///
    /// Precondition: current token is `AsteriskEqualsToken`. If not, this is
    /// a no-op.
    #[wasm_bindgen(js_name = reScanAsteriskEqualsToken)]
    pub fn re_scan_asterisk_equals_token(&mut self) -> SyntaxKind {
        if self.token != SyntaxKind::AsteriskEqualsToken {
            return self.token;
        }
        self.pos = self.token_start + 1;
        self.token = SyntaxKind::EqualsToken;
        self.token
    }

    /// Re-scan a `<` token in JSX context.
    /// Returns `LessThanSlashToken` if followed by `/`, otherwise `LessThanToken`.
    ///
    /// Precondition: current token is `LessThanToken`. If not, this is a
    /// no-op.
    #[wasm_bindgen(js_name = reScanLessThanToken)]
    pub fn re_scan_less_than_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::LessThanToken
            && self.pos < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::SLASH
        {
            self.pos += 1;
            self.token = SyntaxKind::LessThanSlashToken;
        }
        self.token
    }

    /// Re-scan the current `?` token for optional chaining or nullish coalescing.
    /// May produce `QuestionDotToken`, `QuestionQuestionToken`, or
    /// `QuestionQuestionEqualsToken`.
    ///
    /// Precondition: current token is `QuestionToken`. If not, this is a
    /// no-op.
    #[wasm_bindgen(js_name = reScanQuestionToken)]
    pub fn re_scan_question_token(&mut self) -> SyntaxKind {
        if self.token != SyntaxKind::QuestionToken {
            return self.token;
        }
        let ch = self.char_code_at(self.pos);
        if ch == Some(CharacterCodes::DOT) {
            // `?.<digit>` is the ternary `?` followed by a decimal literal,
            // not optional chaining, so don't widen in that case.
            let next = self.char_code_at(self.pos + 1);
            if !next.is_some_and(is_digit) {
                self.pos += 1;
                self.token = SyntaxKind::QuestionDotToken;
            }
        } else if ch == Some(CharacterCodes::QUESTION) {
            if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                self.pos += 2;
                self.token = SyntaxKind::QuestionQuestionEqualsToken;
            } else {
                self.pos += 1;
                self.token = SyntaxKind::QuestionQuestionToken;
            }
        }
        self.token
    }

    /// Re-scan the current `#` token as a hash token or private identifier.
    /// Used by the parser when it encounters `#name` and needs to recognize
    /// it as a single private-identifier token.
    ///
    /// Precondition: current token is `HashToken`. If not, this is a no-op.
    #[wasm_bindgen(js_name = reScanHashToken)]
    pub fn re_scan_hash_token(&mut self) -> SyntaxKind {
        if self.token != SyntaxKind::HashToken || self.pos >= self.end {
            return self.token;
        }
        let ch = self.char_code_unchecked(self.pos);
        if self.is_identifier_start(ch) {
            // Advance by UTF-8 char width so a multi-byte identifier start
            // doesn't leave `pos` mid-codepoint.
            self.pos += self.char_len_at(self.pos);
            let has_escapes = self.scan_private_identifier_rest();
            if !has_escapes {
                self.token_value = self.substring(self.token_start, self.pos);
            }
            self.token = SyntaxKind::PrivateIdentifier;
        } else if ch == CharacterCodes::BACKSLASH {
            // Unicode escape starting a private identifier, e.g. `#x`.
            if let Some(code_point) = self.peek_unicode_escape()
                && self.is_identifier_start(code_point)
            {
                self.scan_private_identifier_with_escapes();
            }
        }
        self.token
    }

    /// Re-scan an invalid identifier to check if it's valid in a specific context.
    ///
    /// When the main scanner emits `Unknown` because the source byte at the
    /// token position is not a valid token start, the parser can still ask
    /// the scanner to re-evaluate the recorded `token_value` as an identifier
    /// or keyword. This is the rescue path that lets, e.g., a Unicode
    /// identifier survive scanner-side classification errors.
    ///
    /// Precondition: current token is `Unknown` with a non-empty token value.
    #[wasm_bindgen(js_name = reScanInvalidIdentifier)]
    pub fn re_scan_invalid_identifier(&mut self) -> SyntaxKind {
        if self.token != SyntaxKind::Unknown || self.token_value.is_empty() {
            return self.token;
        }
        // Use the same identifier-class functions as the pre-refactor body
        // so this rescan is byte-equivalent to the original implementation.
        let mut chars = self.token_value.chars();
        let Some(first) = chars.next() else {
            return self.token;
        };
        if !crate::scanner_impl::is_identifier_start(first as u32) {
            return self.token;
        }
        if !chars.all(|c| crate::scanner_impl::is_identifier_part(c as u32)) {
            return self.token;
        }
        self.token = crate::text_to_keyword(&self.token_value).unwrap_or(SyntaxKind::Identifier);
        self.token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_one(source: &str) -> ScannerState {
        let mut scanner = ScannerState::new(source.to_string(), true);
        scanner.scan();
        scanner
    }

    /// Park the scanner so its current token is `forced_token` and `pos`
    /// points just past the first byte of `source`. Used to test rescans
    /// the main `scan()` loop would otherwise short-circuit by widening
    /// the token before the rescan can see it.
    fn scanner_with_forced_token(source: &str, forced_token: SyntaxKind) -> ScannerState {
        let mut scanner = ScannerState::new(source.to_string(), true);
        scanner.token = forced_token;
        scanner.token_start = 0;
        scanner.pos = 1;
        scanner
    }

    // ── re_scan_greater_token ─────────────────────────────────────────
    //
    // The five widening transitions (>=, >>, >>>, >>=, >>>=) are covered
    // end-to-end in `tests/scanner_comprehensive_tests.rs::rescan_methods`
    // and `tests/scanner_impl_tests.rs`. Module-level coverage just locks
    // in the precondition guard.

    #[test]
    fn greater_token_rescan_is_noop_on_non_greater_token() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        assert_eq!(scanner.re_scan_greater_token(), SyntaxKind::Identifier);
    }

    // ── re_scan_asterisk_equals_token ─────────────────────────────────

    #[test]
    fn asterisk_equals_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one("*");
        assert_eq!(scanner.get_token(), SyntaxKind::AsteriskToken);
        assert_eq!(
            scanner.re_scan_asterisk_equals_token(),
            SyntaxKind::AsteriskToken
        );
    }

    // ── re_scan_less_than_token ───────────────────────────────────────

    #[test]
    fn less_than_unchanged_when_not_followed_by_slash() {
        let mut scanner = scan_one("<a");
        assert_eq!(scanner.get_token(), SyntaxKind::LessThanToken);
        assert_eq!(scanner.re_scan_less_than_token(), SyntaxKind::LessThanToken);
    }

    #[test]
    fn less_than_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one(">");
        assert_eq!(scanner.get_token(), SyntaxKind::GreaterThanToken);
        assert_eq!(
            scanner.re_scan_less_than_token(),
            SyntaxKind::GreaterThanToken
        );
    }

    // ── re_scan_question_token ────────────────────────────────────────
    //
    // The main `scan()` already widens `?` to `?.`, `??`, or `??=` when the
    // following bytes call for it. The forced-state tests below cover the
    // rescan body itself; the no-op tests cover the precondition guard.

    #[test]
    fn question_widens_to_question_dot() {
        let mut scanner = scanner_with_forced_token("?.foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionDotToken
        );
    }

    #[test]
    fn question_does_not_widen_when_followed_by_digit() {
        // `?.1` is the ternary `?` followed by the numeric `.1`, not `?.`.
        let mut scanner = scanner_with_forced_token("?.1", SyntaxKind::QuestionToken);
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::QuestionToken);
    }

    #[test]
    fn question_widens_to_question_question() {
        let mut scanner = scanner_with_forced_token("??foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionQuestionToken
        );
    }

    #[test]
    fn question_widens_to_question_question_equals() {
        let mut scanner = scanner_with_forced_token("??= foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionQuestionEqualsToken
        );
    }

    #[test]
    fn question_remains_when_alone() {
        let mut scanner = scan_one("? ");
        assert_eq!(scanner.get_token(), SyntaxKind::QuestionToken);
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::QuestionToken);
    }

    #[test]
    fn question_rescan_is_noop_on_non_question() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::Identifier);
    }

    // ── re_scan_hash_token ────────────────────────────────────────────
    //
    // The main `scan()` already widens `#name` directly to `PrivateIdentifier`,
    // so the rescan body only triggers on forced state or a bare `#` followed
    // by a non-identifier byte.

    #[test]
    fn hash_widens_to_private_identifier_when_forced() {
        let mut scanner = scanner_with_forced_token("#name", SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#name");
    }

    #[test]
    fn hash_widens_with_underscore_prefix_when_forced() {
        let mut scanner = scanner_with_forced_token("#_private", SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#_private");
    }

    #[test]
    fn hash_remains_when_not_followed_by_identifier_start() {
        let mut scanner = scan_one("#1");
        assert_eq!(scanner.get_token(), SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::HashToken);
    }

    #[test]
    fn hash_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::Identifier);
    }

    // ── re_scan_invalid_identifier ────────────────────────────────────

    fn forced_unknown(value: &str) -> ScannerState {
        let mut scanner = ScannerState::new(String::new(), true);
        scanner.token = SyntaxKind::Unknown;
        scanner.token_value = String::from(value);
        scanner
    }

    #[test]
    fn invalid_identifier_rescue_recovers_identifier_when_chars_are_valid() {
        let mut scanner = forced_unknown("foo");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Identifier);
    }

    #[test]
    fn invalid_identifier_rescue_recognizes_keyword() {
        let mut scanner = forced_unknown("class");
        assert_eq!(
            scanner.re_scan_invalid_identifier(),
            SyntaxKind::ClassKeyword
        );
    }

    #[test]
    fn invalid_identifier_rescue_rejects_non_identifier_start() {
        // Leading digit is not a valid identifier start.
        let mut scanner = forced_unknown("1abc");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }

    #[test]
    fn invalid_identifier_rescue_rejects_invalid_continuation() {
        let mut scanner = forced_unknown("foo!bar");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }

    #[test]
    fn invalid_identifier_rescue_is_noop_on_empty_value() {
        let mut scanner = ScannerState::new(String::new(), true);
        scanner.token = SyntaxKind::Unknown;
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }

    #[test]
    fn invalid_identifier_rescue_is_noop_when_token_is_not_unknown() {
        let mut scanner = scan_one("foo");
        scanner.token_value = String::from("class");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Identifier);
    }
}
