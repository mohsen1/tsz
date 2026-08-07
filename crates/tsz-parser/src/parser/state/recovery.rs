use super::*;

impl ParserState {
    pub(crate) fn parse_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        if code == tsz_common::diagnostics::diagnostic_codes::EXPECTED
            && message == "')' expected."
            && self.is_token(SyntaxKind::CloseParenToken)
            && start == self.token_pos()
            && self.speculate(|parser| {
                parser.next_token();
                parser.is_token(SyntaxKind::SemicolonToken)
            })
        {
            return;
        }
        if code == tsz_common::diagnostics::diagnostic_codes::EXPECTED
            && message == "')' expected."
            && self.get_source_text().as_bytes().get(start as usize) == Some(&b')')
            && self.get_source_text().as_bytes().get(start as usize + 1) == Some(&b';')
        {
            return;
        }
        if let Some(last) = self.parse_diagnostics.last()
            && last.start == start
        {
            self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
            return;
        }
        let scanner_diags = self.scanner.get_scanner_diagnostics();
        if scanner_diags.len() > self.scanner_diagnostics_high_water_mark
            && let Some(last_scanner) = scanner_diags.last()
            && self.u32_from_usize(last_scanner.pos) == start
        {
            return;
        }
        // Track the position of this error to prevent cascading errors at same position
        self.last_error_pos = start;
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
            related: None,
        });
        // After pushing a parser diagnostic, the effective "lastError" is
        // ours; subsequent scanner emissions reset the comparison frame.
        self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
    }

    /// Like [`Self::parse_error_at`], but attaches a single `relatedInformation`
    /// pointer (`tsc`: `DiagnosticRelatedInformation`) into the same file —
    /// e.g. TS1486 `Decorator used before 'export' here.` alongside TS8038.
    /// Bypasses `parse_error_at`'s same-position/scanner dedup: a diagnostic
    /// that carries related info is always a deliberate single emission from
    /// its call site, not a cascade the dedup heuristics need to guard.
    pub(crate) fn parse_error_at_with_related(
        &mut self,
        start: u32,
        length: u32,
        message: &str,
        code: u32,
        related: ParseDiagnosticRelated,
    ) {
        self.last_error_pos = start;
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
            related: Some(Box::new(related)),
        });
        self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
    }

    /// Report parse error at current token with specific error code
    fn recover_after_reserved_word_in_variable_declaration(&mut self, keyword: SyntaxKind) {
        use tsz_common::diagnostics::diagnostic_codes;

        self.next_token();

        // In tsc, `var class;` causes the variable declaration list to abort, then the
        // statement loop reparses `class` as a class declaration which expects `{` but
        // finds `;`, emitting TS1005 '{' expected.' at the semicolon. We emit this
        // directly, then consume the reserved word so the declaration parser can move on.
        if keyword == SyntaxKind::ClassKeyword && self.is_token(SyntaxKind::SemicolonToken) {
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
        } else if keyword == SyntaxKind::ExportKeyword && self.is_token(SyntaxKind::AsKeyword) {
            // `const export as namespace oo4;` — tsc recovers by re-parsing the
            // trailing `as namespace <id>` as the export-as-namespace syntax and
            // emits no further diagnostic beyond TS1389 on `export`.  Silently
            // consume the `as namespace <id>` tail so we don't cascade into
            // "';' expected." at `as`.
            self.next_token(); // consume `as`
            if self.is_token(SyntaxKind::NamespaceKeyword) {
                self.next_token(); // consume `namespace`
                if self.is_identifier_or_keyword() {
                    self.next_token();
                }
            }
        } else if keyword == SyntaxKind::TypeOfKeyword {
            if !self.is_expression_start() {
                // `var typeof;` → TS1109 because `;` can't start an expression.
                self.error_expression_expected();
            } else if self.is_token(SyntaxKind::OpenParenToken) {
                // `var typeof(x);` → skip the parenthesized expression to avoid extra TS1005.
                // TSC reparses `typeof(x)` as a typeof expression, consuming the operand.
                let mut paren_depth = 0u32;
                while !matches!(
                    self.token(),
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CloseBraceToken
                        | SyntaxKind::EndOfFileToken
                ) && !self.scanner.has_preceding_line_break()
                {
                    match self.token() {
                        SyntaxKind::OpenParenToken => paren_depth += 1,
                        SyntaxKind::CloseParenToken => {
                            if paren_depth == 0 {
                                break;
                            }
                            paren_depth -= 1;
                        }
                        _ => {}
                    }
                    self.next_token();
                    if paren_depth == 0 {
                        break;
                    }
                }
            }
        }
    }

    /// Error: TS1389 - '{0}' is not allowed as a variable declaration name.
    /// Emitted when a reserved word appears as the binding name of a var/let/const/using declaration.
    ///
    /// In tsc, the reserved word is NOT consumed — the variable declaration list aborts and the
    /// keyword is reparsed by the statement loop. For `var class;`, this means `class` gets parsed
    /// as a class declaration, which then emits TS1005 `'{' expected.` when it finds `;`.
    /// We consume the token to avoid complex recovery differences, but explicitly emit the TS1005
    /// that tsc would produce when `class` is the keyword (since the class declaration would
    /// expect `{` at the semicolon position).
    pub(crate) fn error_reserved_word_in_variable_declaration(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            let keyword = self.token();
            let word = self.current_keyword_text();
            let msg = diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                .replace("{0}", word);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
            );
            self.recover_after_reserved_word_in_variable_declaration(keyword);
        }
    }

    /// Error: TS1390 - '{0}' is not allowed as a parameter name.
    ///
    /// For a few legacy keyword parameter forms, tsc also emits a companion parser
    /// diagnostic during recovery. We mirror that shape here to avoid falling through
    /// to checker-only diagnostics such as TS7006.
    pub(crate) fn error_reserved_word_in_parameter_name(&mut self) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let keyword = self.token();
        if self.should_report_error() {
            let word = self.current_keyword_text();
            let msg = diagnostic_messages::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME.replace("{0}", word);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME,
            );
        }

        // Consume the reserved word so companion recovery diagnostics are anchored at
        // the following token position (matching tsc's reserved-parameter recovery).
        self.next_token();

        // Match tsc recovery for common reserved parameter names:
        //   enum/function -> TS1003 at the following token (typically ')')
        //   class         -> TS1005 "'{' expected." at the following token
        //   while/for     -> TS1005 "'(' expected." at the following token
        match keyword {
            SyntaxKind::EnumKeyword | SyntaxKind::FunctionKeyword => {
                self.parse_error_at_current_token(
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
            SyntaxKind::ClassKeyword => {
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            }
            SyntaxKind::WhileKeyword | SyntaxKind::ForKeyword => {
                self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            }
            _ => {}
        }
    }

    pub(crate) const fn is_statement_tail_reserved_parameter_keyword(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::EnumKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::ForKeyword
        )
    }

    /// Report TS1390 and the companion parser recovery diagnostic for hard
    /// reserved parameter names, but leave the keyword in the token stream so
    /// statement-level recovery can parse the tail just like `tsc`.
    pub(crate) fn error_reserved_word_in_parameter_name_without_consuming(&mut self) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let keyword = self.token();
        let keyword_end = self.token_end();
        if self.should_report_error() {
            let word = self.current_keyword_text();
            let msg = diagnostic_messages::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME.replace("{0}", word);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME,
            );
        }

        match keyword {
            SyntaxKind::EnumKeyword | SyntaxKind::FunctionKeyword => {
                self.parse_error_at(
                    keyword_end,
                    1,
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
            SyntaxKind::ClassKeyword => {
                self.parse_error_at(keyword_end, 1, "'{' expected.", diagnostic_codes::EXPECTED);
            }
            SyntaxKind::WhileKeyword | SyntaxKind::ForKeyword => {
                self.parse_error_at(keyword_end, 1, "'(' expected.", diagnostic_codes::EXPECTED);
            }
            _ => {}
        }
    }

    /// Error: TS1359 - Identifier expected. '{0}' is a reserved word that cannot be used here.
    pub(crate) fn error_reserved_word_identifier(&mut self) {
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            let word = self.current_keyword_text();
            if self.is_token(SyntaxKind::YieldKeyword) && self.in_generator_context() {
                self.report_yield_reserved_word_error();
                // Consume the reserved word token to prevent cascading errors
                self.next_token();
                return;
            }
            self.parse_error_at_current_token(
                &format!(
                    "Identifier expected. '{word}' is a reserved word that cannot be used here."
                ),
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
            // Consume the reserved word token to prevent cascading errors
            self.next_token();
        }
    }

    /// Error: '{token}' expected (TS1005)
    pub(crate) fn error_token_expected(&mut self, token: &str) {
        // When the current token is Unknown (invalid character), emit only TS1127.
        // In tsc, the scanner emits TS1127 into parseDiagnostics via scanError callback
        // *before* the parser's parseExpected runs. Since tsc's parseErrorAtPosition dedup
        // suppresses errors at the same position as the last error, the parser's TS1005 is
        // always shadowed by the scanner's TS1127. We replicate this by emitting only TS1127.
        if self.is_token(SyntaxKind::Unknown) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                diagnostic_codes::INVALID_CHARACTER,
            );
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when parse_semicolon() and similar functions call this
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                &format!("'{token}' expected."),
                diagnostic_codes::EXPECTED,
            );
        }
    }

    /// Check if current token could start a parameter
    pub(crate) fn is_parameter_start(&mut self) -> bool {
        // Parameters can start with modifiers, identifiers, or binding patterns
        self.is_parameter_modifier()
            || self.is_token(SyntaxKind::AtToken) // decorators on parameters
            || self.is_token(SyntaxKind::DotDotDotToken) // rest parameter
            || self.is_identifier_or_keyword()
            || self.is_token(SyntaxKind::OpenBraceToken) // object binding pattern
            || self.is_token(SyntaxKind::OpenBracketToken) // array binding pattern
    }

    /// Check whether the current token can begin a parameter, mirroring tsc's
    /// `isStartOfParameter`.
    ///
    /// A parameter slot tolerates not only binding identifiers, patterns,
    /// modifiers, decorators, and `...` rest syntax, but also any token that can
    /// begin a TYPE. tsc parses those type-leading tokens as a (malformed)
    /// parameter name and reports TS1003 `Identifier expected.`; only when the
    /// current token can begin neither a parameter nor a type does tsc emit
    /// TS1138 `Parameter declaration expected.` and skip the slot. Keeping this
    /// distinction is what stops an empty slot delimiter such as the stray `,`
    /// in `(a, , b)` from falling through to the identifier/expression path.
    pub(crate) fn is_start_of_parameter(&mut self) -> bool {
        if self.is_parameter_start() {
            return true;
        }
        // Keyword-led type starts (`number`, `string`, `this`, `typeof`,
        // `keyof`, `new`, `import`, `object`, `true`, `null`, ...) are already
        // covered by `is_parameter_start` via `is_identifier_or_keyword`. The
        // remaining type-start tokens are punctuation/literals that sort below
        // `Identifier`, so enumerate them explicitly.
        matches!(
            self.current_token,
            SyntaxKind::StringLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::OpenParenToken
                | SyntaxKind::MinusToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::BarToken
                | SyntaxKind::AmpersandToken
        )
    }

    /// Error: Unterminated template literal (TS1160)
    ///
    /// tsc reports this error at the END of the template content (where EOF was hit),
    /// not at the start (the backtick). We match that behavior.
    pub(crate) fn error_unterminated_template_literal_at(&mut self, _start: u32, end: u32) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_companion_error_at(
            end,
            1,
            "Unterminated template literal.",
            diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL,
        );
    }

    /// Error: Declaration expected (TS1146)
    pub(crate) fn error_declaration_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Declaration expected.",
            diagnostic_codes::DECLARATION_EXPECTED,
        );
    }

    /// Error: Statement expected (TS1129)
    pub(crate) fn error_statement_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Statement expected.",
            diagnostic_codes::STATEMENT_EXPECTED,
        );
    }

    /// Check if a statement is a using/await using declaration not inside a block (TS1156)
    pub(crate) fn check_using_outside_block(&mut self, statement: NodeIndex) {
        use crate::parser::node_flags;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if statement.is_none() {
            return;
        }

        // Get the node and check if it's a variable statement with using flags
        if let Some(node) = self.arena.get(statement) {
            // Check if it's a variable statement (not a block)
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                // Check if it has using or await using flags
                let is_using = (node.flags
                    & self.u16_from_node_flags(node_flags::USING | node_flags::AWAIT_USING))
                    != 0;
                if is_using {
                    // Emit TS1156 error at the statement position
                    self.parse_error_at(
                        node.pos,
                        node.end.saturating_sub(node.pos).max(1),
                        diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                        diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                    );
                }
            }
        }
    }

    /// Parse semicolon (or recover from missing)
    pub(crate) fn parse_semicolon(&mut self) {
        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        } else if self.is_token(SyntaxKind::Unknown) {
            // Scanner/lexer already reported an error for this token.
            // Avoid cascading TS1005 (';' expected) at the same position.
        } else if !self.can_parse_semicolon() {
            // Suppress cascading TS1005 "';' expected" when a recent error was already
            // emitted. This happens when a prior parse failure (e.g., missing identifier,
            // unsupported syntax) causes the parser to not consume tokens, then
            // parse_semicolon is called and fails too.
            // Use centralized error suppression heuristic
            if self.should_report_error() {
                self.error_token_expected(";");
            }
        }
    }

    // =========================================================================
    // Keyword suggestion for misspelled keywords (TS1434/TS1435/TS1438)
    // =========================================================================

    /// Provides a better error message than the generic "';' expected" for
    /// known common variants of a missing semicolon, such as misspelled keywords.
    ///
    /// Matches TypeScript's `parseErrorForMissingSemicolonAfter`.
    ///
    /// `expression` is the node index of the expression that was parsed before
    /// the missing semicolon.
    pub(crate) fn parse_error_for_missing_semicolon_after(&mut self, expression: NodeIndex) {
        use crate::parser::spelling;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some((pos, len, expression_text)) =
            self.missing_semicolon_after_expression_text(expression)
        else {
            // For non-identifier expressions (postfix, literals, etc.),
            // emit a plain TS1005 ";' expected" via parse_error_at which
            // deduplicates by exact start position (matching tsc's
            // parseErrorAtCurrentToken). We emit TS1005 even when the expression
            // had prior errors (like TS1121 for octal literals), matching tsc
            // behavior for cases like `00.5;` where both errors should be reported.
            // Suppress cascading TS1005 when a recent error was emitted nearby —
            // except when the prior error was a leading-zero diagnostic
            // (TS1121/TS1489) at a different position. Those are orthogonal to
            // the missing-semicolon error: tsc's `parseErrorAtPosition` dedups
            // only by exact start, so `00.5;` reports TS1121 at col 1 AND
            // TS1005 at col 3.
            //
            // Also dedup by exact start against a recent diagnostic: tsc emits
            // the missing-semicolon error via `parseErrorAtPosition`, which drops
            // it when the previous diagnostic shares its start. A recovered
            // construct can anchor a diagnostic exactly at this token and leave
            // the token to reparse (e.g. a mismatched JSX closing fragment
            // `<>...</div>` reports TS17015 at `div`, then `div` reparses as the
            // next statement). tsz may push another recovery diagnostic in
            // between, so scan the most recent few by exact position.
            let token_pos = self.token_pos();
            let already_reported_here = self
                .parse_diagnostics
                .iter()
                .rev()
                .take(4)
                .any(|diag| diag.start == token_pos);
            if !already_reported_here
                && (self.should_report_error()
                    || self.last_error_was_leading_zero_at_other_pos()
                    || self.last_error_was_element_access_missing_argument_at_other_pos())
            {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }
            return;
        };

        if self.parse_missing_semicolon_keyword_error(pos, len, &expression_text) {
            return;
        }

        if let Some(suggestion) = spelling::suggest_keyword(&expression_text) {
            if suggestion == "this" && self.is_token(SyntaxKind::DotToken) {
                self.parse_error_at(
                    pos,
                    len,
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
                return;
            }
            if !self.should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
                suggestion.as_str(),
                pos,
            ) {
                self.parse_error_at(
                    pos,
                    len,
                    &format!("Unknown keyword or identifier. Did you mean '{suggestion}'?"),
                    diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN,
                );
            }

            return;
        }

        if self.is_token(SyntaxKind::Unknown) {
            return;
        }

        // An exact-keyword identifier (e.g. `is`, `from`) immediately followed
        // by a closing delimiter still gets TS1434 in tsc — that shape shows up
        // in nested expression-recovery (e.g. a failed type-predicate/assertion
        // parse) where the keyword is not itself a cascade artifact. This is
        // the one case where the general `)`/`]` suppression below does not
        // apply to keyword-exact text.
        if matches!(
            self.token(),
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
        ) && spelling::VIABLE_KEYWORD_SUGGESTIONS
            .iter()
            .any(|&kw| kw == expression_text)
        {
            self.parse_error_at(
                pos,
                len,
                diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            );
            return;
        }

        // tsc emits TS1434 "Unexpected keyword or identifier" at the expression
        // position for any identifier that isn't a recognized keyword/type.
        // Suppress when the following token is a closing delimiter (`)`, `]`)
        // that cannot start a new statement — the identifier is part of
        // cascading recovery from an earlier syntax error, not a standalone
        // statement missing a semicolon.
        if matches!(
            self.token(),
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
        ) {
            return;
        }
        self.parse_error_at(
            pos,
            len,
            diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
        );
    }
}
