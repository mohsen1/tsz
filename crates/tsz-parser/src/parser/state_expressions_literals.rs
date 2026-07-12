//! Parser state - literal, binding pattern, and compound expression parsing.

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_DISALLOW_IN, CONTEXT_FLAG_FUNCTION_BODY,
    CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION, CONTEXT_FLAG_STATIC_BLOCK,
    ParserState,
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
use tsz_common::interner::AstAtom;
use tsz_scanner::SyntaxKind;
use tsz_scanner::keyword_text_len;
use tsz_scanner::scanner_impl::TokenFlags;

#[path = "state_expressions_literals/object_members.rs"]
mod object_members;

impl ParserState {
    /// Parse object binding pattern: { x, y: z, ...rest }
    pub(crate) fn parse_object_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();

        let mut has_trailing_comma = false;
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
                    let name = if name_is_reserved {
                        let start = self.token_pos();
                        let end = self.token_end();
                        let word = self.current_keyword_text();
                        self.parse_error_at_current_token(
                            &format!(
                                "Identifier expected. '{word}' is a reserved word that cannot be used here."
                            ),
                            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                        );
                        self.arena.add_identifier(
                            SyntaxKind::Identifier as u16,
                            start,
                            end,
                            IdentifierData {
                                atom: AstAtom::NONE,
                                escaped_text: String::new(),
                                original_text: None,
                            },
                        )
                    } else {
                        self.parse_binding_element_name()
                    };
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
                self.parse_error_at_current_token(
                    "',' expected.",
                    tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                );

