impl ParserState {
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
                && (self.should_report_error() || self.last_error_was_leading_zero_at_other_pos())
            {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }
            return;
        };

        if self.parse_missing_semicolon_keyword_error(pos, len, &expression_text) {
            return;
        }

        if self.should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
            expression_text.as_str(),
            pos,
        ) {
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

        // If the expression text is already an exact keyword (e.g., `from`, `get`, `set`),
        // the identifier appeared in error recovery from an upstream parse failure.
        // Emitting TS1434 "Unexpected keyword or identifier" here is a cascade artifact —
        // the real error was already reported. tsc suppresses this via different parsing
        // flow that doesn't reach this fallback for exact keywords.
        if spelling::VIABLE_KEYWORD_SUGGESTIONS
            .iter()
            .any(|&kw| kw == expression_text)
        {
            if matches!(
                self.token(),
                SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
            ) {
                self.parse_error_at(
                    pos,
                    len,
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
                return;
            }
            // Keep the suppression for bare keyword recovery, but allow keyword-like
            // statements followed by a literal (notably `from "./mod"`) to report
            // TS1434 like tsc does.
            if !matches!(
                self.token(),
                SyntaxKind::StringLiteral
                    | SyntaxKind::NoSubstitutionTemplateLiteral
                    | SyntaxKind::TemplateHead
            ) {
                return;
            }
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
