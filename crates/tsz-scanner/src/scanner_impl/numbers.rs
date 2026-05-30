use super::*;

impl ScannerState {
    /// Scan a number literal (simplified).
    pub(crate) fn scan_number(&mut self) {
        let start = self.pos;

        // Check for hex, octal, binary
        if self.char_code_unchecked(self.pos) == CharacterCodes::_0 {
            let next = self.char_code_at(self.pos + 1).unwrap_or(0);
            if self.scan_prefixed_number(start, next) {
                return;
            }

            // After leading 0, scan all consecutive digits and check if all are octal.
            // This matches tsc's scanDigits(): scan 0-9, return whether all were 0-7.
            if is_digit(next) && self.scan_legacy_octal_number(start) {
                return;
            }

            // Numeric separator immediately after a leading zero (e.g. `0_0`,
            // `0_1.5`, `0__0`) — tsc treats this as a legacy-octal-style
            // leading zero and rejects the separator with TS6188. The
            // integer part of a decimal literal that starts with a `0`
            // followed by `_` and any continuation (digit OR another `_`)
            // is not a valid form. Emit the diagnostic at the `_` position
            // and fall through to `scan_decimal_number` so the rest of the
            // literal still produces the expected NumericLiteral token.
            //
            // The `_<digit>` arm catches `0_0`. The `_<_>` arm catches
            // `0__0` (regression: `parser.numericSeparators.decmialNegative.ts`
            // files 18/31/44, where a consecutive-separator run after the
            // leading zero ends in a digit; the inner loop emits TS6189 at
            // the second `_`, but the leading-zero TS6188 at the first `_`
            // was missing without this check).
            if next == CharacterCodes::UNDERSCORE
                && self
                    .char_code_at(self.pos + 2)
                    .is_some_and(|c| is_digit(c) || c == CharacterCodes::UNDERSCORE)
            {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: self.pos + 1,
                    length: 1,
                    args: Vec::new(),
                    message: diagnostic_messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                    code: diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                });
                self.token_flags |= TokenFlags::ContainsInvalidSeparator as u32;
                if self.token_invalid_separator_pos.is_none() {
                    self.token_invalid_separator_pos = Some(self.pos + 1);
                    self.token_invalid_separator_is_consecutive = false;
                }
            }
        }

        // Decimal number
        self.scan_decimal_number(start);
    }

    fn scan_prefixed_number(&mut self, start: usize, next: u32) -> bool {
        match next {
            CharacterCodes::LOWER_X | CharacterCodes::UPPER_X => {
                self.scan_integer_base_literal(start, is_hex_digit, TokenFlags::HexSpecifier);
                true
            }
            CharacterCodes::LOWER_B | CharacterCodes::UPPER_B => {
                self.scan_integer_base_literal(start, is_binary_digit, TokenFlags::BinarySpecifier);
                true
            }
            CharacterCodes::LOWER_O | CharacterCodes::UPPER_O => {
                self.scan_integer_base_literal(start, is_octal_digit, TokenFlags::OctalSpecifier);
                true
            }
            _ => false,
        }
    }

    fn scan_integer_base_literal(
        &mut self,
        start: usize,
        is_valid_digit: fn(u32) -> bool,
        specifier_flag: TokenFlags,
    ) {
        self.pos += 2;
        self.token_flags |= specifier_flag as u32;
        let saw_digit = self.scan_digits_with_separators(is_valid_digit);

        // tsc parity: if the prefixed integer literal has no valid digits
        // (e.g. `0xn`, `0bn`, `0on`, `0x`, `0x_`), emit a zero-width
        // "<base> digit expected" diagnostic at the position right after the
        // base prefix. Mirrors `scanner.ts`'s `if (!tokenValue) error(...)`
        // ladder in `scanIntegerBaseLiteral`/`case CharacterCodes._0`.
        if !saw_digit {
            let flag = specifier_flag as u32;
            let (message, code) = if flag == TokenFlags::HexSpecifier as u32 {
                (
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                )
            } else if flag == TokenFlags::BinarySpecifier as u32 {
                (
                    diagnostic_messages::BINARY_DIGIT_EXPECTED,
                    diagnostic_codes::BINARY_DIGIT_EXPECTED,
                )
            } else {
                (
                    diagnostic_messages::OCTAL_DIGIT_EXPECTED,
                    diagnostic_codes::OCTAL_DIGIT_EXPECTED,
                )
            };
            self.scanner_diagnostics.push(ScannerDiagnostic {
                pos: self.pos,
                length: 0,
                args: Vec::new(),
                message,
                code,
            });
        }

        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N {
            self.pos += 1;
            self.token_value = self.substring(start, self.pos);
            self.check_for_identifier_start_after_numeric_literal(
                start, /*is_scientific*/ false,
            );
            self.token = SyntaxKind::BigIntLiteral;
            return;
        }

        self.set_numeric_token_value(start);
        self.check_for_identifier_start_after_numeric_literal(start, /*is_scientific*/ false);
        self.token = SyntaxKind::NumericLiteral;
    }

    fn scan_legacy_octal_number(&mut self, start: usize) -> bool {
        let mut all_octal = true;
        let digit_start = self.pos + 1; // skip the leading 0
        let mut scan_pos = digit_start;
        while scan_pos < self.end && is_digit(self.char_code_unchecked(scan_pos)) {
            if !is_octal_digit(self.char_code_unchecked(scan_pos)) {
                all_octal = false;
            }
            scan_pos += 1;
        }
        if all_octal && scan_pos > digit_start {
            self.pos = scan_pos;
            self.token_flags |= TokenFlags::Octal as u32;
            self.set_numeric_token_value(start);
            self.token = SyntaxKind::NumericLiteral;
            true
        } else {
            self.token_flags |= TokenFlags::ContainsLeadingZero as u32;
            false
        }
    }

    fn set_numeric_token_value(&mut self, start: usize) {
        if (self.token_flags & TokenFlags::ContainsSeparator as u32) != 0 {
            self.token_value = self.substring(start, self.pos);
        } else {
            self.token_value.clear();
        }
    }

    fn scan_decimal_number(&mut self, start: usize) {
        self.scan_digits_with_separators(is_digit);
        let mut has_decimal_point = false;

        // Decimal point
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::DOT {
            has_decimal_point = true;
            self.pos += 1;
            self.scan_digits_with_separators(is_digit);
        }

        // Exponent
        let mut has_exponent = false;
        if self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::LOWER_E || ch == CharacterCodes::UPPER_E {
                has_exponent = true;
                self.pos += 1;
                self.token_flags |= TokenFlags::Scientific as u32;
                if self.pos < self.end {
                    let sign = self.char_code_unchecked(self.pos);
                    if sign == CharacterCodes::PLUS || sign == CharacterCodes::MINUS {
                        self.pos += 1;
                    }
                }
                // Mirror tsc's `scanNumber` exponent branch: if the exponent has
                // no digits (e.g. `1e`, `1e+`, `1ee`, `3en`), emit TS1124
                // "Digit expected" at the current position. Emitting here (before
                // `check_for_identifier_start_after_numeric_literal`) lets us
                // mirror tsc's `parseErrorAtPosition` same-start dedup: a later
                // TS1351 at the same position is suppressed by the helper below.
                let saw_exp_digit = self.scan_digits_with_separators(is_digit);
                if !saw_exp_digit {
                    self.scanner_diagnostics.push(ScannerDiagnostic {
                        pos: self.pos,
                        length: 0,
                        args: Vec::new(),
                        message: diagnostic_messages::DIGIT_EXPECTED,
                        code: diagnostic_codes::DIGIT_EXPECTED,
                    });
                }
            }
        }

        // BigInt suffix
        if !has_decimal_point
            && !has_exponent
            && self.pos < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N
        {
            self.pos += 1;
            self.token_value = self.substring(start, self.pos);
            self.check_for_identifier_start_after_numeric_literal(
                start, /*is_scientific*/ false,
            );
            self.token = SyntaxKind::BigIntLiteral;
            return;
        }

        // OPTIMIZATION: Only allocate token_value if number contains separators
        // Plain numbers (no underscores) can use source slice via get_token_value_ref()
        self.set_numeric_token_value(start);
        // When the recovery branch consumed an `n` BigInt suffix on a non-integer
        // (e.g. `3en`), `check_for_identifier_start_after_numeric_literal` returns
        // `true` and `self.pos` is now past the `n`. Re-capture `token_value` from
        // the full consumed span so emit preserves the source spelling, matching
        // tsc which prints `3en[null];` for `3en[null]` rather than `3e[null];`.
        if self.check_for_identifier_start_after_numeric_literal(start, has_exponent) {
            self.token_value = self.substring(start, self.pos);
        }
        self.token = SyntaxKind::NumericLiteral;
    }

    fn check_for_identifier_start_after_numeric_literal(
        &mut self,
        numeric_start: usize,
        is_scientific: bool,
    ) -> bool {
        if self.pos >= self.end {
            return false;
        }

        // Only check the raw character code, not unicode escapes.
        // TSC uses `codePointAt(text, pos)` here which reads the literal char,
        // so `\u005F` (backslash) is NOT treated as an identifier start.
        let starts_identifier = self.is_identifier_start(self.char_code_unchecked(self.pos));

        if !starts_identifier {
            return false;
        }

        let identifier_start = self.pos;
        self.scan_identifier_parts_after_numeric_literal();
        let identifier_end = self.pos;
        let identifier_text = &self.source[identifier_start..identifier_end];

        if identifier_text == "n" {
            self.scanner_diagnostics.push(ScannerDiagnostic {
                pos: numeric_start,
                length: identifier_end - numeric_start,
                args: Vec::new(),
                message: if is_scientific {
                    diagnostic_messages::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                } else {
                    diagnostic_messages::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                },
                code: if is_scientific {
                    diagnostic_codes::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                } else {
                    diagnostic_codes::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                },
            });
            true
        } else {
            // Mirror tsc's `parseErrorAtPosition` same-start dedup: if a prior
            // scanner diagnostic was already pushed at `identifier_start`
            // (e.g. TS1124 "Digit expected" emitted by the empty-exponent
            // branch in `scan_decimal_number` for `1ee`/`123ee`), tsc's parser
            // would suppress this TS1351 because its `lastError.start` matches.
            // We mirror that suppression here so the merged diagnostics match
            // tsc fingerprint-for-fingerprint.
            let already_diag_at_pos = self
                .scanner_diagnostics
                .last()
                .is_some_and(|d| d.pos == identifier_start);
            if !already_diag_at_pos {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: identifier_start,
                    length: identifier_end - identifier_start,
                    message: diagnostic_messages::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                    code: diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                    args: Vec::new(),
                });
            }
            self.pos = identifier_start;
            false
        }
    }

    fn scan_identifier_parts_after_numeric_literal(&mut self) {
        if self.pos >= self.end {
            return;
        }

        if self.char_code_unchecked(self.pos) == CharacterCodes::BACKSLASH {
            if let Some(code_point) = self.peek_unicode_escape() {
                if self.is_identifier_start(code_point) {
                    let _ = self.scan_unicode_escape_value();
                } else {
                    return;
                }
            } else {
                return;
            }
        } else if self.is_identifier_start(self.char_code_unchecked(self.pos)) {
            self.pos += self.char_len_at(self.pos);
        } else {
            return;
        }

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                if let Some(code_point) = self.peek_unicode_escape()
                    && self.is_identifier_part(code_point)
                {
                    let _ = self.scan_unicode_escape_value();
                    continue;
                }
                break;
            }
            if !self.is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos);
        }
    }

    /// Returns `true` if at least one valid digit was consumed. Mirrors tsc's
    /// `scanHexDigits`/`scanBinaryOrOctalDigits` "did we read anything" signal,
    /// so callers like `scan_integer_base_literal` can emit
    /// `Hexadecimal/Binary/Octal digit expected` for empty prefixed literals.
    /// Also emits one diagnostic per invalid numeric separator as we walk the
    /// digit run, distinguishing TS6189 ("multiple consecutive") from TS6188
    /// ("not allowed here") by tracking whether the immediately preceding
    /// token was a *valid* separator.
    fn scan_digits_with_separators(&mut self, is_valid_digit: fn(u32) -> bool) -> bool {
        let scan_start = self.pos;
        let mut saw_digit = false;
        let mut allow_separator = false;
        let mut is_previous_separator = false;

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::UNDERSCORE {
                self.token_flags |= TokenFlags::ContainsSeparator as u32;
                if allow_separator {
                    allow_separator = false;
                    is_previous_separator = true;
                } else if is_previous_separator {
                    self.push_invalid_numeric_separator(self.pos, /*consecutive*/ true);
                } else {
                    self.push_invalid_numeric_separator(self.pos, /*consecutive*/ false);
                }
                self.pos += 1;
                continue;
            }
            if is_valid_digit(ch) {
                saw_digit = true;
                allow_separator = true;
                is_previous_separator = false;
                self.pos += 1;
                continue;
            }
            break;
        }

        // Trailing underscore — tsc fires Numeric_separators_are_not_allowed_here
        // unconditionally if the byte before pos is `_`. The scanner-level
        // dedup in `push_invalid_numeric_separator` collapses the same-position
        // pair (TS6189 + TS6188) that arises when the inner loop already emitted
        // for that same byte. Bound the look-back at `scan_start` so a zero-iter
        // call (e.g., a `.5` decimal where the first byte is `.`) cannot inspect
        // a byte from a preceding token like the `_` in `_.5`.
        if self.pos > scan_start
            && self.char_code_unchecked(self.pos - 1) == CharacterCodes::UNDERSCORE
        {
            self.push_invalid_numeric_separator(self.pos - 1, /*consecutive*/ false);
        }

        saw_digit
    }

    /// Push a numeric-separator diagnostic, applying the same "skip if last
    /// diagnostic shares this start position" rule that tsc's
    /// `parseErrorAtPosition` enforces (only the first of a same-start run
    /// survives). Also records the first invalid separator position for the
    /// parser's recovery path (e.g. TS2304 emission for `0_X0101`).
    fn push_invalid_numeric_separator(&mut self, pos: usize, consecutive: bool) {
        self.token_flags |= TokenFlags::ContainsInvalidSeparator as u32;
        if self.token_invalid_separator_pos.is_none() {
            self.token_invalid_separator_pos = Some(pos);
            self.token_invalid_separator_is_consecutive = consecutive;
        }
        if let Some(last) = self.scanner_diagnostics.last()
            && last.pos == pos
        {
            return;
        }
        let (message, code) = if consecutive {
            (
                diagnostic_messages::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
                diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
            )
        } else {
            (
                diagnostic_messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
            )
        };
        self.scanner_diagnostics.push(ScannerDiagnostic {
            pos,
            length: 1,
            message,
            code,
            args: Vec::new(),
        });
    }

    pub(crate) fn push_invalid_character(&mut self, pos: usize) {
        if let Some(last) = self.scanner_diagnostics.last()
            && last.pos == pos
        {
            return;
        }
        self.scanner_diagnostics.push(ScannerDiagnostic {
            pos,
            length: 1,
            message: diagnostic_messages::INVALID_CHARACTER,
            code: diagnostic_codes::INVALID_CHARACTER,
            args: Vec::new(),
        });
    }
}
