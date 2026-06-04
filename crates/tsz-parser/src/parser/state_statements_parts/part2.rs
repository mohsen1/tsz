impl ParserState {
    /// Parse variable declaration list
    pub(crate) fn parse_variable_declaration_list(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();

        // Consume var/let/const/using/await using and get flags
        // Use consume_keyword() for TS1260 check (keywords cannot contain escape characters)
        let flags: u16 = match self.token() {
            SyntaxKind::LetKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::LET)
            }
            SyntaxKind::ConstKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::CONST)
            }
            SyntaxKind::UsingKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::USING)
            }
            SyntaxKind::AwaitKeyword => {
                // await using declaration
                self.consume_keyword(); // consume 'await'
                self.parse_expected(SyntaxKind::UsingKeyword); // consume 'using'
                self.u16_from_node_flags(node_flags::AWAIT_USING)
            }
            _ => {
                self.consume_keyword(); // var
                0
            }
        };

        // Parse declarations with enhanced error recovery
        let mut declarations = Vec::new();
        let mut had_decl_expected_error = false;
        loop {
            // Check if we can start a variable declaration
            // Can be: identifier, keyword as identifier, or binding pattern (object/array)
            let starts_recovered_invalid_unicode_identifier =
                self.current_unknown_starts_invalid_unicode_identifier_debris();
            let can_start_decl = self.is_identifier_or_keyword()
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken)
                || self.is_token(SyntaxKind::PrivateIdentifier)
                || starts_recovered_invalid_unicode_identifier;

            if !can_start_decl {
                if self.is_token(SyntaxKind::Unknown) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                    self.next_token();

                    if self.is_identifier_or_keyword() && !self.is_reserved_word() {
                        continue;
                    }

                    if self.is_token(SyntaxKind::ColonToken) {
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        while !matches!(
                            self.token(),
                            SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        ) {
                            self.next_token();
                        }
                        had_decl_expected_error = true;
                    }
                    break;
                }

                // Invalid token for variable declaration - emit error and recover
                if !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::Unknown)
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                    had_decl_expected_error = true;
                }
                break;
            }

            let decl_started_at_numeric_literal_follow_error =
                self.current_token_has_numeric_literal_follow_error();
            let diag_count_before_decl = self.parse_diagnostics.len();
            let decl = self.parse_variable_declaration_with_flags(flags);
            let decl_had_error = self.parse_diagnostics.len() > diag_count_before_decl;
            // A declarator with ONLY numeric-literal-value errors (TS1121
            // legacy octal, TS1352/TS1353 bigint form, TS1489 leading-zero
            // decimal, etc.) is structurally complete — only the literal's
            // value is illegal. The next token can still kick off a missing
            // -comma recovery for the declaration list. Track this so the
            // post-decl loop can distinguish from genuine declarator-shape
            // errors (malformed name, malformed initializer expression).
            let decl_only_literal_value_errors = decl_had_error
                && {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_diagnostics[diag_count_before_decl..]
                    .iter()
                    .all(|d| {
                        matches!(
                            d.code,
                            diagnostic_codes::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX
                                | diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED
                                | diagnostic_codes::BINARY_DIGIT_EXPECTED
                                | diagnostic_codes::OCTAL_DIGIT_EXPECTED
                                | diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL
                                | diagnostic_codes::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                                | diagnostic_codes::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                                | diagnostic_codes::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED
                                | diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE
                                | diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED
                        )
                    })
                };
            declarations.push(decl);

            let comma_pos = self.token_pos();
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // If ASI applies (line break, closing brace, EOF, or semicolon),
                // just break - parse_semicolon() in the caller will handle it
                if self.can_parse_semicolon() {
                    break;
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    use tsz_common::diagnostics::diagnostic_codes;

                    // A `:` that follows an initializer recovered from a
                    // template-literal-as-property-name object literal (e.g.
                    // `var x = {} \`tpl\` : 321`) is never a type annotation.
                    // tsc closes the object literal at the template, attaches
                    // the template as a tagged-template tail, then treats the
                    // `:` as a missing comma between declarators: it reports
                    // TS1005 at the `:` and retries the declarator list at the
                    // next token. That next token then either starts a new
                    // declarator or, when it cannot (e.g. a numeric literal),
                    // yields TS1134 and is left for the surrounding statement
                    // parser. Recover the same way here so the AST — and emit
                    // — match tsc (tagged-template initializer plus a separate
                    // trailing statement) instead of swallowing the value as a
                    // type. Gated on the dedicated recovery flag so it does not
                    // perturb other `:`-after-initializer shapes such as
                    // failed-arrow recovery (`var y = x:number => x*x`).
                    if self.recovered_template_literal_property_in_object {
                        self.recovered_template_literal_property_in_object = false;
                        // Bypass the distance-based suppression gate (the `:`
                        // can sit within `ERROR_SUPPRESSION_DISTANCE` of the
                        // prior TS1136 from the recovered template-literal
                        // property name); tsc dedups only on exact position,
                        // and the `:` is at a distinct position.
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token(); // consume `:`
                        continue;
                    }

                    let recover_invalid_jsx_namespace_head =
                        self.recover_jsx_invalid_namespace_head_tail;
                    if self.recover_jsx_closing_tag_extra_namespace_tail
                        || recover_invalid_jsx_namespace_head
                    {
                        let snapshot = self.scanner.save_state();
                        let current = self.current_token;
                        self.next_token();
                        let colon_followed_by_declaration =
                            self.is_identifier_or_keyword() && !self.is_reserved_word();
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;

                        if colon_followed_by_declaration {
                            self.next_token();
                            if recover_invalid_jsx_namespace_head {
                                self.recover_jsx_invalid_namespace_head_tail = false;
                            }
                            continue;
                        }
                    }

                    let use_failed_async_arrow_recovery =
                        self.pending_failed_async_arrow_colon_recovery;
                    self.pending_failed_async_arrow_colon_recovery = false;

                    self.error_comma_expected();
                    self.next_token();

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        self.parse_error_at_current_token(
                            "';' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        break;
                    }

                    let generic_like_type_arg_pos =
                        if use_failed_async_arrow_recovery && self.is_identifier_or_keyword() {
                            let snapshot = self.scanner.save_state();
                            let current = self.current_token;
                            self.next_token();
                            let result = self
                                .is_token(SyntaxKind::LessThanToken)
                                .then(|| self.token_pos());
                            self.scanner.restore_state(snapshot);
                            self.current_token = current;
                            result
                        } else {
                            None
                        };

                    let recover_start = self.token_pos();
                    let _ = self.parse_type();
                    if self.token_pos() == recover_start
                        && !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        )
                    {
                        self.next_token();
                    }

                    if let Some(pos) = generic_like_type_arg_pos {
                        self.parse_error_at(pos, 1, "',' expected.", diagnostic_codes::EXPECTED);
                    }

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        if use_failed_async_arrow_recovery {
                            self.error_expression_expected();
                        } else {
                            self.parse_error_at_current_token(
                                "';' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        }
                    }
                    break;
                }

                // `=>` after a declaration is never a valid comma separator.
                // Break silently so parse_semicolon() in the caller can emit
                // "';' expected." at the `=` position, matching tsc's diagnostic.
                // Example: `var tt = (a, (b, c)) => ...` — rejected arrow function.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    if self.in_static_block_context() {
                        let arrow_pos = self.token_pos();
                        let already_reported_expression_at_arrow =
                            self.parse_diagnostics.last().is_some_and(|diag| {
                                diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                                    && diag.start == arrow_pos
                            });
                        if !already_reported_expression_at_arrow {
                            self.parse_error_at_current_token(
                                "';' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        }
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenBraceToken) {
                            self.parse_block();
                        } else if self.is_expression_start() {
                            self.parse_assignment_expression();
                        }
                    }
                    break;
                }

                // When the variable name itself was erroneous (e.g., TS1389 for a
                // reserved word like `const export`), stop this declaration list so
                // the statement loop can reparse the keyword in the tsc-shaped way.
                //
                // Carve-out: when the only error came from the initializer's
                // value (e.g. TS1121 on legacy octal `0123n` — the scanner
                // returns `0123` as a complete numeric literal and leaves `n`
                // as a separate identifier token), the declarator itself is
                // structurally complete. Let the missing-comma recovery below
                // (the can_continue branch) treat the next token as the start
                // of a new declarator so the `n` produces TS1005 "',' expected"
                // at the right position, matching tsc.
                if decl_had_error && !self.is_token(SyntaxKind::CloseBracketToken) {
                    let recover_typeof_object_target_as_declarator = self
                        .is_token(SyntaxKind::OpenBraceToken)
                        && self.variable_declaration_has_missing_typeof_target(decl);
                    let next_starts_declarator = (self.is_identifier_or_keyword()
                        && !self.is_reserved_word())
                        || self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::OpenBracketToken)
                        || self.is_token(SyntaxKind::PrivateIdentifier)
                        || self.current_unknown_starts_invalid_unicode_identifier_debris();
                    if !(next_starts_declarator
                        && (decl_only_literal_value_errors
                            || self.is_token(SyntaxKind::PrivateIdentifier)
                            || recover_typeof_object_target_as_declarator))
                    {
                        break;
                    }
                }

                // `var v: void.x;` parses `void` as the type, then tsc reports
                // a missing comma at `.` and recovers `x` as a second declarator.
                let decl_has_type_annotation = self
                    .arena
                    .get(decl)
                    .and_then(|node| self.arena.get_variable_declaration(node))
                    .is_some_and(|decl| decl.type_annotation.is_some());
                if decl_has_type_annotation && self.is_token(SyntaxKind::DotToken) {
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let dot_followed_by_declaration =
                        self.is_identifier_or_keyword() && !self.is_reserved_word();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if dot_followed_by_declaration {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                        continue;
                    }
                }

                if decl_started_at_numeric_literal_follow_error
                    && self.is_token(SyntaxKind::OpenParenToken)
                {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);

                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    if self.is_token(SyntaxKind::CloseParenToken) {
                        self.parse_error_at_current_token(
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                    }
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;
                    break;
                }

                // If the unexpected token can start a new variable declaration
                // (identifier/keyword, { or [) AND is not a reserved word, treat
                // the missing comma as the only error and let the loop continue to
                // parse the token as the next declarator.
                // Example: `const a number = "missing colon";`
                //   tsc treats this as `const a, number = "missing colon";`
                //   and emits only one TS1005 at `number`.
                {
                    if self.current_unknown_starts_invalid_unicode_identifier_debris() {
                        continue;
                    }

                    let can_continue = (self.is_identifier_or_keyword()
                        && !self.is_reserved_word())
                        || self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::OpenBracketToken)
                        || self.is_token(SyntaxKind::PrivateIdentifier);
                    if can_continue {
                        // Emit ',' expected directly, bypassing the distance-based
                        // suppression heuristic. tsc's parseDelimitedList always
                        // emits TS1005 here (it only deduplicates at the exact same
                        // position). Without force-emit, two adjacent short
                        // identifiers (e.g. `var y: z is number;`) can fall within
                        // the suppression window and lose the second error.
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        continue;
                    }
                }

                // `var a₁ = "hello";` leaves an Unknown token for the subscript
                // character between the parsed identifier and `=`.
                // tsc recovers by treating the assignment tail as malformed
                // declaration syntax and reports TS1134 at `=` and again at
                // the initializer start, instead of bubbling out as TS1005 ';'
                // from parse_semicolon.
                if self.is_token(SyntaxKind::Unknown) {
                    if self.current_unknown_starts_braced_unicode_escape_debris() {
                        self.consume_braced_unicode_escape_debris_after_unknown();
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        continue;
                    }

                    let snapshot = self.scanner.save_state();
                    let current = self.current_token;
                    let unknown_text = self.scanner.get_token_text();
                    self.next_token();
                    let unknown_followed_by_equals = self.is_token(SyntaxKind::EqualsToken);
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;

                    if unknown_followed_by_equals {
                        self.parse_error_at_current_token(
                            "Invalid character.",
                            diagnostic_codes::INVALID_CHARACTER,
                        );
                        self.next_token(); // consume Unknown

                        if unknown_text.starts_with("\\u") {
                            if self.parse_optional(SyntaxKind::EqualsToken)
                                && !matches!(
                                    self.token(),
                                    SyntaxKind::SemicolonToken
                                        | SyntaxKind::CloseBraceToken
                                        | SyntaxKind::EndOfFileToken
                                )
                            {
                                self.parse_assignment_expression();
                            }
                            break;
                        }

                        if self.is_token(SyntaxKind::EqualsToken) {
                            self.parse_error_at_current_token(
                                "Variable declaration expected.",
                                diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                            );
                            self.next_token(); // consume '='

                            if !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) {
                                if self.is_token(SyntaxKind::NewKeyword) {
                                    let msg = tsz_common::diagnostics::diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                                        .replace("{0}", self.current_keyword_text());
                                    self.parse_error_at_current_token(
                                        &msg,
                                        diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                                    );
                                } else {
                                    self.parse_error_at_current_token(
                                        "Variable declaration expected.",
                                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                                    );
                                }
                            }
                        }
                        break;
                    }
                }

                if self.look_ahead_is_invalid_shebang() {
                    self.recover_invalid_shebang_token();
                    if self.is_token(SyntaxKind::ExclamationToken) {
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                    }
                    break;
                }

                if self.recover_jsx_closing_tag_extra_namespace_tail
                    && self.is_token(SyntaxKind::GreaterThanToken)
                {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    self.recover_jsx_closing_namespace_tail_greater_statement();
                    self.recover_jsx_closing_tag_extra_namespace_tail = false;
                    break;
                }

                // No ASI - emit ',' expected for the unexpected token and stop.
                // Use position-only dedup for normal tokens, not the broader
                // distance heuristic: tsc still reports adjacent declaration-list
                // comma errors like `var x: typeof function f() { };` at both
                // `f` and `(`. Keep Unknown tokens on the scanner-shaped TS1127
                // path instead of forcing TS1005.
                if self.is_token(SyntaxKind::Unknown) {
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                } else {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                }

                // Otherwise stop the list. We break instead of continuing to avoid
                // cascading TS1134 errors when the recovery eats into what tsc
                // treats as a separate statement.
                // Example: `var b = new C0 32, '';` - tsc emits only TS1005 at `32`.
                // Only consume the unexpected token if it cannot start a new
                // statement.  Tokens like `delete`, `typeof`, `void`, `~` etc.
                // can begin an expression statement and must be preserved so the
                // subsequent statement-parsing loop can emit them.
                // Example: `var a = q~;` → tsc emits `var a = q;\n~;`
                if !self.is_statement_start() {
                    let unexpected_token = self.token();
                    let decl_name_is_private_identifier = self
                        .arena
                        .get(decl)
                        .and_then(|node| self.arena.get_variable_declaration(node))
                        .and_then(|decl| self.arena.get(decl.name))
                        .is_some_and(|name| name.kind == SyntaxKind::PrivateIdentifier as u16);
                    // When a `.` separates what looks like two declarations
                    // (e.g., `const x: "".typeof(...)`), tsc treats the `.` as
                    // a missing `,` and continues the declaration list. When the
                    // next token is a keyword (e.g., `typeof`), tsc's list-parse
                    // error recovery emits TS1389 "not allowed as a variable
                    // declaration name". Emit the same diagnostic here, bypassing
                    // `error_reserved_word_in_variable_declaration` which would be
                    // suppressed by `should_report_error` proximity heuristic.
                    let was_dot = unexpected_token == SyntaxKind::DotToken;
                    self.next_token();
                    if matches!(
                        unexpected_token,
                        SyntaxKind::CloseBracketToken | SyntaxKind::CloseParenToken
                    ) && matches!(
                        self.token(),
                        SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken
                    ) {
                        // Keep malformed tails like `var v = /[]/]/` inside the
                        // declaration-list recovery so the trailing slash becomes
                        // TS1134 instead of a fresh unterminated regex statement.
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        self.next_token();
                    } else if unexpected_token == SyntaxKind::CloseBracketToken
                        && self.is_token(SyntaxKind::EqualsToken)
                    {
                        if decl_name_is_private_identifier {
                            self.parse_error_at_current_token(
                                "Variable declaration expected.",
                                diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                            );
                            let snapshot = self.scanner.save_state();
                            let saved_token = self.current_token;
                            self.next_token();
                            if !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) {
                                self.parse_error_at_current_token(
                                    "Variable declaration expected.",
                                    diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                                );
                            }
                            self.scanner.restore_state(snapshot);
                            self.current_token = saved_token;
                            break;
                        }
                        // `const x: C[#bar] = 3;` is recovered as a malformed
                        // declaration tail after `]`, producing TS1134 at `=`
                        // and at the initializer start (matching tsc).
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        self.next_token();
                        if !matches!(
                            self.token(),
                            SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        ) {
                            self.parse_error_at_current_token(
                                "Variable declaration expected.",
                                diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                            );
                            self.next_token();
                        }
                    }
                    if was_dot && token_is_keyword(self.token()) {
                        use tsz_common::diagnostics::diagnostic_messages;
                        let word = self.current_keyword_text();
                        let msg =
                            diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                                .replace("{0}", word);
                        self.parse_error_at_current_token(
                            &msg,
                            diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                        );
                        // Consume the reserved word and, if followed by a call tail
                        // like `typeof(this.foo)`, silently skip it. tsc stops after
                        // the TS1389 diagnostic without cascading TS1109/TS1005 into
                        // the trailing parentheses. Example:
                        //   `const x: "".typeof(this.foo);` → TS1005 at `.`, TS1389
                        //   at `typeof`, and nothing more.
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken)
                            && !self.scanner.has_preceding_line_break()
                        {
                            self.next_token(); // consume `(`
                            let mut paren_depth = 1u32;
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
                                        paren_depth -= 1;
                                        if paren_depth == 0 {
                                            self.next_token();
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                                self.next_token();
                            }
                        }
                    } else if was_dot && self.is_identifier_or_keyword() && !self.is_reserved_word()
                    {
                        // `declare const x: "foo".charCodeAt(0);` is recovered by tsc as if
                        // `charCodeAt` started a second declarator. Mirror that by surfacing
                        // the follow-up TS1005 at `(` and then skipping the call tail.
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                diagnostic_codes::EXPECTED,
                            );
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
                break;
            }

            // After comma, check if next token can start another declaration.
            // Handle cases like: let x, , y (missing declaration between commas).
            // Reserved words (return, if, while, etc.) cannot be binding identifiers,
            // so `var a, return;` should be a trailing comma error, not a new declaration.
            let can_start_next = (self.is_identifier_or_keyword() && !self.is_reserved_word())
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken)
                || self.is_token(SyntaxKind::PrivateIdentifier);

            if !can_start_next {
                // Trailing comma in variable declaration list — emit TS1009.
                // This covers `var a,;`, `var a,}`, `var a,` (EOF), and
                // `var a,\nreturn;` (reserved word after comma = trailing comma).
                use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                if self.is_token(SyntaxKind::SemicolonToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_reserved_word()
                {
                    self.parse_error_at(
                        comma_pos,
                        1,
                        diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                break;
            }
        }

        // Check for empty declaration list: var ;
        // TSC emits TS1123 "Variable declaration list cannot be empty"
        // Skip when TS1134 was already emitted (e.g., `using 1` — TSC only emits TS1134)
        if declarations.is_empty()
            && !had_decl_expected_error
            && !self.is_token(SyntaxKind::Unknown)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let pos = self.token_full_start();
            self.parse_error_at(
                pos,
                0,
                "Variable declaration list cannot be empty.",
                diagnostic_codes::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
            );
        }

        let end_pos = self.token_end();
        self.arena.add_variable_with_flags(
            syntax_kind_ext::VARIABLE_DECLARATION_LIST,
            start_pos,
            end_pos,
            VariableData {
                modifiers: None,
                declarations: self.make_node_list(declarations),
            },
            flags,
        )
    }

    /// Parse variable declaration with declaration flags (for using/await using checks)
    /// Flags: bits 0-2 used for LET/CONST/USING, bit 3 for catch-clause binding (suppresses TS1182)
    pub(crate) fn parse_variable_declaration_with_flags(&mut self, flags: u16) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_variable_declaration_with_flags_pre_checks(flags);

        // Clear any stale template-literal-property recovery signal so that it
        // can only reflect *this* declarator's initializer (e.g. an object
        // literal in a non-declaration context must not leak into a later
        // `var`).
        self.recovered_template_literal_property_in_object = false;

        let name = self.parse_variable_declaration_name();
        // tsc only treats a postfix `!` as a definite assignment assertion when
        // the binding name is a plain identifier and no line break precedes it.
        // Outside `for` initializers `allowExclamation` is true.
        let exclamation_token = self.parse_definite_assignment_assertion(name, true);
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        let initializer = self.parse_variable_declaration_initializer();
        self.parse_variable_declaration_after_parse_checks(flags, start_pos, name, initializer);

        let end_pos =
            self.parse_variable_declaration_end_pos(start_pos, type_annotation, name, initializer);

        self.arena.add_variable_declaration(
            syntax_kind_ext::VARIABLE_DECLARATION,
            start_pos,
            end_pos,
            VariableDeclarationData {
                name,
                exclamation_token,
                type_annotation,
                initializer,
            },
        )
    }
}