                // Skip tokens that cannot start a binding element so the
                // next loop iteration sees either a valid element start, `}`
                // or EOF. Without this, tokens like `?` (from `{h?}`) get
                // fed to parse_property_name producing cascading errors that
                // tsc avoids.
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_identifier_or_keyword()
                    && !self.is_token(SyntaxKind::OpenBraceToken)
                    && !self.is_token(SyntaxKind::OpenBracketToken)
                    && !self.is_token(SyntaxKind::DotDotDotToken)
                    && !self.is_token(SyntaxKind::CommaToken)
                    && !self.is_token(SyntaxKind::StringLiteral)
                    && !self.is_token(SyntaxKind::NumericLiteral)
                    && !self.is_token(SyntaxKind::BigIntLiteral)
                {
                    self.next_token();
                }
            } else if self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::EndOfFileToken)
            {
                has_trailing_comma = true;
                break;
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
                elements: {
                    let mut list = Self::make_node_list(elements);
                    list.has_trailing_comma = has_trailing_comma;
                    list
                },
            },
        )
    }

    /// Parse array binding pattern: [x, y, ...rest]
    pub(crate) fn parse_array_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();
        let mut last_comma_pos = None;
        let mut reserved_word_element_needs_close_error = false;

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle omitted element: [, , x]
            if self.is_token(SyntaxKind::CommaToken) {
                // Omitted element - push NONE as placeholder
                elements.push(NodeIndex::NONE);
                last_comma_pos = Some(self.token_pos());
                self.next_token();
                continue;
            }

            let in_parameter_binding_pattern = (self.context_flags
                & crate::parser::state::CONTEXT_FLAG_PARAMETER_BINDING_PATTERN)
                != 0;

            // Reserved words in the first array-binding position should recover as
            // an invalid destructuring pattern rather than a generic identifier error.
            if elements.is_empty() && self.is_reserved_word() {
                self.error_array_element_destructuring_pattern_expected();
                if in_parameter_binding_pattern
                    && self.recover_reserved_parameter_as_statement_tail_allowed
                {
                    self.reserved_parameter_yielded_to_statement = true;
                    break;
                }
                self.next_token();
                continue;
            }

            // Later reserved words in an array binding should stay on the
            // structural recovery path instead of surfacing a reserved-word
            // identifier diagnostic that tsc does not emit here.
            let hard_reserved_word = self.is_reserved_word();
            let parameter_future_reserved_word =
                in_parameter_binding_pattern && self.is_strict_mode_future_reserved_word();
            if !elements.is_empty() && (hard_reserved_word || parameter_future_reserved_word) {
                if let Some(comma_pos) = last_comma_pos {
                    let message = if in_parameter_binding_pattern {
                        "'(' expected."
                    } else {
                        "';' expected."
                    };
                    self.parse_error_at(comma_pos, 1, message, diagnostic_codes::EXPECTED);
                }
                if in_parameter_binding_pattern && hard_reserved_word {
                    self.parse_companion_error_at_current_token(
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
                self.pending_array_binding_tail_recovery = true;
                reserved_word_element_needs_close_error = true;
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
                let saved = self.context_flags;
                self.context_flags &= !CONTEXT_FLAG_DISALLOW_IN;
                let init = self.parse_assignment_expression();
                self.context_flags = saved;
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
            last_comma_pos = Some(self.token_pos().saturating_sub(1));
        }

        let in_parameter_binding_pattern = (self.context_flags
            & crate::parser::state::CONTEXT_FLAG_PARAMETER_BINDING_PATTERN)
            != 0;
        if reserved_word_element_needs_close_error {
            let message = if in_parameter_binding_pattern {
                "';' expected."
            } else {
                "'(' expected."
            };
            self.parse_error_at_current_token(message, diagnostic_codes::EXPECTED);
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        if reserved_word_element_needs_close_error
            && ((in_parameter_binding_pattern && self.is_token(SyntaxKind::CloseParenToken))
                || self.is_token(SyntaxKind::EqualsToken))
        {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }

        self.arena.add_binding_pattern(
            syntax_kind_ext::ARRAY_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::node::BindingPatternData {
                elements: Self::make_node_list(elements),
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

        // TS1124 ("Digit expected") for empty exponents (`1e+`, `1e`, `1ee`,
        // `3en`, etc.) is emitted by the scanner inline during
        // `scan_decimal_number` so the position matches tsc (right after the
        // `e`/sign) and the same-start dedup in
        // `check_for_identifier_start_after_numeric_literal` can suppress a
        // colliding TS1351 the same way tsc's `parseErrorAtPosition` does.

        // Note: TS1125/TS1177/TS1178 ("Hexadecimal/Binary/Octal digit expected")
        // for empty- or invalid-digit prefixed integer literals (`0x`, `0b21010`,
        // `0o81010`, etc.) are emitted by the scanner in
        // `scan_integer_base_literal`'s `if !saw_digit` branch at the same
        // position the parser would re-emit them here. Re-emitting via
        // `parse_error_at` would dedup against the scanner's diagnostic but
        // bumps `scanner_diagnostics_high_water_mark` — "consuming" the slot
        // that subsequent diagnostics like TS1005 (',' expected) at the same
        // position would otherwise dedup against, leaking spurious TS1005 at
        // every malformed-base-literal site. Removing the parser-side
        // duplicates is behavior-preserving for the digit-expected diagnostic
        // itself (still emitted by the scanner) and lets the position-based
        // dedup work for cascading parser errors.
        let _ = end_pos;

        // Check if this numeric literal has an invalid separator (for TS1351 check).
        // The scanner has already pushed per-occurrence TS6188/TS6189 diagnostics
        // into `scanner_diagnostics`; the parser consults the flag and the first-pos
        // accessor only for the recovery path below (e.g. TS1351/TS2304 emission
        // when a recoverable identifier follows the literal).
        let has_invalid_separator =
            (token_flags & TokenFlags::ContainsInvalidSeparator as u32) != 0;

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

        // Note: tsc does NOT emit TS2304 ("Cannot find name") for the
        // recovered identifier after an invalid separator (e.g. `0_X0101`).
        // The TS1351 and TS6188 diagnostics already cover the user-facing
        // error; the identifier survives only as a parser-recovery token.
        // Suppressing TS2304 here matches tsc's
        // `parser.numericSeparators.{hex,binary,octal}Negative.ts` baselines.
        let _ = invalid_separator_pos;

        self.arena.add_literal(
            SyntaxKind::NumericLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value,
                has_invalid_escape: false,
            },
        )
    }

    /// Parse bigint literal
    /// Uses zero-copy accessor, stores the raw text (e.g. "123n")
    pub(crate) fn parse_bigint_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        // Per-occurrence TS6188/TS6189 diagnostics for invalid separators are
        // emitted by the scanner during numeric scanning.
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::BigIntLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
                has_invalid_escape: false,
            },
        )
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

        let super_node = self
            .arena
            .add_token(SyntaxKind::SuperKeyword as u16, start_pos, end_pos);

        // If super is followed by (, ., [, or <, return just the super keyword.
        // The caller (parse_member_expression_rest) will handle the access chain.
        if self.is_token(SyntaxKind::OpenParenToken)
            || self.is_token(SyntaxKind::DotToken)
            || self.is_token(SyntaxKind::OpenBracketToken)
            || self.is_token(SyntaxKind::LessThanToken)
        {
            return super_node;
        }

        // `super` must be followed by an argument list or member access. When it
        // is not, tsc recovers by consuming an (expected) `.` token and parsing
        // the right side of the dot, which yields a missing-identifier
        // PropertyAccessExpression (`super.<missing>`). The downstream emitter
        // then prints `super.` verbatim, and the static-member super lowering
        // sees a real property access (e.g. `Reflect.get(base, "", self)` with an
        // empty key). Matching tsc structurally — rather than returning a bare
        // `super` keyword — keeps error-recovery emit aligned with tsc.
        //
        // Emit TS1034 at the current token position (matching tsc's
        // `parseExpectedToken(DotToken, ...)`).
        let missing_name_pos = self.token_pos();
        self.parse_error_at_current_token(
            diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
            diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
        );

        // Build `super.<missing>`: a property access whose name is the missing
        // identifier (NodeIndex::NONE), spanning from `super` to the recovery
        // point. The dot token is synthetic (no `.` exists in the source).
        self.arena.add_access_expr(
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
            start_pos,
            missing_name_pos,
            AccessExprData {
                expression: super_node,
                name_or_argument: NodeIndex::NONE,
                question_dot_token: false,
            },
        )
    }

    /// Parse import expression: import(...), import.meta, or import.defer(...)
    pub(crate) fn parse_import_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);
        let import_node = self.arena.add_token(
            SyntaxKind::ImportKeyword as u16,
            start_pos,
            start_pos + keyword_text_len(SyntaxKind::ImportKeyword),
        );
        let mut import_call_type_arguments: Option<NodeList> = None;

        // Check for import.meta / import.defer(...)
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
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

            // In type-import query contexts, `typeof import.defer("...")` should
            // recover as a malformed dynamic import call, not as a valid
            // meta-property access. This yields TS1005 '(' + ')' expected and
            // avoids semantic fallback noise (e.g. TS2339).
            if self.in_import_type_options_context
                && prop_name == "defer"
                && self.is_token(SyntaxKind::OpenParenToken)
            {
                self.parse_error_at(
                    name_start.saturating_sub(1),
                    name_end.saturating_sub(name_start),
                    "'(' expected.",
                    diagnostic_codes::EXPECTED,
                );
                self.parse_error_at_current_token("')' expected.", diagnostic_codes::EXPECTED);
            } else {
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
        }

        // Check for invalid import forms before expecting '('
        if self.is_token(SyntaxKind::LessThanToken) {
            // import<T>(...) — type arguments not allowed on import calls (TS1326)
            self.parse_error_at(
                start_pos,
                keyword_text_len(SyntaxKind::ImportKeyword),
                diagnostic_messages::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR,
                diagnostic_codes::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR,
            );
            // Preserve type arguments so unresolved names still surface as semantic
            // diagnostics (e.g. TS2304 in `import<T>`).
            //
            // tsc suppresses TS1099 for `import<>` and reports only TS1326.
            // Keep that behavior by dropping the empty-type-arg diagnostic only
            // in this import-call recovery path.
            let diagnostics_len_before_type_args = self.parse_diagnostics.len();
            let type_arguments = self.parse_type_arguments();
            if type_arguments.nodes.is_empty()
                && let Some(last) = self.parse_diagnostics.last()
                && self.parse_diagnostics.len() > diagnostics_len_before_type_args
                && last.code == diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY
            {
                self.parse_diagnostics.pop();
            }
            if !self.is_token(SyntaxKind::OpenParenToken) {
                let end_pos = type_arguments
                    .nodes
                    .last()
                    .and_then(|last| self.arena.get(*last))
                    .map_or_else(|| self.token_end(), |node| node.end);
                return self.arena.add_expr_with_type_args(
                    syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                    start_pos,
                    end_pos,
                    crate::parser::node::ExprWithTypeArgsData {
                        expression: import_node,
                        type_arguments: Some(type_arguments),
                    },
                );
            }
            import_call_type_arguments = Some(type_arguments);
        } else if !self.is_token(SyntaxKind::OpenParenToken) {
            // import followed by something other than '(' or '.' — not a valid expression.
            // Emit TS1109 "Expression expected" (matches tsc behavior for e.g. `import { ... } from`)
            self.parse_error_at(
                start_pos,
                keyword_text_len(SyntaxKind::ImportKeyword),
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
                        && self.is_token(SyntaxKind::OpenBraceToken)
                    {
                        self.parse_import_options_object_literal()
                    } else {
                        let options_starts_with_array = self.is_token(SyntaxKind::OpenBracketToken);
                        let options_starts_with_identifier = self.is_identifier_or_keyword();
                        if self.in_import_type_options_context
                            && !self.fallback_import_type_options_once
                            && !self.is_token(SyntaxKind::OpenBraceToken)
                        {
                            self.error_token_expected("{");
                            // For malformed import-type options in intersections,
                            // match tsc by downgrading the next `& import(...)`
                            // constituent to expression-mode option parsing.
                            self.abort_intersection_continuation = true;
                        }
                        let parsed = self.parse_assignment_expression();

                        if self.in_import_type_options_context
                            && !self.fallback_import_type_options_once
                            && !self.is_token(SyntaxKind::OpenBraceToken)
                        {
                            let mut dot_pos = None;
                            let mut inner_close_paren = None;
                            let mut outer_close_paren = None;

                            if self.is_token(SyntaxKind::CloseParenToken) {
                                let inner_start = self.token_pos();
                                let inner_end = self.token_end();
                                inner_close_paren =
                                    Some((inner_start, inner_end.saturating_sub(inner_start)));

                                let snapshot = self.scanner.save_state();
                                let saved_token = self.current_token;
                                self.next_token();
                                if self.is_token(SyntaxKind::DotToken) {
                                    dot_pos = Some(self.token_pos());
                                    self.next_token();
                                    if self.is_identifier_or_keyword() {
                                        self.next_token();
                                        if self.is_token(SyntaxKind::CloseParenToken)
                                            && !self.scanner.has_preceding_line_break()
                                        {
                                            let outer_start = self.token_pos();
                                            let outer_end = self.token_end();
                                            outer_close_paren = Some((
                                                outer_start,
                                                outer_end.saturating_sub(outer_start),
                                            ));
                                        }
                                    }
                                }
                                self.scanner.restore_state(snapshot);
                                self.current_token = saved_token;
                            }

                            if options_starts_with_identifier {
                                if let Some((outer_start, outer_len)) = outer_close_paren {
                                    self.error_comma_expected();
                                    if let Some(dot_start) = dot_pos {
                                        self.parse_error_at(
                                            dot_start,
                                            1,
                                            diagnostic_messages::VARIABLE_DECLARATION_EXPECTED,
                                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                                        );
                                    }
                                    self.parse_error_at(
                                        outer_start,
                                        outer_len,
                                        "',' expected.",
                                        diagnostic_codes::EXPECTED,
                                    );
                                } else if dot_pos.is_some() {
                                    self.report_invalid_import_attribute_tail_recovery(None);
                                }
                            } else if options_starts_with_array {
                                if let Some((outer_start, outer_len)) = outer_close_paren {
                                    self.parse_error_at(
                                        outer_start,
                                        outer_len,
                                        "',' expected.",
                                        diagnostic_codes::EXPECTED,
                                    );
                                } else if let (Some((inner_start, inner_len)), Some(dot_start)) =
                                    (inner_close_paren, dot_pos)
                                {
                                    self.parse_error_at(
                                        inner_start,
                                        inner_len,
                                        "';' expected.",
                                        diagnostic_codes::EXPECTED,
                                    );
                                    self.parse_error_at(
                                        dot_start,
                                        1,
                                        diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                                    );
                                }
                            }
                        }
                        parsed
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
        let mut args = vec![argument];
        if let Some(opt) = options {
            args.push(opt);
        }
        let arguments = Self::make_node_list(args);

        self.arena.add_call_expr(
            syntax_kind_ext::CALL_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::node::CallExprData {
                expression: import_node,
                type_arguments: import_call_type_arguments,
                arguments: Some(arguments),
                question_dot_token: false,
            },
        )
    }

    /// Parse no-substitution template literal: `hello`
    pub(crate) fn parse_no_substitution_template_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let text = self.scanner.get_token_value_ref().to_string();
        let raw_text = Some(self.scanner.get_token_text_ref().to_string());
        let has_invalid_escape =
            (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32) != 0;
        let end_pos = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.parse_expected(SyntaxKind::NoSubstitutionTemplateLiteral);
        if is_unterminated {
            self.report_unterminated_template_recovery_delimiters(start_pos, end_pos);
            self.error_unterminated_template_literal_at(start_pos, end_pos);
        }

        self.arena.add_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text,
                value: None,
                has_invalid_escape,
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
        let raw_text = Some(self.scanner.get_token_text_ref().to_string());
        let has_invalid_escape =
            (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32) != 0;
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
                raw_text,
                value: None,
                has_invalid_escape,
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

        (end_pos, Self::make_node_list(spans))
    }

    fn parse_template_expression_span(&mut self) -> (u32, NodeIndex, bool) {
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_TEMPLATE_SPAN_EXPRESSION;
        // A `${...}` substitution is an independent expression: any template
        // literal it contains determines its own tagged status. The enclosing
        // template's tagged status must not leak in, or a nested *untagged*
        // template's invalid escapes would be wrongly suppressed. tsc parses the
        // span expression via `allowInAnd(parseExpression)` and only re-applies
        // `isTaggedTemplate` when re-scanning the middle/tail literal below, so we
        // clear the flag for the duration of the substitution and restore it for
        // the literal-span re-scan that follows.
        let saved_in_tagged_template = self.in_tagged_template;
        self.in_tagged_template = false;
        let expression = self.parse_expression();
        self.in_tagged_template = saved_in_tagged_template;
        self.context_flags = saved_flags;
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
                    let end = self.token_end();
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
                    has_invalid_escape: false,
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
                    has_invalid_escape: false,
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
        let raw_text = Some(self.scanner.get_token_text_ref().to_string());
        let has_invalid_escape =
            (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32) != 0;
        let literal_kind = if is_tail {
            SyntaxKind::TemplateTail
        } else {
            SyntaxKind::TemplateMiddle
        };

        let literal_end = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.next_token();

        if is_unterminated {
            self.report_unterminated_template_recovery_delimiters(literal_start, literal_end);
        }

        let literal = self.arena.add_literal(
            literal_kind as u16,
            literal_start,
            literal_end,
            LiteralData {
                text: literal_text,
                raw_text,
                value: None,
                has_invalid_escape,
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

    fn report_unterminated_template_recovery_delimiters(&mut self, start: u32, end: u32) {
        let source = self.get_source_text();
        let Some(source_tail) = source.get(start as usize..end as usize) else {
            return;
        };

        let Some(backtick_before_comma) = source_tail.rfind("`,") else {
            return;
        };

        // Suppress synthetic recovery markers when the recovered tail
        // template's content (between its opening backtick and
        // `backtick_before_comma`) contains a `${` interpolation. tsc
        // treats interpolated tail templates as continuation of the outer
        // unterminated template's text and does NOT surface synthetic
        // markers — only plain tail templates yield TS1005 markers.
        // Difference between
        // `labeledStatementDeclarationListInLoopNoCrash3.ts`
        // (interpolated — no markers) and `...NoCrash4.ts`
        // (plain — markers).
        // The `\`,` pattern's backtick is the CLOSING backtick of a tail
        // template literal `\`<text>\`,`. Find that template's OPENING
        // backtick and check whether its content contains `${`. The
        // opening may be inside the tail (e.g., `...\`a\`,...\`b\`,`) OR
        // before `start` (when the closing backtick at position 0 of the
        // tail belongs to a template that started before `start`). For
        // the latter, walk the source backwards from `start` to locate
        // the opening.
        //
        // tsc emits the synthetic recovery markers only for plain
        // (non-interpolated) tail templates. If the recovered template
        // segment contains `${`, suppress the markers — matching
        // `labeledStatementDeclarationListInLoopNoCrash3.ts` (interpolated
        // tail) vs `...NoCrash4.ts` (plain tail).
        let abs_close_backtick = start as usize + backtick_before_comma;
        let opening_backtick = source[..abs_close_backtick].rfind('`');
        let opening_backtick_segment_has_interp = opening_backtick
            .is_some_and(|open| source[open + 1..abs_close_backtick].contains("${"));
        if opening_backtick_segment_has_interp {
            return;
        }

        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at(
            start + backtick_before_comma as u32,
            1,
            "',' expected.",
            diagnostic_codes::EXPECTED,
        );
        self.parse_error_at(end, 0, "'}' expected.", diagnostic_codes::EXPECTED);
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

    /// Parse the template portion of a tagged template, marking the parser as
    /// being inside a tagged template so the immediate head/middle/tail literals
    /// suppress invalid-escape errors (TS1125), which tagged templates permit
    /// since ES2018.
    ///
    /// The `in_tagged_template` flag is saved and restored rather than reset to
    /// `false`, so this works correctly under nesting: a tagged template that
    /// appears inside another tagged template's substitution must not clobber the
    /// enclosing template's tagged status when it finishes (which would otherwise
    /// make the enclosing template's middle/tail literals spuriously report
    /// TS1125). Substitution expressions themselves are parsed with the flag
    /// cleared in `parse_template_expression_span`, matching tsc, where
    /// `isTaggedTemplate` is re-applied only when re-scanning the literal spans.
    pub(crate) fn parse_tagged_template_literal(&mut self) -> NodeIndex {
        let saved_in_tagged_template = self.in_tagged_template;
        self.in_tagged_template = true;
        let template = self.parse_template_literal();
        self.in_tagged_template = saved_in_tagged_template;
        template
    }

    /// Parse parenthesized expression
    pub(crate) fn parse_parenthesized_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let saved_context_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION;
        self.parse_expected(SyntaxKind::OpenParenToken);
        let recovered_const_for_of_header = self.is_recovered_const_for_of_header_start();
        let expression = if recovered_const_for_of_header {
            self.create_missing_expression()
        } else {
            self.parse_expression()
        };
        if expression.is_none() {
            // Emit TS1109 for empty parentheses or invalid expression: ([missing])
            self.error_expression_expected();
        }
        let end_pos = self.token_end();
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_expected(SyntaxKind::CloseParenToken);
        } else if !recovered_const_for_of_header {
            use tsz_common::diagnostics::diagnostic_codes;
            if self.should_report_error() {
                self.parse_error_at_current_token("')' expected.", diagnostic_codes::EXPECTED);
            }
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
        let mut recovered_empty_array_end = None;
        let mut has_trailing_comma = false;
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
                let expression = self.parse_assignment_expression_allowing_arrow_return_type();
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
                let elem = self.parse_assignment_expression_allowing_arrow_return_type();
                if elem.is_none() {
                    // tsc uses TS1137 ("Expression or comma expected") when a closing
                    // delimiter from an outer context terminates the array (e.g. `[)` or
                    // `[...}\n}`), and TS1109 ("Expression expected") otherwise.
                    let terminated_by_outer_closer = matches!(
                        self.token(),
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBraceToken
                    );
                    if terminated_by_outer_closer {
                        self.error_expression_or_comma_expected();
                        if elements.is_empty() {
                            recovered_empty_array_end = Some(start_pos + 1);
                        }
                    } else {
                        self.error_expression_expected();
                    }
                    // Continue parsing with empty element for error recovery
                    if terminated_by_outer_closer {
                        break;
                    }
                }
                elements.push(elem);
            }

            if self.parse_optional(SyntaxKind::CommaToken) {
                // A separator followed directly by the closer is a trailing
                // comma; the checker needs this for the TS1013 rest-element
                // rule on destructuring assignment targets (`[...c,] = a`).
                if self.is_token(SyntaxKind::CloseBracketToken)
                    || self.is_token(SyntaxKind::EndOfFileToken)
                {
                    has_trailing_comma = true;
                }
            } else {
                if self.is_token(SyntaxKind::SemicolonToken) {
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token(); // look past `;`
                    // A `;` is not a valid array-element separator and cannot begin
                    // an array element. tsc's parseDelimitedList aborts the list at
                    // such a token. When the token following `;` could itself begin a
                    // fresh statement-level element list (an expression start or
                    // spread), terminate the array literal at the `;` so the remainder
                    // re-parses as a separate statement, matching tsc's recovery node
                    // shape. The `;` is left unconsumed for the outer parser.
                    let next_can_begin_element =
                        self.is_expression_start() || self.is_token(SyntaxKind::DotDotDotToken);
                    let next_is_close_bracket = self.is_token(SyntaxKind::CloseBracketToken);
                    let follows_eof = self.is_token(SyntaxKind::EndOfFileToken);
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;

                    if next_can_begin_element {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        // Terminate the list at the `;`; do not consume it. The
                        // enclosing statement parser handles the trailing tokens.
                        break;
                    }

                    if next_is_close_bracket {
                        // `[a ;]` — a trailing `;` immediately before the closer is a
                        // mistyped comma rather than a list terminator. Report the
                        // missing comma, consume the `;`, and let the loop close the
                        // array on the `]`.
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
                    // Match tsc's parseDelimitedList: when the array is terminated by an
                    // outer-context closer (e.g. `)` from an enclosing call, `}` from a
                    // block), report "',' expected" first. parse_expected(]) then runs at
                    // the same position and gets dedup'd, so the user sees the comma
                    // diagnostic that tsc would produce instead of a "']' expected" that
                    // points the user at the wrong fix.
                    if matches!(
                        self.token(),
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBraceToken
                    ) {
                        // `error_comma_expected()` suppresses the comma when the
                        // closer is `)` immediately followed by `;` (a shape it
                        // treats as call-argument / arrow recovery). For an array
                        // literal terminated by `)` — e.g. `a0([1, 2);` where the
                        // inner `[` is never closed — tsc still reports the missing
                        // separator before the closer, so suppressing it would leak
                        // a misleading `']' expected` from `parse_expected(])` below.
                        // Emit `',' expected` directly for that case to match tsc.
                        let paren_then_semicolon = self.is_token(SyntaxKind::CloseParenToken)
                            && self.speculate(|parser| {
                                parser.next_token();
                                parser.is_token(SyntaxKind::SemicolonToken)
                            });
                        if paren_then_semicolon {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        } else {
                            self.error_comma_expected();
                        }
                    }
                    break;
                }
            }
        }

        if emit_semicolon_expected_at_close_bracket && self.is_token(SyntaxKind::CloseBracketToken)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
        }

        let end_pos = recovered_empty_array_end.unwrap_or_else(|| self.token_end());
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: {
                    let mut list = Self::make_node_list(elements);
                    list.has_trailing_comma = has_trailing_comma;
                    list
                },
                multi_line: false,
            },
        )
    }

    /// Parse new expression
    pub(crate) fn parse_new_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Handle new.target meta-property
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            let new_node = self.synth_keyword_token(SyntaxKind::NewKeyword, start_pos);
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
        if expression.is_none() {
            self.error_expression_expected();
        }
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
                question_dot_token: false,
            },
        )
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
                    let name = if let Some(name) = self.parse_private_identifier_or_bare_hash() {
                        name
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
                    let missing_argument_start = self.token_end();
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // TS1011: An element access expression should take an argument
                        let current_start = self.token_pos();
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
                    let template = self.parse_tagged_template_literal();
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
