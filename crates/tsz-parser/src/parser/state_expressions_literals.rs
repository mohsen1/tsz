//! Parser state - literal, binding pattern, and compound expression parsing.

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_DISALLOW_IN, CONTEXT_FLAG_GENERATOR,
    CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION, CONTEXT_FLAG_STATIC_BLOCK, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        AccessExprData, CallExprData, IdentifierData, LiteralData, LiteralExprData,
        ParenthesizedData, TaggedTemplateData, TemplateExprData, TemplateSpanData,
    },
    syntax_kind_ext,
};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    /// Parse object binding pattern: { x, y: z, ...rest }
    pub(crate) fn parse_object_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            if dot_dot_dot {
                // Rest element: parse name (may be property_name if followed by `:`)
                let first_name = self.parse_binding_element_name();
                if first_name.is_none() {
                    // Emit TS1109 for missing rest binding element: {...missing}
                    self.error_expression_expected();
                }

                // Handle `...propertyName: name` — invalid but parsed for error
                // recovery. The checker will emit TS2566.
                let (property_name, name) = if self.parse_optional(SyntaxKind::ColonToken) {
                    let actual_name = self.parse_binding_element_name();
                    if actual_name.is_none() {
                        self.error_expression_expected();
                    }
                    (first_name, actual_name)
                } else {
                    (NodeIndex::NONE, first_name)
                };

                // Check for illegal initializer: {...x = value} - emit TS1186
                if self.is_token(SyntaxKind::EqualsToken) {
                    self.parse_error_at_current_token(
                        "A rest element cannot have an initializer.",
                        1186,
                    );
                    // Consume the = token and value to continue parsing
                    self.next_token();
                    self.parse_assignment_expression();
                }

                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::node::BindingElementData {
                        dot_dot_dot_token: true,
                        property_name,
                        name,
                        initializer: NodeIndex::NONE,
                    },
                ));
            } else {
                // Regular binding element: name or propertyName: name
                let first_token = self.token();
                let first_token_is_identifier_or_keyword =
                    first_token == SyntaxKind::Identifier || self.is_identifier_or_keyword();
                let first_token_is_reserved = self.is_reserved_word();
                let first_name_start = self.token_pos();
                let first_name_end = self.token_end();
                let first_name = self.parse_property_name();

                let (property_name, name) = if self.parse_optional(SyntaxKind::ColonToken) {
                    // propertyName: name
                    let name_is_reserved = self.is_reserved_word();
                    let name = self.parse_binding_element_name();
                    if name.is_none() {
                        // Emit TS1109 for missing property binding element: {prop: missing}
                        self.error_expression_expected();
                    }
                    if name_is_reserved {
                        // Emit TS1005 at current position (after the reserved word)
                        // to avoid suppression from TS1359 emitted at the same position
                        self.parse_error_at_current_token(
                            "':' expected.",
                            tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        );
                    }
                    (first_name, name)
                } else {
                    // Just name (shorthand)
                    if !first_token_is_identifier_or_keyword || first_token_is_reserved {
                        // Reserved words (while, for, if, etc.) can be property names
                        // but cannot be used in shorthand form — require ':'
                        // Report at current token position (where ':' should appear),
                        // matching tsc's behavior.
                        self.parse_error_at_current_token(
                            "':' expected.",
                            tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        );
                    }
                    // Check for contextually reserved identifiers in shorthand binding.
                    // e.g., `let { await } = x` in a static block or async function.
                    // The property name was already parsed, so check at its position.
                    if (first_token == SyntaxKind::AwaitKeyword
                        && (self.in_async_context() || self.in_static_block_context()))
                        || (first_token == SyntaxKind::YieldKeyword && self.in_generator_context())
                    {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at(
                            first_name_start,
                            first_name_end.saturating_sub(first_name_start),
                            "Identifier expected. 'await' is a reserved word that cannot be used here.",
                            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                        );
                    }
                    (NodeIndex::NONE, first_name)
                };

                // Optional initializer: = value
                // Per spec, BindingElement initializers always use [+In],
                // so 'in' is allowed even inside for-statement headers.
                let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                    let saved = self.context_flags;
                    self.context_flags &= !CONTEXT_FLAG_DISALLOW_IN;
                    let init = self.parse_assignment_expression();
                    self.context_flags = saved;
                    if init.is_none() {
                        // Emit TS1109 for missing object binding initializer: {x = missing}
                        self.error_expression_expected();
                    }
                    init
                } else {
                    NodeIndex::NONE
                };

                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::node::BindingElementData {
                        dot_dot_dot_token: false,
                        property_name,
                        name,
                        initializer,
                    },
                ));
            }

            let has_comma = self.parse_optional(SyntaxKind::CommaToken);
            if dot_dot_dot
                && has_comma
                && (self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::CloseBracketToken)
                    || self.is_token(SyntaxKind::EndOfFileToken))
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                        self.token_pos() - 1, // approximate comma position
                        1,
                        "A rest parameter or binding pattern may not have a trailing comma.",
                        diagnostic_codes::A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA,
                    );
            }

            if !has_comma {
                if self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::EndOfFileToken)
                {
                    break;
                }
                // Missing comma - emit error and continue parsing for recovery
                self.parse_expected(SyntaxKind::CommaToken);
            }
        }

        let end_pos = if self.is_token(SyntaxKind::CloseBraceToken) {
            let end = self.token_end();
            self.parse_expected(SyntaxKind::CloseBraceToken);
            end
        } else {
            // Recover by advancing until we see a closing brace or EOF to avoid infinite loops.
            self.parse_expected(SyntaxKind::CloseBraceToken);
            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.next_token();
            }
            if self.is_token(SyntaxKind::CloseBraceToken) {
                let end = self.token_end();
                self.next_token();
                end
            } else {
                self.token_end()
            }
        };

        self.arena.add_binding_pattern(
            syntax_kind_ext::OBJECT_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse array binding pattern: [x, y, ...rest]
    pub(crate) fn parse_array_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle omitted element: [, , x]
            if self.is_token(SyntaxKind::CommaToken) {
                // Omitted element - push NONE as placeholder
                elements.push(NodeIndex::NONE);
                self.next_token();
                continue;
            }

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse name (can be identifier or nested binding pattern)
            let name = self.parse_binding_element_name();
            if name.is_none() {
                // Emit TS1109 for missing binding element: [...missing] or [missing]
                self.error_expression_expected();
            }

            // Optional initializer: = value
            // Per spec, BindingElement initializers always use [+In],
            // so 'in' is allowed even inside for-statement headers.
            let initializer = if !dot_dot_dot && self.parse_optional(SyntaxKind::EqualsToken) {
                let saved = self.context_flags;
                self.context_flags &= !CONTEXT_FLAG_DISALLOW_IN;
                let init = self.parse_assignment_expression();
                self.context_flags = saved;
                if init.is_none() {
                    // Emit TS1109 for missing binding initializer: [x = missing]
                    self.error_expression_expected();
                }
                init
            } else if dot_dot_dot && self.is_token(SyntaxKind::EqualsToken) {
                // Rest element with initializer: [...x = value] - emit TS1186
                self.parse_error_at_current_token(
                    "A rest element cannot have an initializer.",
                    1186,
                );
                // Consume the = token and value to continue parsing
                self.next_token();
                self.parse_assignment_expression();
                NodeIndex::NONE
            } else {
                NodeIndex::NONE
            };

            let elem_end = self.token_end();
            elements.push(self.arena.add_binding_element(
                syntax_kind_ext::BINDING_ELEMENT,
                elem_start,
                elem_end,
                crate::parser::node::BindingElementData {
                    dot_dot_dot_token: dot_dot_dot,
                    property_name: NodeIndex::NONE,
                    name,
                    initializer,
                },
            ));

            let has_comma = self.parse_optional(SyntaxKind::CommaToken);
            if dot_dot_dot
                && has_comma
                && (self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::CloseBracketToken)
                    || self.is_token(SyntaxKind::EndOfFileToken))
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                        self.token_pos() - 1, // approximate comma position
                        1,
                        "A rest parameter or binding pattern may not have a trailing comma.",
                        diagnostic_codes::A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA,
                    );
            }

            if !has_comma {
                break;
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_binding_pattern(
            syntax_kind_ext::ARRAY_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse binding element name (can be identifier or nested binding pattern)
    pub(crate) fn parse_binding_element_name(&mut self) -> NodeIndex {
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else {
            self.parse_identifier()
        }
    }

    /// Parse numeric literal
    /// Uses zero-copy accessor for parsing, clones only when storing
    pub(crate) fn parse_numeric_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        let token_flags = self.scanner.get_token_flags();

        // Check for numbers with leading zeros that should emit TS1121 or TS1489
        // This includes:
        // - TS1121: Legacy octal (01, 0777) - Octal flag set, all digits 0-7
        // - TS1489: Decimal with leading zero (08, 009, 08.5) - starts with 0, contains 8/9
        //
        // The scanner sets Octal flag only when first digit after 0 is 0-7.
        // So "08" doesn't have Octal flag (8 is not octal), but should still emit TS1489.
        // We need to check both cases.
        let bytes = text.as_bytes();
        let is_leading_zero_number = bytes.len() > 1
            && bytes[0] == b'0'
            && bytes[1].is_ascii_digit()
            && (token_flags
                & (TokenFlags::HexSpecifier as u32
                    | TokenFlags::BinarySpecifier as u32
                    | TokenFlags::OctalSpecifier as u32))
                == 0;

        if is_leading_zero_number {
            // Find the integer part (before any decimal point or exponent)
            let integer_part = text.split(['.', 'e', 'E']).next().unwrap_or(&text);
            // Check if any digit after the leading 0 is 8 or 9
            let has_non_octal =
                integer_part.len() > 1 && integer_part[1..].bytes().any(|b| b == b'8' || b == b'9');
            if has_non_octal {
                // TS1489: Decimals with leading zeros are not allowed (e.g., 08, 009, 08.5)
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    end_pos - start_pos,
                    "Decimals with leading zeros are not allowed.",
                    diagnostic_codes::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED,
                );
            } else if integer_part.len() > 1 {
                // TS1121: Legacy octal literal (e.g., 01, 0777)
                use tsz_common::diagnostics::diagnostic_codes;
                // Convert legacy octal to modern octal for the suggestion (e.g., "01" -> "0o1")
                // Parse the octal value and format without leading zeros (tsc behavior)
                let octal_digits = &integer_part[1..];
                let octal_value = octal_digits
                    .bytes()
                    .filter(|&b| b != b'_')
                    .fold(0u64, |acc, b| acc * 8 + (b - b'0') as u64);
                // tsc's scanner checks if the previous token was MinusToken and includes
                // the `-` prefix in both the error span and the suggestion.
                // e.g., `-01` → error at `-`, suggestion `'-0o1'`
                let source = self.scanner.source_text();
                let with_minus =
                    start_pos > 0 && source.as_bytes().get(start_pos as usize - 1) == Some(&b'-');
                let minus_prefix = if with_minus { "-" } else { "" };
                let suggested = format!("{minus_prefix}0o{octal_value:o}");
                let err_start = if with_minus { start_pos - 1 } else { start_pos };
                let message =
                    format!("Octal literals are not allowed. Use the syntax '{suggested}'.");
                self.parse_error_at(
                    err_start,
                    end_pos - err_start,
                    &message,
                    diagnostic_codes::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
                );
            }
        }

        // Check for missing exponent digits (e.g., 1e+, 1e-, 1e) - TS1124
        // If there's a Scientific flag but the text ends without valid digits after e/E
        if (token_flags & TokenFlags::Scientific as u32) != 0 {
            let bytes = text.as_bytes();
            let len = bytes.len();
            let missing_digit = if let Some(exp_offset) = bytes
                .iter()
                .rposition(|byte| *byte == b'e' || *byte == b'E')
            {
                if exp_offset + 1 >= len {
                    true
                } else {
                    let after = &bytes[exp_offset + 1..];
                    let first = after[0];
                    if first == b'+' || first == b'-' {
                        after.len() == 1 || after[1] == b'_'
                    } else {
                        first == b'_'
                    }
                }
            } else {
                false
            };
            if missing_digit {
                use tsz_common::diagnostics::diagnostic_codes;
                // Find position of the missing digit (at the end)
                self.parse_error_at(
                    end_pos,
                    0,
                    "Digit expected.",
                    diagnostic_codes::DIGIT_EXPECTED,
                );
            }
        }

        // Check for missing hex digits (e.g., 0x, 0X) - TS1125
        if (token_flags & TokenFlags::HexSpecifier as u32) != 0 {
            // Hex literal should be at least 3 chars (0x followed by at least one digit)
            // If it's just "0x" or "0X", no hex digits were provided
            if text.len() == 2 {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    end_pos,
                    0,
                    "Hexadecimal digit expected.",
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            }
        }

        // Check for missing binary digits (e.g., 0b, 0B) - TS1177
        if (token_flags & TokenFlags::BinarySpecifier as u32) != 0 {
            // Binary literal should be at least 3 chars (0b followed by at least one digit)
            // If it's just "0b" or "0B", no binary digits were provided
            if text.len() == 2 {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    end_pos,
                    0,
                    "Binary digit expected.",
                    diagnostic_codes::BINARY_DIGIT_EXPECTED,
                );
            }
        }

        // Check for missing octal digits (e.g., 0o, 0O) - TS1178
        if (token_flags & TokenFlags::OctalSpecifier as u32) != 0 {
            // Octal literal should be at least 3 chars (0o followed by at least one digit)
            // If it's just "0o" or "0O", no octal digits were provided
            if text.len() == 2 {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    end_pos,
                    0,
                    "Octal digit expected.",
                    diagnostic_codes::OCTAL_DIGIT_EXPECTED,
                );
            }
        }

        // Check if this numeric literal has an invalid separator (for TS1351 check)
        let has_invalid_separator =
            (token_flags & TokenFlags::ContainsInvalidSeparator as u32) != 0;

        self.report_invalid_numeric_separator();
        let invalid_separator_pos = if has_invalid_separator {
            self.scanner.get_invalid_separator_pos()
        } else {
            None
        };
        let value = tsz_common::numeric::parse_numeric_literal_value(&text);
        self.next_token();

        // TS1351: If a numeric literal has an invalid separator and is immediately
        // followed by an identifier or keyword, report "identifier cannot follow numeric literal"
        // Keep the following identifier as a recoverable token; checker may emit TS2304 if needed.
        if has_invalid_separator && self.is_identifier_or_keyword() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An identifier or keyword cannot immediately follow a numeric literal.",
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            );
        }

        // If the scanner stopped numeric scanning at an invalid separator and left a recoverable
        // identifier immediately after it (for example `0_b`), emit the missing-name diagnostic
        // expected by TS (TS2304) on that identifier.
        if let Some(sep_pos) = invalid_separator_pos
            && self.is_identifier_or_keyword()
            && self.token_pos() as usize == sep_pos + 1
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let missing_name = self.scanner.get_token_text_ref().to_string();
            if !missing_name.is_empty() {
                self.parse_error_at(
                    self.token_pos(),
                    self.token_end() - self.token_pos(),
                    &format!("Cannot find name '{missing_name}'."),
                    diagnostic_codes::CANNOT_FIND_NAME,
                );
            }
        }

        self.arena.add_literal(
            SyntaxKind::NumericLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value,
            },
        )
    }

    /// Parse bigint literal
    /// Uses zero-copy accessor, stores the raw text (e.g. "123n")
    pub(crate) fn parse_bigint_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.report_invalid_numeric_separator();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::BigIntLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    pub(crate) fn report_invalid_numeric_separator(&mut self) {
        if (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidSeparator as u32) == 0 {
            return;
        }

        let (message, code) = if self.scanner.invalid_separator_is_consecutive() {
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

        if let Some(pos) = self.scanner.get_invalid_separator_pos() {
            self.parse_error_at(self.u32_from_usize(pos), 1, message, code);
        } else {
            self.parse_error_at_current_token(message, code);
        }
    }

    /// Parse boolean literal
    pub(crate) fn parse_boolean_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let kind = self.token();
        self.next_token();

        self.arena.add_token(kind as u16, start_pos, end_pos)
    }

    /// Parse null literal
    pub(crate) fn parse_null_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::NullKeyword as u16, start_pos, end_pos)
    }

    /// Parse this expression
    pub(crate) fn parse_this_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
    }

    /// Parse super expression
    ///
    /// Matches tsc's `parseSuperExpression`: if `super` is not followed by `(`, `.`,
    /// `[`, or `<`, emit TS1034 at the current token position (matching tsc's
    /// parseExpectedToken behavior where the error is at the token after `super`).
    pub(crate) fn parse_super_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        // If super is followed by (, ., [, or <, return just the super keyword.
        // The caller (parse_member_expression_rest) will handle the access chain.
        if !self.is_token(SyntaxKind::OpenParenToken)
            && !self.is_token(SyntaxKind::DotToken)
            && !self.is_token(SyntaxKind::OpenBracketToken)
            && !self.is_token(SyntaxKind::LessThanToken)
        {
            // super must be followed by an argument list or member access.
            // Emit TS1034 at the current token position (matching tsc's parseExpectedToken).
            self.parse_error_at_current_token(
                diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
            );
        }

        self.arena
            .add_token(SyntaxKind::SuperKeyword as u16, start_pos, end_pos)
    }

    /// Parse regex literal: /pattern/flags
    pub(crate) fn parse_regex_literal(&mut self) -> NodeIndex {
        fn regex_body_end(raw_text: &str) -> Option<usize> {
            let bytes = raw_text.as_bytes();
            if bytes.first().copied() != Some(b'/') {
                return None;
            }

            let mut i = 1usize;
            let mut escaped = false;
            let mut in_character_class = false;
            while i < bytes.len() {
                let ch = bytes[i];
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                match ch {
                    b'\\' => {
                        escaped = true;
                        i += 1;
                    }
                    b'[' => {
                        in_character_class = true;
                        i += 1;
                    }
                    b']' => {
                        in_character_class = false;
                        i += 1;
                    }
                    b'/' if !in_character_class => return Some(i),
                    _ => i += 1,
                }
            }
            None
        }

        fn decode_surrogate_pair(high: u32, low: u32) -> Option<u32> {
            if !(0xD800..=0xDBFF).contains(&high) || !(0xDC00..=0xDFFF).contains(&low) {
                return None;
            }
            Some(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
        }

        fn parse_hex_u32(raw_text: &str, start: usize, len: usize) -> Option<u32> {
            raw_text
                .get(start..start + len)
                .and_then(|slice| u32::from_str_radix(slice, 16).ok())
        }

        fn split_non_unicode_atom_offsets(start: usize, ch: char) -> Vec<u32> {
            let utf16_len = ch.len_utf16();
            let utf8_len = ch.len_utf8();
            ch.encode_utf16(&mut [0; 2])
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    u32::try_from(start + (i * utf8_len) / utf16_len)
                        .expect("regex offsets must fit in u32")
                })
                .collect()
        }

        fn regex_range_order_errors(raw_text: &str, body_end: usize) -> Vec<(u32, u32)> {
            #[derive(Clone, Copy)]
            enum ClassToken {
                Atom { value: u32, start: u32 },
                OpaqueAtom,
                Hyphen,
            }

            fn parse_class_atom(
                raw_text: &str,
                start: usize,
                class_end: usize,
                unicode_mode: bool,
            ) -> Option<(Vec<(u32, u32)>, usize)> {
                let rest = raw_text.get(start..class_end)?;
                let mut chars = rest.chars();
                let ch = chars.next()?;
                if ch == '\\' {
                    let next_start = start + ch.len_utf8();
                    let next = raw_text.get(next_start..class_end)?.chars().next()?;
                    if next == 'u' {
                        let brace_start = next_start + next.len_utf8();
                        if raw_text.as_bytes().get(brace_start).copied() == Some(b'{') {
                            let hex_start = brace_start + 1;
                            let mut hex_end = hex_start;
                            while hex_end < class_end
                                && raw_text.as_bytes().get(hex_end).copied() != Some(b'}')
                            {
                                hex_end += 1;
                            }
                            if hex_end < class_end
                                && let Some(value) =
                                    parse_hex_u32(raw_text, hex_start, hex_end - hex_start)
                            {
                                return Some((
                                    vec![(
                                        value,
                                        u32::try_from(start).expect("regex offsets must fit in u32"),
                                    )],
                                    hex_end + 1,
                                ));
                            }
                        } else if let Some(value) = parse_hex_u32(raw_text, brace_start, 4) {
                            let next_index = brace_start + 4;
                            if unicode_mode
                                && let Some(after_first) = raw_text.get(next_index..class_end)
                                && after_first.starts_with("\\u")
                                && let Some(low) = parse_hex_u32(raw_text, next_index + 2, 4)
                                && let Some(code_point) = decode_surrogate_pair(value, low)
                            {
                                return Some((
                                    vec![(
                                        code_point,
                                        u32::try_from(start).expect("regex offsets must fit in u32"),
                                    )],
                                    next_index + 6,
                                ));
                            }
                            return Some((
                                vec![(
                                    value,
                                    u32::try_from(start).expect("regex offsets must fit in u32"),
                                )],
                                next_index,
                            ));
                        }
                    }

                    let escaped_start = next_start;
                    let escaped = raw_text.get(escaped_start..class_end)?.chars().next()?;
                    if matches!(escaped, 'd' | 'D' | 's' | 'S' | 'w' | 'W') {
                        return Some((Vec::new(), escaped_start + escaped.len_utf8()));
                    }
                    if unicode_mode {
                        Some((
                            vec![(
                                escaped as u32,
                                u32::try_from(start).expect("regex offsets must fit in u32"),
                            )],
                            escaped_start + escaped.len_utf8(),
                        ))
                    } else {
                        Some((
                            escaped
                                .encode_utf16(&mut [0; 2])
                                .iter()
                                .zip(split_non_unicode_atom_offsets(start, escaped))
                                .map(|(u, offset)| (*u as u32, offset))
                                .collect(),
                            escaped_start + escaped.len_utf8(),
                        ))
                    }
                } else if unicode_mode {
                    Some((
                        vec![(
                            ch as u32,
                            u32::try_from(start).expect("regex offsets must fit in u32"),
                        )],
                        start + ch.len_utf8(),
                    ))
                } else {
                    Some((
                        ch.encode_utf16(&mut [0; 2])
                            .iter()
                            .zip(split_non_unicode_atom_offsets(start, ch))
                            .map(|(u, offset)| (*u as u32, offset))
                            .collect(),
                        start + ch.len_utf8(),
                    ))
                }
            }

            let flags = &raw_text[body_end + 1..];
            let unicode_mode = flags.contains('u') || flags.contains('v');
            let bytes = raw_text.as_bytes();
            let mut errors = Vec::new();
            let mut i = 1usize;

            while i < body_end {
                match bytes[i] {
                    b'\\' => {
                        i += 1;
                        if i < body_end {
                            i += 1;
                        }
                    }
                    b'[' => {
                        i += 1;
                        let mut tokens = Vec::new();
                        while i < body_end {
                            if bytes[i] == b']' {
                                i += 1;
                                break;
                            }
                            if bytes[i] == b'-' {
                                tokens.push(ClassToken::Hyphen);
                                i += 1;
                                continue;
                            }
                            let Some((atoms, next_i)) =
                                parse_class_atom(raw_text, i, body_end, unicode_mode)
                            else {
                                break;
                            };
                            if atoms.is_empty() {
                                tokens.push(ClassToken::OpaqueAtom);
                            } else {
                                tokens.extend(
                                    atoms.into_iter()
                                        .map(|(value, start)| ClassToken::Atom { value, start }),
                                );
                            }
                            i = next_i;
                        }

                        if let Some(offending_start) = tokens.windows(3).find_map(|window| {
                            match window {
                                [
                                    ClassToken::Atom { value: left, start },
                                    ClassToken::Hyphen,
                                    ClassToken::Atom { value: right, .. },
                                ] if left > right => Some(*start),
                                _ => None,
                            }
                        }) {
                            errors.push((offending_start, 1));
                        }
                    }
                    _ => {
                        if let Some(ch) = raw_text.get(i..body_end).and_then(|s| s.chars().next()) {
                            i += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }

            errors
        }

        let start_pos = self.token_pos();

        // Rescan the / or /= as a regex literal
        self.scanner.re_scan_slash_token();
        self.current_token = self.scanner.get_token();

        // Check for unterminated regex literal (TS1161)
        if (self.scanner.get_token_flags() & TokenFlags::Unterminated as u32) != 0 {
            // Suppress TS1161 when the unterminated "regex" body looks like a JSX
            // closing tag artifact (e.g., `</a:b>` parsed outside JSX context where
            // `/` is misinterpreted as a regex start). These bodies contain `>`
            // before any semicolon, which never occurs in a real regex literal at
            // statement level (the scanner stops at newlines, so `>` would only
            // appear if the `/` was actually part of a `</tag>` construct).
            let regex_body = self.scanner.get_token_text_ref();
            let is_jsx_artifact = regex_body.find('>').is_some_and(|gt_pos| {
                regex_body
                    .find(';')
                    .is_none_or(|semi_pos| gt_pos < semi_pos)
            });
            if !is_jsx_artifact {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    1,
                    "Unterminated regular expression literal.",
                    diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL,
                );
            }
        }

        // Get the regex text (including slashes and flags)
        let text = self.scanner.get_token_value_ref().to_string();
        let raw_text = self.scanner.get_token_text_ref().to_string();

        // Capture regex flag errors BEFORE calling parse_expected (which clears them via next_token)
        let flag_errors: Vec<_> = self.scanner.get_regex_flag_errors().to_vec();
        // NOTE: tsc does not emit TS1125 for invalid unicode escapes inside regex literals.
        // Removed call to report_invalid_regular_expression_escape_errors() to match tsc behavior.
        let extended_unicode_escape_errors = regex_body_end(&raw_text)
            .filter(|body_end| {
                let flags = &raw_text[*body_end + 1..];
                !flags.contains('u') && !flags.contains('v')
            })
            .map(|body_end| {
                let bytes = raw_text.as_bytes();
                let mut errors = Vec::new();
                let mut i = 1usize;
                while i + 2 < body_end {
                    if bytes[i] == b'\\' && bytes[i + 1] == b'u' && bytes[i + 2] == b'{' {
                        let mut j = i + 3;
                        while j < body_end && bytes[j] != b'}' {
                            j += 1;
                        }
                        if j < body_end {
                            errors.push((start_pos + i as u32, (j + 1 - i) as u32));
                            i = j + 1;
                            continue;
                        }
                    }
                    i += 1;
                }
                errors
            })
            .unwrap_or_default();
        let range_order_errors = regex_body_end(&raw_text)
            .map(|body_end| regex_range_order_errors(&raw_text, body_end))
            .unwrap_or_default();

        self.parse_expected(SyntaxKind::RegularExpressionLiteral);
        let end_pos = self.token_end();

        if let Some(missing) = self.missing_regex_closing_token(&text) {
            let message = if missing == b']' {
                "']' expected."
            } else {
                "')' expected."
            };
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(end_pos, 1, message, diagnostic_codes::EXPECTED);
        }

        // Emit errors for all regex flag issues detected by scanner
        for error in flag_errors {
            let (message, code) = match error.kind {
                tsz_scanner::scanner_impl::RegexFlagErrorKind::Duplicate => {
                    ("Duplicate regular expression flag.", 1500)
                }
                tsz_scanner::scanner_impl::RegexFlagErrorKind::InvalidFlag => {
                    ("Unknown regular expression flag.", 1499)
                }
                tsz_scanner::scanner_impl::RegexFlagErrorKind::IncompatibleFlags => (
                    "The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously.",
                    1502,
                ),
            };
            self.parse_error_at(self.u32_from_usize(error.pos), 1, message, code);
        }
        for (pos, len) in extended_unicode_escape_errors {
            self.parse_error_at(
                pos,
                len,
                tsz_common::diagnostics::diagnostic_messages::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO,
                tsz_common::diagnostics::diagnostic_codes::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO,
            );
        }
        for (pos, len) in range_order_errors {
            self.parse_error_at(
                start_pos + pos,
                len,
                tsz_common::diagnostics::diagnostic_messages::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
                tsz_common::diagnostics::diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
            );
        }

        self.arena.add_literal(
            SyntaxKind::RegularExpressionLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: Some(raw_text),
                value: None,
            },
        )
    }

    fn missing_regex_closing_token(&self, text: &str) -> Option<u8> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'/' {
            return None;
        }

        // Mirror the regex scan state for body extraction.
        let mut in_escape = false;
        let mut in_character_class = false;
        let mut body_end = bytes.len();

        for (i, ch) in bytes.iter().enumerate().skip(1) {
            let ch = *ch;
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
            } else if ch == b'[' && !in_character_class {
                in_character_class = true;
            } else if ch == b']' && in_character_class {
                in_character_class = false;
            } else if ch == b'/' && !in_character_class {
                body_end = i;
                break;
            }
        }

        if body_end <= 1 {
            return None;
        }

        let mut missing = None;
        let mut paren_depth = 0i32;
        in_escape = false;
        in_character_class = false;
        for &ch in &bytes[1..body_end] {
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
                continue;
            }
            if in_character_class {
                if ch == b']' {
                    in_character_class = false;
                }
                continue;
            }
            match ch {
                b'[' => in_character_class = true,
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                }
                _ => {}
            }
        }

        if in_character_class {
            missing = Some(b']');
        }
        if missing.is_none() && paren_depth > 0 {
            missing = Some(b')');
        }

        missing
    }

    /// Parse import expression: import(...), import.meta, or import.defer(...)
    pub(crate) fn parse_import_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import.meta / import.defer(...)
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            // Create import keyword node
            let import_node =
                self.arena
                    .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
            // Check if identifier after '.' is 'meta' or 'defer'
            let prop_name = if self.is_identifier_or_keyword() {
                self.scanner.get_token_value_ref().to_string()
            } else {
                String::new()
            };
            let is_valid = prop_name == "meta" || prop_name == "defer";
            let name_start = self.token_pos();
            let name = self.parse_identifier_name();
            let name_end = self.token_end();
            if prop_name == "defer" && !self.is_token(SyntaxKind::OpenParenToken) {
                // import.defer without '(' — TS1005 "'(' expected."
                // Unlike import.meta, import.defer is only valid as a call expression.
                self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            } else if !is_valid && !prop_name.is_empty() {
                // import.X where X is neither 'meta' nor 'defer'
                // If followed by '(' → TS18061 (suggest 'meta' or 'defer')
                // Otherwise → TS17012 (suggest only 'meta')
                if self.is_token(SyntaxKind::OpenParenToken) {
                    let msg = format_message(
                        diagnostic_messages::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_IMPORT_DID_YOU_MEAN_META_OR_DEFER,
                        &[&prop_name],
                    );
                    self.parse_error_at(
                        name_start,
                        name_end.saturating_sub(name_start),
                        &msg,
                        diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_IMPORT_DID_YOU_MEAN_META_OR_DEFER,
                    );
                } else {
                    let msg = format_message(
                        diagnostic_messages::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN,
                        &[&prop_name, "import", "meta"],
                    );
                    self.parse_error_at(
                        name_start,
                        name_end.saturating_sub(name_start),
                        &msg,
                        diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN,
                    );
                }
            }
            let end_pos = self.token_end();

            return self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: import_node,
                    question_dot_token: false,
                    name_or_argument: name,
                },
            );
        }

        // Check for invalid import forms before expecting '('
        if self.is_token(SyntaxKind::LessThanToken) {
            // import<T>(...) — type arguments not allowed on import calls (TS1326)
            self.parse_error_at(
                start_pos,
                6, // length of "import"
                diagnostic_messages::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR,
                diagnostic_codes::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR,
            );
            // Skip the type arguments for recovery, handling nested <...>
            self.next_token(); // consume '<'
            let mut depth: u32 = 1;
            while depth > 0 {
                if self.is_token(SyntaxKind::LessThanToken) {
                    depth += 1;
                } else if self.is_token(SyntaxKind::GreaterThanToken) {
                    depth -= 1;
                    if depth == 0 {
                        self.next_token(); // consume final '>'
                        break;
                    }
                } else if self.is_token(SyntaxKind::GreaterThanGreaterThanToken) {
                    // >> can close two levels
                    if depth <= 2 {
                        self.next_token();
                        break;
                    }
                    depth -= 2;
                } else if self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_token(SyntaxKind::SemicolonToken)
                {
                    break;
                }
                self.next_token();
            }
        } else if !self.is_token(SyntaxKind::OpenParenToken) {
            // import followed by something other than '(' or '.' — not a valid expression.
            // Emit TS1109 "Expression expected" (matches tsc behavior for e.g. `import { ... } from`)
            self.parse_error_at(
                start_pos,
                6, // length of "import"
                diagnostic_messages::EXPRESSION_EXPECTED,
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            // Skip remaining tokens until statement boundary to prevent cascading errors
            while !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
            // Return a missing expression to recover
            return self.create_missing_expression();
        }

        // Dynamic import: import(...)
        self.parse_expected(SyntaxKind::OpenParenToken);

        // Handle spread arguments: import(...expr)
        // tsc parses these as spread elements and reports TS1325 from the checker.
        let argument = if self.is_token(SyntaxKind::DotDotDotToken) {
            let spread_start = self.token_pos();
            self.next_token();
            let expression = self.parse_assignment_expression();
            let spread_end = self.token_end();
            self.arena.add_spread(
                syntax_kind_ext::SPREAD_ELEMENT,
                spread_start,
                spread_end,
                crate::parser::node::SpreadData { expression },
            )
        } else if self.is_token(SyntaxKind::CloseParenToken) {
            // import() with no arguments — tsc parses this as missing argument
            // and the checker reports TS2554 "Expected 1-2 arguments, but got 0."
            self.create_missing_expression()
        } else {
            self.parse_assignment_expression()
        };

        // Optional second argument (import attributes/assertions)
        let options = if self.parse_optional(SyntaxKind::CommaToken) {
            if self.is_token(SyntaxKind::CloseParenToken) {
                // Trailing comma after first arg → TS1009
                self.parse_error_at(
                    self.token_pos().saturating_sub(1),
                    1,
                    "Trailing comma not allowed.",
                    diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                );
                None
            } else {
                Some(
                    if self.in_import_type_options_context
                        && !self.fallback_import_type_options_once
                        && self.look_ahead_is_import_options_object_literal()
                    {
                        self.parse_import_options_object_literal()
                    } else {
                        self.parse_assignment_expression()
                    },
                )
            }
        } else {
            None
        };

        // Consume trailing comma and any excess arguments to avoid
        // cascading TS1005 parse errors. The checker validates arity.
        // TSC allows trailing commas in import() argument lists (like regular
        // function calls), so we don't emit TS1009 here.
        while self.parse_optional(SyntaxKind::CommaToken) {
            if self.is_token(SyntaxKind::CloseParenToken) {
                break;
            }
            self.parse_assignment_expression();
        }

        let end_pos = self.token_end();
        let tail_recovered = self.import_attribute_tail_recovered;
        self.import_attribute_tail_recovered = false;
        if !tail_recovered {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        // Create a call expression with import as the callee
        let import_keyword =
            self.arena
                .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
        let mut args = vec![argument];
        if let Some(opt) = options {
            args.push(opt);
        }
        let arguments = self.make_node_list(args);

        self.arena.add_call_expr(
            syntax_kind_ext::CALL_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::node::CallExprData {
                expression: import_keyword,
                type_arguments: None,
                arguments: Some(arguments),
            },
        )
    }

    /// Parse no-substitution template literal: `hello`
    pub(crate) fn parse_no_substitution_template_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.parse_expected(SyntaxKind::NoSubstitutionTemplateLiteral);
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, end_pos);
        }

        self.arena.add_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template expression: `hello ${name}!`
    pub(crate) fn parse_template_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let head = self.parse_template_head();
        let (end_pos, template_spans) = self.parse_template_expression_spans();

        self.arena.add_template_expr(
            syntax_kind_ext::TEMPLATE_EXPRESSION,
            start_pos,
            end_pos,
            TemplateExprData {
                head,
                template_spans,
            },
        )
    }

    fn parse_template_head(&mut self) -> NodeIndex {
        let head_text = self.scanner.get_token_value_ref().to_string();
        let head_start = self.token_pos();
        let head_end = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.parse_expected(SyntaxKind::TemplateHead);

        self.arena.add_literal(
            SyntaxKind::TemplateHead as u16,
            head_start,
            head_end,
            LiteralData {
                text: head_text,
                raw_text: None,
                value: None,
            },
        )
    }

    fn parse_template_expression_spans(&mut self) -> (u32, NodeList) {
        let mut spans = Vec::new();
        let end_pos = loop {
            let (end_pos, span, is_tail) = self.parse_template_expression_span();
            spans.push(span);
            if is_tail {
                break end_pos;
            }
        };

        (end_pos, self.make_node_list(spans))
    }

    fn parse_template_expression_span(&mut self) -> (u32, NodeIndex, bool) {
        let expression = self.parse_expression();
        if expression.is_none() {
            // Emit TS1109 "Expression expected." for empty template expressions.
            // Position depends on the current token:
            // - If `}` (closing the template span): emit at token_start (after trivia),
            //   matching tsc's createMissingNode with reportAtCurrentPosition=true.
            // - Otherwise (e.g., EOF): emit at full_start (before trivia) so the
            //   position differs from the TS1005 "'}' expected." that follows,
            //   allowing both errors through dedup.
            {
                use tsz_common::diagnostics::diagnostic_codes;
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                } else {
                    let start = self.u32_from_usize(self.scanner.get_token_full_start());
                    let end = self.u32_from_usize(self.scanner.get_token_end());
                    self.parse_error_at(
                        start,
                        end.saturating_sub(start),
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
            }
        }

        if !self.is_token(SyntaxKind::CloseBraceToken) {
            // Emit TS1005 "'}' expected." at the token start position (after whitespace),
            // matching tsc's parseExpected which uses parseErrorAtPosition(scanner.getTokenStart()).
            // parse_error_at's same-position dedup handles the case where there's no
            // whitespace between the expression error and this error.
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'}' expected.", diagnostic_codes::EXPECTED);
            }
            let literal_start = self.token_pos();
            let literal_end = self.token_end();
            let literal = self.arena.add_literal(
                SyntaxKind::TemplateTail as u16,
                literal_start,
                literal_end,
                LiteralData {
                    text: String::new(),
                    raw_text: None,
                    value: None,
                },
            );
            let span_start = self
                .arena
                .get(expression)
                .map_or(literal_start, |node| node.pos);
            let span =
                self.add_template_expression_span(expression, literal, span_start, literal_end);
            return (literal_end, span, true);
        }

        self.scanner.re_scan_template_token(false);
        self.current_token = self.scanner.get_token();

        let literal_start = self.token_pos();
        let is_tail = self.is_token(SyntaxKind::TemplateTail);
        let is_middle = self.is_token(SyntaxKind::TemplateMiddle);
        if !is_tail && !is_middle {
            self.error_token_expected("`");
            let literal_end = self.token_end();
            let literal = self.arena.add_literal(
                SyntaxKind::TemplateTail as u16,
                literal_start,
                literal_end,
                LiteralData {
                    text: String::new(),
                    raw_text: None,
                    value: None,
                },
            );
            let span_start = self
                .arena
                .get(expression)
                .map_or(literal_start, |node| node.pos);
            let span =
                self.add_template_expression_span(expression, literal, span_start, literal_end);
            return (literal_end, span, true);
        }

        let is_unterminated = self.scanner.is_unterminated();
        let literal_text = self.scanner.get_token_value_ref().to_string();
        let literal_kind = if is_tail {
            SyntaxKind::TemplateTail
        } else {
            SyntaxKind::TemplateMiddle
        };

        let literal_end = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.next_token();

        let literal = self.arena.add_literal(
            literal_kind as u16,
            literal_start,
            literal_end,
            LiteralData {
                text: literal_text,
                raw_text: None,
                value: None,
            },
        );
        if is_unterminated {
            self.error_unterminated_template_literal_at(literal_start, literal_end);
        }

        let span_start = if let Some(node) = self.arena.get(expression) {
            node.pos
        } else {
            literal_start
        };
        let span = self.add_template_expression_span(expression, literal, span_start, literal_end);
        (literal_end, span, is_tail)
    }

    fn add_template_expression_span(
        &mut self,
        expression: NodeIndex,
        literal: NodeIndex,
        span_start: u32,
        span_end: u32,
    ) -> NodeIndex {
        self.arena.add_template_span(
            syntax_kind_ext::TEMPLATE_SPAN,
            span_start,
            span_end,
            TemplateSpanData {
                expression,
                literal,
            },
        )
    }

    /// Parse template literal (either no-substitution or full template expression)
    /// Used for both standalone template literals and as the template part of tagged templates
    pub(crate) fn parse_template_literal(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral) {
            self.parse_no_substitution_template_literal()
        } else {
            self.parse_template_expression()
        }
    }

    /// Parse parenthesized expression
    pub(crate) fn parse_parenthesized_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let saved_context_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION;
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression();
        if expression.is_none() {
            // Emit TS1109 for empty parentheses or invalid expression: ([missing])
            self.error_expression_expected();
        }
        let end_pos = self.token_end();
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_expected(SyntaxKind::CloseParenToken);
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("')' expected.", diagnostic_codes::EXPECTED);
            self.recover_parenthesized_expression_typed_arrow_tail();
        }
        self.context_flags = saved_context_flags;

        self.arena.add_parenthesized(
            syntax_kind_ext::PARENTHESIZED_EXPRESSION,
            start_pos,
            end_pos,
            ParenthesizedData { expression },
        )
    }

    fn recover_parenthesized_expression_typed_arrow_tail(&mut self) {
        if !self.is_token(SyntaxKind::ColonToken) {
            return;
        }

        self.next_token();
        let _ = self.parse_type();

        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
            self.next_token();
        }

        if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token();
            let _ = self.parse_assignment_expression();
        }
    }

    /// Parse array literal
    pub(crate) fn parse_array_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();
        let mut emit_semicolon_expected_at_close_bracket = false;
        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CommaToken) {
                // Elided element
                elements.push(NodeIndex::NONE);
            } else if self.is_token(SyntaxKind::DotDotDotToken) {
                // Spread element: ...expr
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1109 for incomplete spread element: [...missing]
                    self.error_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::node::SpreadData { expression },
                );
                elements.push(spread);
            } else {
                let elem = self.parse_assignment_expression();
                if elem.is_none() {
                    // tsc uses TS1137 ("Expression or comma expected") when a closing
                    // delimiter from an outer context terminates the array (e.g. `[)` or
                    // `[...}\n}`), and TS1109 ("Expression expected") otherwise.
                    if matches!(
                        self.token(),
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBraceToken
                    ) {
                        self.error_expression_or_comma_expected();
                    } else {
                        self.error_expression_expected();
                    }
                    // Continue parsing with empty element for error recovery
                }
                elements.push(elem);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_token(SyntaxKind::SemicolonToken) {
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token(); // look past `;`
                    let should_continue = self.is_expression_start()
                        || self.is_token(SyntaxKind::DotDotDotToken)
                        || self.is_token(SyntaxKind::CloseBracketToken);
                    let follows_eof = self.is_token(SyntaxKind::EndOfFileToken);
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;

                    if should_continue {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        emit_semicolon_expected_at_close_bracket = true;
                        self.next_token(); // skip `;`
                        continue;
                    }

                    if follows_eof {
                        self.next_token(); // let missing `]` report at EOF
                        break;
                    }
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token();
                    let colon_followed_by_expression = self.is_expression_start();
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;

                    if colon_followed_by_expression {
                        self.error_comma_expected();
                        self.next_token();
                        continue;
                    }
                }

                // Missing comma - check if next token looks like another array element
                // If so, emit error and continue parsing (better recovery)
                if self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    // We have an element-like token but no comma - likely missing comma
                    // Emit the comma error and continue parsing for better recovery
                    // This handles cases like: [1 2 3] instead of [1, 2, 3]
                    self.error_comma_expected();
                } else {
                    // Not followed by an element, so we're really done
                    break;
                }
            }
        }

        if emit_semicolon_expected_at_close_bracket && self.is_token(SyntaxKind::CloseBracketToken)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(elements),
                multi_line: false,
            },
        )
    }

    /// Check if current token can start an object property
    /// Used for error recovery in object literals when commas are missing
    pub(crate) const fn is_property_start(&self) -> bool {
        match self.token() {
            SyntaxKind::DotDotDotToken
            | SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::AsteriskToken
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::Identifier
            | SyntaxKind::OpenBracketToken => true,
            _ => self.is_identifier_or_keyword(),
        }
    }

    /// Parse object literal
    pub(crate) fn parse_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken) {
            let prop = self.parse_property_assignment();
            if prop.is_some() {
                properties.push(prop);
            }

            // Try to parse comma separator
            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.suppress_object_literal_comma_once && self.is_property_start() {
                    self.suppress_object_literal_comma_once = false;
                    continue;
                }
                self.suppress_object_literal_comma_once = false;

                if self.is_token(SyntaxKind::SemicolonToken) {
                    // Semicolons in object literals: look ahead to decide whether to
                    // treat as a mistyped comma (continue) or abort the list (break).
                    // tsc's parseDelimitedList aborts when the token after `;` is in
                    // some other parsing context (e.g., EOF, statement keyword without
                    // subsequent property-like content). We look ahead past `;` to
                    // decide: if the next token looks like it could continue the object
                    // literal (property start, or `}` to close it), treat `;` as a
                    // mistyped comma. Otherwise, abort the list so the outer parser
                    // can handle the rest as statements.
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token(); // look past `;`
                    let should_continue =
                        self.is_property_start() || self.is_token(SyntaxKind::CloseBraceToken);
                    let follows_eof = self.is_token(SyntaxKind::EndOfFileToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if should_continue {
                        // Treat `;` as mistyped `,` — emit error and continue
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token(); // skip `;`
                    } else if follows_eof {
                        self.next_token(); // let missing `}` report at EOF
                        break;
                    } else {
                        // `;` followed by non-property → abort the list
                        // so the outer parser can handle the rest as statements.
                        break;
                    }
                } else if self.is_property_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    // We have a property-like token but no comma - likely missing comma
                    // Emit the comma error and continue parsing for better recovery
                    // This handles cases like: {a: 1 b: 2} instead of {a: 1, b: 2}
                    self.error_comma_expected();
                } else if self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                {
                    break;
                } else {
                    // Unexpected token (e.g., `.` in `{ m.x }`). Emit ',' expected
                    // and skip to resync, matching tsc's delimited list recovery.
                    self.error_comma_expected();
                    self.next_token();
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    /// Parse property assignment, method, getter, setter, or spread element
    pub(crate) fn parse_property_assignment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle spread element: ...expr
        if self.is_token(SyntaxKind::DotDotDotToken) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            if expression.is_none() {
                // Emit TS1109 for incomplete spread element: {...missing}
                self.error_expression_expected();
            }
            let end_pos = self.token_end();
            return self.arena.add_spread(
                syntax_kind_ext::SPREAD_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::SpreadData { expression },
            );
        }

        // NOTE: public/private/protected/abstract are contextual keywords in object literals.
        // When used as a modifier (followed by another property name), TS1042 is reported.
        // When used as a property name (followed by `:`, `,`, `}`, etc.), they're treated as identifiers.
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::AbstractKeyword
        ) && !self.look_ahead_is_property_name_after_keyword()
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let modifier_name = match self.token() {
                SyntaxKind::PublicKeyword => "'public'",
                SyntaxKind::PrivateKeyword => "'private'",
                SyntaxKind::ProtectedKeyword => "'protected'",
                SyntaxKind::AbstractKeyword => "'abstract'",
                _ => "modifier",
            };
            self.parse_error_at_current_token(
                &format!("{modifier_name} modifier cannot be used here."),
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE, // TS1042
            );
            // TSC also emits TS1184 — but only when the modifier precedes a
            // shorthand method (`public foo() {}`).  Property assignments
            // (`public foo: v`) and accessor declarations (`public get foo()`)
            // only get TS1042.
            {
                let snap = self.scanner.save_state();
                let saved_tok = self.current_token;
                self.next_token(); // peek past modifier
                let is_method = if self.is_identifier_or_keyword() {
                    // Check if identifier is followed by `(` or `<` (method call)
                    self.next_token();
                    matches!(
                        self.token(),
                        SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken
                    )
                } else {
                    false
                };
                self.scanner.restore_state(snap);
                self.current_token = saved_tok;
                if is_method {
                    self.parse_companion_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, // TS1184
                    );
                }
            }
            self.next_token(); // consume the modifier
            // Continue parsing the actual property/method
        }

        // Handle get accessor: get foo() { }
        if self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_get_accessor(start_pos);
        }

        // Handle set accessor: set foo(v) { }
        if self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_set_accessor(start_pos);
        }

        // Handle async method: async foo() { }
        if self.is_token(SyntaxKind::AsyncKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_method(start_pos, true, false);
        }

        // Handle generator method: *foo() { }
        if self.is_token(SyntaxKind::AsteriskToken) {
            self.next_token(); // consume '*'
            return self.parse_object_method(start_pos, false, true);
        }

        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Property assignment expected.",
                diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
            );
            let name = self.parse_template_literal();
            // tsc emits cascading errors when a template literal is used
            // as a property name with `: value` following:
            //   TS1136 on the template, TS1005 "',' expected" at `:`,
            //   TS1134 "Variable declaration expected." at the value.
            // We emit these inline to match tsc's diagnostic output.
            let initializer = if self.is_token(SyntaxKind::ColonToken) {
                let colon_pos = self.token_pos();
                let colon_len = self.token_end() - colon_pos;
                self.parse_error_at(
                    colon_pos,
                    colon_len,
                    "',' expected.",
                    diagnostic_codes::EXPECTED,
                );
                self.next_token(); // consume `:`
                // Emit TS1134 at the value position
                if !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    let val_pos = self.token_pos();
                    let val_len = self.token_end() - val_pos;
                    self.parse_error_at(
                        val_pos,
                        val_len,
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                let expr = self.parse_assignment_expression();
                // Emit TS1128 at the next token (typically `}` or next line)
                // tsc sees the closing `}` in statement context, not as
                // the object literal closer.
                if !self.is_token(SyntaxKind::EndOfFileToken) {
                    let next_pos = self.token_pos();
                    let next_len = self.token_end() - next_pos;
                    self.parse_error_at(
                        next_pos,
                        next_len,
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                expr
            } else {
                name
            };
            let end_pos = self.token_end();
            return self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer,
                },
            );
        }

        // Check if the property name requires `:` syntax (can't be a shorthand property).
        // Shorthand properties only work with identifiers, not:
        // - Reserved words (class, function, etc.)
        // - String literals ("key")
        // - Numeric literals (0, 1, etc.)
        let requires_colon = self.is_reserved_word()
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken);

        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        // Private identifiers (#foo) are not allowed in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        // Handle method: foo() { } or foo<T>() { }
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_object_method_after_name(start_pos, name, false, false);
        }

        // Check for optional property marker '?' - not allowed in object literals
        // TSC emits TS1162: "An object member cannot be declared optional."
        if self.is_token(SyntaxKind::QuestionToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An object member cannot be declared optional.",
                diagnostic_codes::AN_OBJECT_MEMBER_CANNOT_BE_DECLARED_OPTIONAL,
            );
            self.next_token(); // Skip the '?' for error recovery

            // After skipping '?', if followed by '(' or '<', continue parsing as method
            // for error recovery (e.g., `{ foo?() { } }` should still parse the method body)
            if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken)
            {
                return self.parse_object_method_after_name(start_pos, name, false, false);
            }
        }

        // Check for definite assignment assertion '!' - not allowed in object literals.
        // TSC emits TS1255 as a grammar error (not a parse error), so it does not
        // suppress downstream semantic checks. We skip the '!' here for error recovery
        // and let the checker emit TS1255 based on the exclamation_token_pos field.
        let exclamation_pos = if self.is_token(SyntaxKind::ExclamationToken) {
            let pos = self.u32_from_usize(self.scanner.get_token_start());
            self.next_token(); // Skip the '!' for error recovery
            pos
        } else {
            0
        };

        if self.parse_optional(SyntaxKind::ColonToken) {
            let expr = self.parse_assignment_expression();
            let initializer = if expr.is_none() {
                // Emit TS1109 for missing property value: { prop: }
                self.error_expression_expected();
                if self.scanner.has_preceding_line_break() && self.is_property_start() {
                    self.suppress_object_literal_comma_once = true;
                }
                name // Use property name as fallback for error recovery
            } else {
                expr
            };

            let end_pos = self.token_end();
            // Regular property assignment with explicit value
            self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer,
                },
            )
        } else {
            // Shorthand property - but certain property names require `:` syntax
            if requires_colon {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
            }

            // CoverInitializedName: `{ x = expr }` in destructuring patterns
            // ECMAScript: CoverInitializedName[Yield] : IdentifierReference[?Yield] Initializer[In, ?Yield]
            let has_equals = self.parse_optional(SyntaxKind::EqualsToken);
            let initializer = if has_equals {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            // Create SHORTHAND_PROPERTY_ASSIGNMENT node for `{ name }` or `{ name = expr }` syntax
            self.arena.add_shorthand_property(
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::ShorthandPropertyData {
                    modifiers: None,
                    name,
                    equals_token: has_equals,
                    exclamation_token_pos: exclamation_pos,
                    object_assignment_initializer: initializer,
                },
            )
        }
    }

    /// Look ahead to check if get/set/async is a method vs property name
    pub(crate) fn look_ahead_is_object_method(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip get/set/async

        // If there's a line break after get/set/async, it's treated as a property name
        // (shorthand property), not as an accessor or async modifier.
        // This matches TypeScript's ASI behavior.
        if self.scanner.has_preceding_line_break() {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

        // Check if followed by property name (identifier, keyword, string, number, bigint, [)
        // Keywords like 'return', 'throw', 'delete' can be method names
        let is_method = self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken)
            || self.is_token(SyntaxKind::AsteriskToken) // async *foo()
            || self.is_identifier_or_keyword(); // keywords as method names

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_method
    }

    /// Parse get accessor in object literal: get `foo()` { }
    pub(crate) fn parse_object_get_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'get'
        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !had_open_paren {
            // If ( was missing entirely, don't consume following tokens as parameters.
            // They belong to the enclosing context (e.g., object literal list).
            // This prevents `get e,` from consuming `,` as a parameter delimiter
            // and cascading errors into subsequent properties.
            self.make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CommaToken) {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            );
            self.next_token();
            self.make_node_list(vec![])
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the accessor name, matching tsc behavior
            if let Some(name_node) = self.arena.get(name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A 'get' accessor cannot have parameters.",
                    diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                );
            } else {
                self.parse_error_at_current_token(
                    "A 'get' accessor cannot have parameters.",
                    diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                );
            }
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        // Only expect ) if ( was actually found
        if had_open_paren {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        // If there's a type annotation, use its end; otherwise use close paren end
        let signature_end = if type_annotation.is_none() {
            close_paren_end
        } else {
            self.token_pos()
        };

        // Parse body if present. Missing body is reported in grammar check, not here.
        // This matches TypeScript's behavior of allowing ASI and checking later.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, signature_end for missing body
        let end_pos = if body.is_none() {
            signature_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor in object literal: set foo(v) { }
    pub(crate) fn parse_object_set_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'set'
        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !had_open_paren {
            // If ( was missing entirely, don't consume following tokens as parameters.
            // They belong to the enclosing context (e.g., object literal list).
            self.make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        if had_open_paren {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        if self.parse_optional(SyntaxKind::ColonToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the accessor name, matching tsc behavior
            if let Some(name_node) = self.arena.get(name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A 'set' accessor cannot have a return type annotation.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                );
            } else {
                self.parse_error_at_current_token(
                    "A 'set' accessor cannot have a return type annotation.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                );
            }
            let _ = self.parse_type();
        }

        // Parse body if present. Missing body is reported in grammar check, not here.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, close_paren_end for missing body
        let end_pos = if body.is_none() {
            close_paren_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation: NodeIndex::NONE,
                body,
            },
        )
    }

    /// Parse method in object literal: `foo()` { } or async `foo()` { } or *`foo()` { }
    pub(crate) fn parse_object_method(
        &mut self,
        start_pos: u32,
        is_async: bool,
        is_generator: bool,
    ) -> NodeIndex {
        // Build modifiers if async
        let modifiers = is_async.then(|| {
            self.next_token();
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            self.make_node_list(vec![mod_idx])
        });

        // Check for generator after async: async *foo()
        // or standalone generator: *foo()
        let asterisk = if is_generator {
            // Asterisk already consumed by caller for standalone generator
            true
        } else if self.parse_optional(SyntaxKind::AsteriskToken) {
            // async *foo() - consume asterisk here
            true
        } else {
            false
        };

        // Recovery for malformed generator object members:
        //   *{}        -> synthesize empty parameter list and parse body
        //   *<T>() {}  -> parse type params/signature, omit missing name
        //   *} / *,    -> drop invalid member
        if asterisk
            && (self.is_token(SyntaxKind::LessThanToken)
                || self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::CommaToken))
        {
            if self.is_token(SyntaxKind::CloseBraceToken) || self.is_token(SyntaxKind::CommaToken) {
                // TS1003: Identifier expected (after `*` with no name before `}` or `,`)
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                );
                return NodeIndex::NONE;
            }

            // TS1003: Identifier expected (generator method without name)
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
            );

            let type_parameters = self
                .is_token(SyntaxKind::LessThanToken)
                .then(|| self.parse_type_parameters());

            let parameters = if self.is_token(SyntaxKind::OpenParenToken) {
                self.parse_expected(SyntaxKind::OpenParenToken);
                let params = self.parse_parameter_list();
                self.parse_expected(SyntaxKind::CloseParenToken);
                params
            } else {
                self.make_node_list(vec![])
            };

            let saved_flags = self.context_flags;
            self.context_flags &=
                !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
            if is_async {
                self.context_flags |= CONTEXT_FLAG_ASYNC;
            }
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
            self.push_label_scope();
            let body = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_block()
            } else {
                NodeIndex::NONE
            };
            self.pop_label_scope();
            self.context_flags = saved_flags;

            let end_pos = self.token_end();
            return self.arena.add_method_decl(
                syntax_kind_ext::METHOD_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::MethodDeclData {
                    modifiers,
                    asterisk_token: true,
                    name: NodeIndex::NONE,
                    question_token: false,
                    type_parameters,
                    parameters,
                    type_annotation: NodeIndex::NONE,
                    body,
                },
            );
        }

        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        self.parse_object_method_after_name(start_pos, name, asterisk, modifiers.is_some())
    }

    /// Parse method after name has been parsed
    pub(crate) fn parse_object_method_after_name(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        asterisk: bool,
        is_async: bool,
    ) -> NodeIndex {
        // Optional type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        let has_open_paren = self.parse_optional(SyntaxKind::OpenParenToken);
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            self.recover_from_missing_method_open_paren();
            self.make_node_list(vec![])
        };

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Push a new label scope for the method body
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // tsc emits TS1005 "'{' expected." when an object method body is missing
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            NodeIndex::NONE
        };
        self.pop_label_scope();

        // Restore context flags after parsing body.
        self.context_flags = saved_flags;

        let modifiers = is_async.then(|| {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            self.make_node_list(vec![mod_idx])
        });

        let end_pos = self.token_end();
        self.arena.add_method_decl(
            syntax_kind_ext::METHOD_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::MethodDeclData {
                modifiers,
                asterisk_token: asterisk,
                name,
                question_token: false,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse property name (identifier, string literal, numeric literal, bigint literal, computed)
    pub(crate) fn parse_property_name(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::StringLiteral => {
                // String literal can be property name: { "key": value }
                self.parse_string_literal()
            }
            SyntaxKind::NumericLiteral => {
                // Numeric literal can be property name: { 0: value }
                self.parse_numeric_literal()
            }
            SyntaxKind::BigIntLiteral => {
                // BigInt literal can be a property name for parser recovery/parity.
                self.parse_bigint_literal()
            }
            SyntaxKind::OpenBracketToken => {
                // Computed property name: { [expr]: value }
                let start_pos = self.token_pos();
                self.next_token();

                // In class member computed property names, keywords such as `public`
                // and `yield` should emit TS1213.
                if self.in_class_member_name()
                    && !self.in_generator_context()
                    && !self.is_computed_class_member_yield_expression()
                {
                    self.check_illegal_binding_identifier();
                }

                // Note: await in computed property name is NOT a parser error
                // The type checker will emit TS2304 if 'await' is not in scope
                // Example: { [await]: foo } should only emit TS2304, not TS1109

                let expression = self.parse_expression();
                if expression.is_none() {
                    // Emit TS1109 for empty computed property: { [[missing]]: value }
                    self.error_expression_expected();
                } else if self.computed_name_is_comma_expression(expression) {
                    let Some(expr_node) = self.arena.get(expression) else {
                        return self.arena.add_computed_property(
                            syntax_kind_ext::COMPUTED_PROPERTY_NAME,
                            start_pos,
                            self.token_end(),
                            crate::parser::node::ComputedPropertyData { expression },
                        );
                    };
                    self.parse_error_at(
                        expr_node.pos,
                        expr_node.end.saturating_sub(expr_node.pos),
                        diagnostic_messages::A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME,
                        diagnostic_codes::A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME,
                    );
                }
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_end();

                self.arena.add_computed_property(
                    syntax_kind_ext::COMPUTED_PROPERTY_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::node::ComputedPropertyData { expression },
                )
            }
            SyntaxKind::PrivateIdentifier => {
                // Private identifier: #name
                self.parse_private_identifier()
            }
            _ => {
                // Identifier or keyword used as property name
                // But first check if it's actually a valid identifier/keyword
                let start_pos = self.token_pos();
                let is_identifier_or_keyword = self.is_identifier_or_keyword();

                if !is_identifier_or_keyword {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Property assignment expected.",
                        diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                    );
                }

                // OPTIMIZATION: Capture atom for O(1) comparison
                let atom = self.scanner.get_token_atom();
                // Use zero-copy accessor
                let text = self.scanner.get_token_value_ref().to_string();
                // Preserve unicode escape sequences for emission parity with tsc
                let original_text =
                    if (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0 {
                        let src = self.scanner.source_text();
                        let start = self.scanner.get_token_start();
                        let end = self.scanner.get_token_end();
                        if start < end && end <= src.len() {
                            let slice = &src[start..end];
                            if slice != text {
                                Some(slice.to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                self.next_token(); // Accept any token as property name (error recovery)
                let end_pos = self.token_end();

                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    start_pos,
                    end_pos,
                    IdentifierData {
                        atom,
                        escaped_text: text,
                        original_text,
                        type_arguments: None,
                    },
                )
            }
        }
    }

    pub(crate) fn is_computed_class_member_yield_expression(&mut self) -> bool {
        if !self.in_class_member_name() || !self.is_token(SyntaxKind::YieldKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current_token = self.current_token;
        self.next_token();
        let next_token = self.token();
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current_token;

        if has_line_break {
            return false;
        }

        !matches!(
            next_token,
            SyntaxKind::CloseBracketToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::ColonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::EqualsGreaterThanToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::EndOfFileToken
        )
    }

    /// Check whether an expression node is a computed property name that uses a top-level
    /// comma expression (e.g., `[0, 1]`).
    fn computed_name_is_comma_expression(&self, expression: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(expression)
            && let Some(binary_expr) = self.arena.get_binary_expr(node)
        {
            return binary_expr.operator_token == SyntaxKind::CommaToken as u16;
        }
        false
    }

    /// Parse new expression
    pub(crate) fn parse_new_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Handle new.target meta-property
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            let new_node =
                self.arena
                    .add_token(SyntaxKind::NewKeyword as u16, start_pos, start_pos + 3);
            let name = self.parse_identifier_name();
            // TS17012: Check that the meta-property is 'target', not a misspelling
            if let Some(name_node) = self.arena.get(name)
                && let Some(ident) = self.arena.get_identifier(name_node)
                && ident.escaped_text != "target"
            {
                let msg = format_message(
                    diagnostic_messages::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN,
                    &[&ident.escaped_text.to_string(), "new", "target"],
                );
                self.parse_error_at(
                    name_node.pos,
                    name_node.end.saturating_sub(name_node.pos),
                    &msg,
                    diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN,
                );
            }
            let end_pos = self
                .arena
                .get(name)
                .map_or_else(|| self.token_end(), |n| n.end);
            return self.arena.add_access_expr(
                syntax_kind_ext::META_PROPERTY,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: new_node,
                    question_dot_token: false,
                    name_or_argument: name,
                },
            );
        }

        // Type assertion syntax (<T>expr) is not valid in new expressions
        // Check if the next token is '<' and report TS1109 if so
        if self.is_token(SyntaxKind::LessThanToken) {
            self.error_expression_expected();
        }

        // Parse the callee expression - member access without call (we handle call ourselves)
        let expression = self.parse_member_expression_base();
        let mut end_pos = self
            .arena
            .get(expression)
            .map_or_else(|| self.token_end(), |node| node.end);

        // Parse type arguments: new Array<string>()
        // Use try_parse to handle ambiguity with comparison operators (e.g., new Date<A)
        let type_arguments = if self.is_less_than_or_compound() {
            self.try_parse_type_arguments_for_call()
        } else {
            None
        };
        if let Some(type_args) = type_arguments.as_ref()
            && let Some(last) = type_args.nodes.last()
            && let Some(node) = self.arena.get(*last)
        {
            end_pos = end_pos.max(node.end);
        }

        let arguments = self.is_token(SyntaxKind::OpenParenToken).then(|| {
            self.next_token();
            let args = self.parse_argument_list();
            let call_end = self.token_end();
            self.parse_expected(SyntaxKind::CloseParenToken);
            end_pos = call_end;
            args
        });

        self.arena.add_call_expr(
            syntax_kind_ext::NEW_EXPRESSION,
            start_pos,
            end_pos,
            CallExprData {
                expression,
                type_arguments,
                arguments,
            },
        )
    }

    fn look_ahead_is_import_options_object_literal(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBraceToken) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();

        let result = self.is_token(SyntaxKind::CloseBraceToken)
            || self.look_ahead_is_import_attributes_property();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    fn look_ahead_is_import_attributes_property(&mut self) -> bool {
        let matches_key = if self.is_token(SyntaxKind::StringLiteral) {
            matches!(self.scanner.get_token_value_ref(), "with" | "assert")
        } else if self.is_identifier_or_keyword() {
            matches!(self.scanner.get_token_value_ref(), "with" | "assert")
        } else {
            false
        };

        if !matches_key {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        if self.is_token(SyntaxKind::StringLiteral) {
            self.parse_string_literal();
        } else {
            self.parse_identifier_name();
        }

        let result = self.is_token(SyntaxKind::ColonToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    fn import_property_name_matches(&self, name: NodeIndex, expected: &str) -> bool {
        let Some(node) = self.arena.get(name) else {
            return false;
        };

        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text == expected;
        }

        if let Some(literal) = self.arena.get_literal(node) {
            return literal.text == expected;
        }

        false
    }

    fn parse_import_options_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        let mut aborted_after_nested_recovery = false;
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let prop = if self.look_ahead_is_import_attributes_property() {
                self.parse_import_options_property_assignment()
            } else {
                self.parse_property_assignment()
            };

            if prop.is_some() {
                properties.push(prop);
            }

            if self.import_attribute_tail_recovered {
                aborted_after_nested_recovery = true;
                break;
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_token(SyntaxKind::SemicolonToken) {
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let should_continue =
                        self.is_property_start() || self.is_token(SyntaxKind::CloseBraceToken);
                    let follows_eof = self.is_token(SyntaxKind::EndOfFileToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if should_continue {
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                    } else if follows_eof {
                        self.error_comma_expected();
                        break;
                    } else {
                        break;
                    }
                } else if self.is_property_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    self.error_comma_expected();
                } else if self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                {
                    break;
                } else {
                    self.error_comma_expected();
                    self.next_token();
                }
            }
        }

        let end_pos = self.token_end();
        if !aborted_after_nested_recovery {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    fn parse_import_options_property_assignment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let name = self.parse_property_name();

        let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
            if (self.import_property_name_matches(name, "with")
                || self.import_property_name_matches(name, "assert"))
                && self.is_token(SyntaxKind::OpenBraceToken)
            {
                self.parse_import_attributes_value_object_literal()
            } else {
                self.parse_assignment_expression()
            }
        } else {
            self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
            name
        };

        let end_pos = self.token_end();
        self.arena.add_property_assignment(
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            start_pos,
            end_pos,
            crate::parser::node::PropertyAssignmentData {
                modifiers: None,
                name,
                initializer,
            },
        )
    }

    fn parse_import_attributes_value_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        let mut aborted_on_invalid_key = false;

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let prop_start = self.token_pos();
            let name = if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                let mut semicolon_recovery = None;
                self.parse_error_at_current_token(
                    diagnostic_messages::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                    diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                );
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    if semicolon_recovery.is_none() && self.is_token(SyntaxKind::ColonToken) {
                        let start = self.u32_from_usize(self.scanner.get_token_start());
                        let end = self.u32_from_usize(self.scanner.get_token_end());
                        semicolon_recovery = Some((start, end - start));
                    }
                    self.next_token();
                }
                self.abort_intersection_continuation = true;
                self.report_invalid_import_attribute_tail_recovery(semicolon_recovery);
                aborted_on_invalid_key = true;
                break;
            };

            self.parse_expected(SyntaxKind::ColonToken);
            let value = self.parse_assignment_expression();
            let end_pos = self
                .arena
                .get(value)
                .map_or_else(|| self.token_end(), |node| node.end);

            properties.push(self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                prop_start,
                end_pos,
                crate::parser::node::PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer: value,
                },
            ));

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = self.token_end();
        if !aborted_on_invalid_key {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    fn report_invalid_import_attribute_tail_recovery(
        &mut self,
        semicolon_recovery: Option<(u32, u32)>,
    ) {
        if let Some((start, length)) = semicolon_recovery {
            self.parse_error_at(start, length, "';' expected.", diagnostic_codes::EXPECTED);
        }

        let mut saw_dot = false;
        while matches!(
            self.token(),
            SyntaxKind::CloseBraceToken | SyntaxKind::CloseParenToken | SyntaxKind::DotToken
        ) {
            let token = self.token();
            self.parse_error_at_current_token(
                diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            if token == SyntaxKind::CloseParenToken {
                self.import_attribute_tail_recovered = true;
            }
            saw_dot = token == SyntaxKind::DotToken;
            self.next_token();
            if self.scanner.has_preceding_line_break() {
                return;
            }
        }

        if saw_dot && self.is_identifier_or_keyword() {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_has_line_break = self.scanner.has_preceding_line_break();
            let next_token = self.token();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if !next_has_line_break {
                self.parse_error_at_current_token(
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
            }
            self.next_token();
            if !next_has_line_break && next_token == SyntaxKind::CloseParenToken {
                self.parse_error_at_current_token(
                    diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
            }
        }
    }

    /// Parse member expression base (identifier with property/element access, but no calls)
    pub(crate) fn parse_member_expression_base(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let diag_count_before = self.parse_diagnostics.len();
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };

                    // If parsing the name produced an error, don't create a property access
                    // expression to avoid spurious semantic errors (e.g., TS2339 for incomplete `this.`)
                    if self.parse_diagnostics.len() > diag_count_before {
                        break;
                    }

                    let end_pos = self.token_end();

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: name,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenBracketToken => {
                    let missing_argument_start = self.u32_from_usize(self.scanner.get_token_end());
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // TS1011: An element access expression should take an argument
                        let current_start = self.u32_from_usize(self.scanner.get_token_start());
                        self.parse_error_at(
                            missing_argument_start,
                            (current_start.saturating_sub(missing_argument_start)).max(1),
                            tsz_common::diagnostics::diagnostic_messages::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                            tsz_common::diagnostics::diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                        );
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseBracketToken);

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: argument,
                            question_dot_token: false,
                        },
                    );
                }
                // Optional chaining: `new A?.b()` — parse `?.prop` or `?.[idx]`
                // as part of the member expression so the NewExpression wraps
                // the whole chain.  The checker later emits TS1209 for this.
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // `new A?.[idx]()`
                        self.next_token();
                        let argument = self.parse_expression();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseBracketToken);

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: argument,
                                question_dot_token: true,
                            },
                        );
                    } else {
                        // `new A?.b()` — property access
                        let name = if self.is_identifier_or_keyword() {
                            self.parse_identifier_name()
                        } else {
                            self.error_identifier_expected();
                            NodeIndex::NONE
                        };
                        let end_pos = self.token_end();

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: name,
                                question_dot_token: true,
                            },
                        );
                    }
                }
                // Tagged template literals: tag`template` — needed so that
                // `new f\`abc\`.member(...)` parses the tagged template as
                // part of the member expression, not as `(new f)\`abc\`...`.
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal();
                    let end_pos = self.token_end();

                    expr = self.arena.add_tagged_template(
                        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                        start_pos,
                        end_pos,
                        TaggedTemplateData {
                            tag: expr,
                            type_arguments: None,
                            template,
                        },
                    );
                }
                _ => break,
            }
        }

        expr
    }
}
