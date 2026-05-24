use super::state::{ParseDiagnostic, ParserState};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    pub fn parse_error_at_current_token(&mut self, message: &str, code: u32) {
        let start = self.u32_from_usize(self.scanner.get_token_start());
        let end = self.u32_from_usize(self.scanner.get_token_end());
        self.parse_error_at(start, end - start, message, code);
    }

    /// Emit a companion diagnostic at the current token, bypassing position-based
    /// deduplication.  Use when TSC emits multiple distinct error codes at the same
    /// position (e.g. TS1042 + TS1184 for object-literal modifiers).
    pub(crate) fn parse_companion_error_at_current_token(&mut self, message: &str, code: u32) {
        let start = self.u32_from_usize(self.scanner.get_token_start());
        let end = self.u32_from_usize(self.scanner.get_token_end());
        let length = end - start;
        self.parse_companion_error_at(start, length, message, code);
    }

    pub(crate) fn parse_companion_error_at(
        &mut self,
        start: u32,
        length: u32,
        message: &str,
        code: u32,
    ) {
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
        });
    }
    pub(crate) fn report_invalid_string_or_template_escape_errors(&mut self) {
        // Tagged templates (ES2018+) allow invalid escape sequences per spec.
        // Only untagged templates and string literals should report escape errors.
        if self.in_tagged_template {
            return;
        }
        let token_text = self.scanner.get_token_text_ref().to_string();
        if token_text.is_empty()
            || (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32) == 0
        {
            return;
        }

        let bytes = token_text.as_bytes();
        let token_len = bytes.len();
        let token_start = self.token_pos() as usize;

        let Some((content_start_offset, content_end_offset)) =
            self.string_template_escape_content_span(token_len, bytes)
        else {
            return;
        };

        if content_end_offset <= content_start_offset || content_end_offset > token_len {
            return;
        }

        let raw = &bytes[content_start_offset..content_end_offset];
        let content_start = token_start + content_start_offset;

        let is_template = matches!(
            self.current_token,
            SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::TemplateMiddle
                | SyntaxKind::TemplateTail
        );

        let mut i = 0usize;

        while i < raw.len() {
            if raw[i] != b'\\' {
                i += 1;
                continue;
            }
            if i + 1 >= raw.len() {
                break;
            }
            i = match raw[i + 1] {
                b'x' => self.report_invalid_string_or_template_hex_escape(raw, content_start, i),
                b'u' => {
                    self.report_invalid_string_or_template_unicode_escape(raw, content_start, i)
                }
                // Octal escapes (\0-\7 followed by octal digits) and the invalid
                // \8 / \9 escapes are only reported in template literals here;
                // string literals are handled at scanner-time in tsz-scanner.
                b'0'..=b'9' if is_template => {
                    self.report_invalid_template_octal_escape(raw, content_start, i)
                }
                _ => i + 1,
            };
        }
    }

    pub(crate) fn string_template_escape_content_span(
        &self,
        token_len: usize,
        bytes: &[u8],
    ) -> Option<(usize, usize)> {
        match self.current_token {
            SyntaxKind::StringLiteral => {
                if token_len < 2
                    || (bytes[0] != b'"' && bytes[0] != b'\'')
                    || bytes[token_len - 1] != bytes[0]
                {
                    return None;
                }
                Some((1, token_len - 1))
            }
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                if bytes[0] != b'`' || bytes[token_len - 1] != b'`' {
                    return None;
                }
                Some((1, token_len - 1))
            }
            SyntaxKind::TemplateHead => {
                if bytes[0] != b'`' || !bytes.ends_with(b"${") {
                    return None;
                }
                Some((1, token_len - 2))
            }
            SyntaxKind::TemplateMiddle | SyntaxKind::TemplateTail => {
                if bytes[0] != b'}' {
                    return None;
                }
                if bytes.ends_with(b"${") {
                    Some((1, token_len - 2))
                } else if bytes.ends_with(b"`") {
                    Some((1, token_len - 1))
                } else {
                    Some((1, token_len))
                }
            }
            _ => None,
        }
    }

    pub(crate) fn report_invalid_string_or_template_hex_escape(
        &mut self,
        raw: &[u8],
        content_start: usize,
        i: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let first = i + 2;
        let second = i + 3;
        let err_len = |offset: usize| u32::from(offset < raw.len());

        if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
            self.parse_error_at(
                self.u32_from_usize(content_start + first),
                err_len(first),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
            self.parse_error_at(
                self.u32_from_usize(content_start + second),
                err_len(second),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        }
        i + 2
    }

    pub(crate) fn report_invalid_string_or_template_unicode_escape(
        &mut self,
        raw: &[u8],
        content_start: usize,
        i: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if i + 2 >= raw.len() {
            self.parse_error_at(
                self.u32_from_usize(content_start + i + 2),
                u32::from(i + 2 < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
            return i + 2;
        }

        if raw[i + 2] == b'{' {
            let mut close = i + 3;
            let mut has_digit = false;
            while close < raw.len() && Self::is_hex_digit(raw[close]) {
                has_digit = true;
                close += 1;
            }
            if close >= raw.len() {
                if !has_digit {
                    // No hex digits at all: \u{ followed by end → TS1125
                    self.parse_error_at(
                        self.u32_from_usize(content_start + i + 3),
                        0,
                        diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                        diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                    );
                } else {
                    // Had hex digits but no closing brace → TS1508
                    self.parse_error_at(
                        self.u32_from_usize(content_start + close),
                        0,
                        diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                        diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    );
                }
            } else if raw[close] == b'}' {
                if !has_digit {
                    self.parse_error_at(
                        self.u32_from_usize(content_start + close),
                        1,
                        diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                        diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                    );
                } else {
                    // Check if the value exceeds 0x10FFFF (TS1198)
                    let hex_str = std::str::from_utf8(&raw[i + 3..close]).unwrap_or("");
                    if let Ok(value) = u32::from_str_radix(hex_str, 16)
                        && value > 0x10FFFF
                    {
                        self.parse_error_at(
                            self.u32_from_usize(content_start + i + 3),
                            (close - i - 3) as u32,
                            diagnostic_messages::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                            diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                        );
                    }
                }
            } else if !has_digit {
                self.parse_error_at(
                    self.u32_from_usize(content_start + i + 3),
                    1,
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else {
                self.parse_error_at(
                    self.u32_from_usize(content_start + close),
                    1,
                    diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                );
            }
            close + 1
        } else {
            let first = i + 2;
            let second = i + 3;
            let third = i + 4;
            let fourth = i + 5;
            let err_len = |offset: usize| u32::from(offset < raw.len());

            if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + first),
                    err_len(first),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + second),
                    err_len(second),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if third >= raw.len() || !Self::is_hex_digit(raw[third]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + third),
                    err_len(third),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if fourth >= raw.len() || !Self::is_hex_digit(raw[fourth]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + fourth),
                    err_len(fourth),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            }
            i + 2
        }
    }

    #[inline]
    pub(crate) const fn is_hex_digit(byte: u8) -> bool {
        byte.is_ascii_hexdigit()
    }

    /// Emit TS1487 for legacy octal escapes or TS1488 for `\8`/`\9` inside an
    /// untagged template literal. Mirrors tsc's per-leading-digit maximum:
    /// leading 0-3 allows up to 3 octal digits, leading 4-7 only up to 2.
    pub(crate) fn report_invalid_template_octal_escape(
        &mut self,
        raw: &[u8],
        content_start: usize,
        i: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let is_octal = |b: u8| (b'0'..=b'7').contains(&b);
        let first = raw[i + 1];

        if first == b'8' || first == b'9' {
            let digit = first as char;
            let msg = diagnostic_messages::ESCAPE_SEQUENCE_IS_NOT_ALLOWED
                .replace("{0}", &format!("\\{digit}"));
            self.parse_error_at(
                self.u32_from_usize(content_start + i),
                2,
                &msg,
                diagnostic_codes::ESCAPE_SEQUENCE_IS_NOT_ALLOWED,
            );
            return i + 2;
        }

        // \0 followed by non-digit is the NUL escape — not an octal sequence.
        if first == b'0' && (i + 2 >= raw.len() || !raw[i + 2].is_ascii_digit()) {
            return i + 2;
        }

        // Consume octal digits starting at i+1. Leading 0-3 may pull up to 3 digits;
        // leading 4-7 only 2 (so '\477' parses as '\47' + '7').
        let max_digits = if first <= b'3' { 3 } else { 2 };
        let mut end = i + 2;
        let mut count = 1;
        while count < max_digits && end < raw.len() && is_octal(raw[end]) {
            end += 1;
            count += 1;
        }

        let mut value: u32 = 0;
        for &byte in &raw[i + 1..end] {
            value = value * 8 + u32::from(byte - b'0');
        }
        let suggestion = format!("\\x{value:02x}");
        let msg = diagnostic_messages::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX
            .replace("{0}", &suggestion);
        self.parse_error_at(
            self.u32_from_usize(content_start + i),
            self.u32_from_usize(end - i),
            &msg,
            diagnostic_codes::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
        );
        end
    }

    /// Parse regex escape diagnostics for regex literals.
    #[allow(dead_code)]
    pub(crate) fn report_invalid_regular_expression_escape_errors(&mut self) {
        let token_text = self.scanner.get_token_text_ref().to_string();
        if !token_text.starts_with('/') || token_text.len() < 2 {
            return;
        }

        let bytes = token_text.as_bytes();
        let mut i = 1usize;
        let mut in_escape = false;
        let mut in_character_class = false;
        while i < bytes.len() {
            let ch = bytes[i];
            if in_escape {
                in_escape = false;
                i += 1;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
                i += 1;
                continue;
            }
            if ch == b'[' {
                in_character_class = true;
                i += 1;
                continue;
            }
            if ch == b']' {
                in_character_class = false;
                i += 1;
                continue;
            }
            if ch == b'/' && !in_character_class {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return;
        }

        let body = &token_text[1..i];
        let flags = if i + 1 < token_text.len() {
            &token_text[i + 1..]
        } else {
            ""
        };
        let _ = flags;

        let body_start = self.token_pos() as usize + 1;
        let raw = body.as_bytes();
        let mut j = 0usize;

        while j < raw.len() {
            if raw[j] != b'\\' {
                j += 1;
                continue;
            }
            if j + 1 >= raw.len() {
                break;
            }
            match raw[j + 1] {
                b'x' => {
                    j = self.report_invalid_regular_expression_hex_escape(raw, body_start, j);
                }
                b'u' => {
                    if let Some(next) =
                        self.report_invalid_regular_expression_unicode_escape(raw, body_start, j)
                    {
                        j = next;
                    } else {
                        break;
                    }
                }
                _ => {
                    j += 1;
                }
            }
        }
    }

    pub(crate) fn report_invalid_regular_expression_hex_escape(
        &mut self,
        raw: &[u8],
        body_start: usize,
        j: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        // tsc accepts `_` in regex `\x` escape positions (numericSeparators
        // proposal extended into regex contexts) and defers strict validation
        // to the regex runtime. Mirror that: `_` does not trigger TS1125 here.
        // See `parser.numericSeparators.unicodeEscape.ts` regex files (8, 12,
        // 20, …) where tsc emits no diagnostic for `\xf_f`/`\u_ffff`/etc.
        let is_hex_or_separator = |b: u8| Self::is_hex_digit(b) || b == b'_';

        let first = j + 2;
        let second = j + 3;
        if first >= raw.len() || !is_hex_or_separator(raw[first]) {
            self.parse_error_at(
                self.u32_from_usize(body_start + first),
                u32::from(first < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        } else if second >= raw.len() || !is_hex_or_separator(raw[second]) {
            self.parse_error_at(
                self.u32_from_usize(body_start + second),
                u32::from(second < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        }
        j + 2
    }

    pub(crate) fn report_invalid_regular_expression_unicode_escape(
        &mut self,
        raw: &[u8],
        body_start: usize,
        j: usize,
    ) -> Option<usize> {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if j + 2 < raw.len() && raw[j + 2] == b'{' {
            let mut close = j + 3;
            while close < raw.len() && raw[close] != b'}' {
                close += 1;
            }
            if close >= raw.len() {
                return None;
            }
            Some(close + 1)
        } else {
            // tsc accepts `_` as a numeric separator inside regex `\u` escapes
            // (see `parser.numericSeparators.unicodeEscape.ts` regex files); the
            // strict hex grammar is enforced by the regex runtime, not the
            // parser. Treat `_` as a valid char in any of the four positions.
            let is_hex_or_separator = |b: u8| Self::is_hex_digit(b) || b == b'_';

            let first = j + 2;
            let second = j + 3;
            let third = j + 4;
            let fourth = j + 5;
            if first >= raw.len() || !is_hex_or_separator(raw[first]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + first),
                    u32::from(first < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if second >= raw.len() || !is_hex_or_separator(raw[second]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + second),
                    u32::from(second < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if third >= raw.len() || !is_hex_or_separator(raw[third]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + third),
                    u32::from(third < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if fourth >= raw.len() || !is_hex_or_separator(raw[fourth]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + fourth),
                    u32::from(fourth < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            }
            Some(j + 2)
        }
    }

    // =========================================================================
    // Typed error helper methods (use these instead of parse_error_at_current_token)
    // =========================================================================

    /// Error: Expression expected (TS1109)
    pub(crate) fn error_expression_expected(&mut self) {
        if !self.is_js_file()
            && self.is_token(SyntaxKind::GreaterThanToken)
            && self
                .get_source_text()
                .get(self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize)
                == Some("<")
        {
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading TS1109 errors when TS1005 or other errors already reported
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }
    }

    /// Error: Expression or comma expected (TS1137)
    /// Used in array literal element parsing where tsc uses TS1137 instead of TS1109.
    pub(crate) fn error_expression_or_comma_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::EXPRESSION_OR_COMMA_EXPECTED,
                diagnostic_codes::EXPRESSION_OR_COMMA_EXPECTED,
            );
        }
    }

    /// Error: Argument expression expected (TS1135)
    /// Used in function call argument list parsing instead of generic TS1109.
    pub(crate) fn error_argument_expression_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Argument expression expected.",
                diagnostic_codes::ARGUMENT_EXPRESSION_EXPECTED,
            );
        }
    }

    /// Error: Array element destructuring pattern expected (TS1181)
    /// Used in array binding-pattern-like contexts when an element-like token is invalid.
    pub(crate) fn error_array_element_destructuring_pattern_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Array element destructuring pattern expected.",
                diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED,
            );
        }
    }

    /// Error: Type expected (TS1110)
    pub(crate) fn error_type_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Type expected.", diagnostic_codes::TYPE_EXPECTED);
    }

    /// Error: Identifier expected (TS1003), or Invalid character (TS1127)
    pub(crate) fn error_identifier_expected(&mut self) {
        // Special case: When the current token is JSX text starting with '}',
        // emit TS1005. This handles cases where JSX text containing '}' is
        // encountered when an identifier is expected (e.g., missing closing tag).
        // Note: plain CloseBraceToken should NOT be special-cased here — tsc
        // emits TS1003 "Identifier expected" for '}' in most contexts (class
        // body after `*`, JSX attribute expressions, etc.).
        if self.is_token(SyntaxKind::JsxText) && self.scanner.get_token_value_ref().starts_with("}")
        {
            if self.should_report_error() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'}' expected.", diagnostic_codes::EXPECTED);
            }
            return;
        }
        // When the current token is Unknown (invalid character), emit TS1127
        // instead of TS1003, matching tsc's behavior where the scanner's
        // TS1127 shadows the parser's TS1003 via position-based dedup.
        if self.is_token(SyntaxKind::Unknown) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                diagnostic_codes::INVALID_CHARACTER,
            );
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when a missing token causes identifier to be expected
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            // tsc uses TS1359 for reserved words ("'X' is a reserved word that cannot
            // be used here") and TS1003 for other non-identifier tokens.
            if self.is_reserved_word() {
                let word = self.current_keyword_text();
                let msg = diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE
                    .replace("{0}", word);
                self.parse_error_at_current_token(
                    &msg,
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            } else {
                self.parse_error_at_current_token(
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }
    }

    /// Check if current token is a reserved word that cannot be used as an identifier
    /// Reserved words are keywords from `BreakKeyword` through `WithKeyword`
    #[inline]
    pub(crate) const fn is_reserved_word(&self) -> bool {
        // Match TypeScript's isReservedWord logic:
        // token >= SyntaxKind.FirstReservedWord && token <= SyntaxKind.LastReservedWord
        self.current_token as u16 >= SyntaxKind::FIRST_RESERVED_WORD as u16
            && self.current_token as u16 <= SyntaxKind::LAST_RESERVED_WORD as u16
    }

    /// ES strict-mode future reserved words that can still be parsed as names
    /// so the checker can issue context-specific TS1212/TS1213/TS1214 errors.
    #[inline]
    pub(crate) const fn is_strict_mode_future_reserved_word(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::ImplementsKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::PackageKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::YieldKeyword
        )
    }

    /// Check if current token is a reserved word that namespace-import recovery
    /// should leave for statement parsing.
    #[inline]
    pub(crate) const fn is_namespace_import_recovery_statement_starter(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::WhileKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::ReturnKeyword
        )
    }

    /// Get the text representation of the current keyword token
    pub(crate) const fn current_keyword_text(&self) -> &'static str {
        match self.current_token {
            SyntaxKind::BreakKeyword => "break",
            SyntaxKind::CaseKeyword => "case",
            SyntaxKind::CatchKeyword => "catch",
            SyntaxKind::ClassKeyword => "class",
            SyntaxKind::ConstKeyword => "const",
            SyntaxKind::ContinueKeyword => "continue",
            SyntaxKind::DebuggerKeyword => "debugger",
            SyntaxKind::DefaultKeyword => "default",
            SyntaxKind::DeleteKeyword => "delete",
            SyntaxKind::DoKeyword => "do",
            SyntaxKind::ElseKeyword => "else",
            SyntaxKind::EnumKeyword => "enum",
            SyntaxKind::ExportKeyword => "export",
            SyntaxKind::ExtendsKeyword => "extends",
            SyntaxKind::FalseKeyword => "false",
            SyntaxKind::FinallyKeyword => "finally",
            SyntaxKind::ForKeyword => "for",
            SyntaxKind::FunctionKeyword => "function",
            SyntaxKind::IfKeyword => "if",
            SyntaxKind::ImportKeyword => "import",
            SyntaxKind::InKeyword => "in",
            SyntaxKind::InstanceOfKeyword => "instanceof",
            SyntaxKind::NewKeyword => "new",
            SyntaxKind::NullKeyword => "null",
            SyntaxKind::ReturnKeyword => "return",
            SyntaxKind::SuperKeyword => "super",
            SyntaxKind::SwitchKeyword => "switch",
            SyntaxKind::ThisKeyword => "this",
            SyntaxKind::ThrowKeyword => "throw",
            SyntaxKind::TrueKeyword => "true",
            SyntaxKind::TryKeyword => "try",
            SyntaxKind::TypeOfKeyword => "typeof",
            SyntaxKind::VarKeyword => "var",
            SyntaxKind::VoidKeyword => "void",
            SyntaxKind::WhileKeyword => "while",
            SyntaxKind::WithKeyword => "with",
            // Future reserved words (strict mode)
            SyntaxKind::ImplementsKeyword => "implements",
            SyntaxKind::InterfaceKeyword => "interface",
            SyntaxKind::LetKeyword => "let",
            SyntaxKind::PackageKeyword => "package",
            SyntaxKind::PrivateKeyword => "private",
            SyntaxKind::ProtectedKeyword => "protected",
            SyntaxKind::PublicKeyword => "public",
            SyntaxKind::StaticKeyword => "static",
            SyntaxKind::YieldKeyword => "yield",
            _ => "reserved word",
        }
    }
}
