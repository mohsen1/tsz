use super::state::*;
use crate::parser::node::*;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    pub(crate) fn is_optional_chain_expression(&self, expr: NodeIndex) -> bool {
        let Some(node) = self.arena.get(expr) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.arena
                    .get_access_expr(node)
                    .is_some_and(|access| access.question_dot_token)
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if node.is_optional_chain() {
                    return true;
                }
                self.arena
                    .get_call_expr(node)
                    .is_some_and(|call| self.is_optional_chain_expression(call.expression))
            }
            _ => false,
        }
    }

    // Parse argument list
    pub(crate) fn parse_argument_list(&mut self) -> NodeList {
        let mut args = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken) {
            if self.is_argument_list_recovery_boundary() {
                // At EOF, don't emit TS1135 — let parse_expected(CloseParenToken)
                // emit the more informative TS1005 "')' expected" instead.
                if !self.is_token(SyntaxKind::EndOfFileToken) {
                    self.error_argument_expression_expected();
                }
                break;
            }

            if self.is_token(SyntaxKind::DotDotDotToken) {
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression_allowing_arrow_return_type();
                if expression.is_none() {
                    // Emit TS1135 for incomplete spread argument: func(...missing)
                    self.error_argument_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::node::SpreadData { expression },
                );
                args.push(spread);
            } else if self.is_token(SyntaxKind::CommaToken) {
                // TS1135: missing argument before comma: func(a, , c)
                self.error_argument_expression_expected();
                args.push(NodeIndex::NONE);
            } else if self.is_token(SyntaxKind::SemicolonToken) {
                // Semicolon terminates argument list — don't emit TS1135 here.
                // Let parse_expected(CloseParenToken) emit TS1005 instead,
                // matching tsc which treats `;` as a clear boundary.
                break;
            } else {
                let arg = self.parse_assignment_expression_allowing_arrow_return_type();
                if arg.is_none() {
                    // TS1135 for missing function argument
                    self.error_argument_expression_expected();
                }
                args.push(arg);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    self.error_comma_expected();
                    self.next_token();
                    continue;
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    self.error_comma_expected();
                    self.next_token();

                    // After consuming the spurious `:`, if the next token is a
                    // statement-only recovery boundary (e.g. `var`, `let`,
                    // `return`), don't try to parse it as a type — emit TS1135
                    // at the keyword and break out of the argument list so the
                    // outer statement parser can recover. This matches tsc's
                    // behaviour where `f(x: var ...)` produces TS1005 at `:`,
                    // TS1135 at `var`, then re-parses `var ...` as a top-level
                    // variable declaration.
                    if self.is_argument_list_recovery_boundary() {
                        if !self.is_token(SyntaxKind::EndOfFileToken) {
                            // Bypass the distance-based suppression heuristic in
                            // `should_report_error` because the previous TS1005
                            // ',' expected emitted on the spurious `:` is only
                            // a couple of columns away — we still want to flag
                            // the keyword position with TS1135.
                            use tsz_common::diagnostics::diagnostic_codes;
                            self.parse_error_at_current_token(
                                "Argument expression expected.",
                                diagnostic_codes::ARGUMENT_EXPRESSION_EXPECTED,
                            );
                        }
                        break;
                    }

                    let recover_start = self.token_pos();
                    let recovered_type = self.parse_type();
                    if recovered_type.is_some() {
                        args.push(recovered_type);
                    }
                    if self.token_pos() == recover_start
                        && !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::CloseParenToken
                                | SyntaxKind::SemicolonToken
                                | SyntaxKind::EndOfFileToken
                        )
                    {
                        self.next_token();
                    }

                    if self.parse_optional(SyntaxKind::CommaToken) {
                        continue;
                    }
                    if self.is_token(SyntaxKind::CloseParenToken)
                        || self.is_token(SyntaxKind::SemicolonToken)
                        || self.is_token(SyntaxKind::EndOfFileToken)
                    {
                        break;
                    }
                }

                // Missing comma - check if next token looks like another argument
                // If so, emit comma error for better diagnostics
                if self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.error_comma_expected();
                    // Continue parsing for error recovery
                } else {
                    // Emit ',' expected for tokens that aren't the list terminator
                    if !self.is_token(SyntaxKind::CloseParenToken)
                        && !self.is_token(SyntaxKind::EndOfFileToken)
                    {
                        self.error_comma_expected();
                    }
                    break;
                }
            }
        }

        self.make_node_list(args)
    }

    // Returns true for statement-only keywords that should stop argument parsing
    // during recovery to avoid cascading diagnostics.
    pub(crate) const fn is_argument_list_recovery_boundary(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::ReturnKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::DebuggerKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::EndOfFileToken
        )
    }

    pub(crate) fn look_ahead_is_hashbang_after_at(&mut self) -> bool {
        if !self.is_token(SyntaxKind::AtToken) {
            return false;
        }
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = if self.is_token(SyntaxKind::HashToken) {
            self.next_token();
            self.is_token(SyntaxKind::ExclamationToken)
        } else {
            false
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    // Parse primary expression
    pub(crate) fn parse_primary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::Identifier => self.parse_identifier(),
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            SyntaxKind::NullKeyword => self.parse_null_literal(),
            SyntaxKind::UndefinedKeyword
            | SyntaxKind::AnyKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::RequireKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword
            | SyntaxKind::UsingKeyword => self.parse_keyword_as_identifier(),
            SyntaxKind::ThisKeyword => self.parse_this_expression(),
            SyntaxKind::SuperKeyword => self.parse_super_expression(),
            SyntaxKind::OpenParenToken => self.parse_parenthesized_expression(),
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::OpenBraceToken => self.parse_object_literal(),
            SyntaxKind::NewKeyword => self.parse_new_expression(),
            SyntaxKind::FunctionKeyword => self.parse_function_expression(),
            SyntaxKind::ClassKeyword => self.parse_class_expression(),
            SyntaxKind::AtToken => {
                if self.look_ahead_is_hashbang_after_at() {
                    let start_pos = self.token_pos();
                    let end_pos = self.token_end();
                    self.next_token(); // consume '@' and leave '#!' for outer recovery
                    self.arena
                        .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
                } else {
                    self.parse_decorated_class_expression()
                }
            }
            SyntaxKind::AsyncKeyword => {
                // async function expression or async arrow function
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_expression()
                } else {
                    // 'async' used as identifier (e.g., variable named async)
                    // Use parse_identifier_name since 'async' is a keyword
                    self.parse_identifier_name()
                }
            }
            // `<<` at expression start is invalid as a primary expression.
            // It is usually an ambiguous generic assertion case that should fall
            // through as a malformed left side and then recover with
            // TS1109: Expression expected.
            SyntaxKind::LessThanLessThanToken => {
                self.error_expression_expected();
                NodeIndex::NONE
            }
            SyntaxKind::LessThanToken => {
                if self.is_jsx_file() {
                    let (
                        next_token_is_numeric_literal,
                        next_token_is_open_brace,
                        next_token_is_slash,
                        next_token_is_dot,
                        next_token_pos,
                        next_token_end,
                    ) = {
                        let snapshot = self.scanner.save_state();
                        let current = self.current_token;
                        self.next_token();
                        let result = (
                            self.is_token(SyntaxKind::NumericLiteral),
                            self.is_token(SyntaxKind::OpenBraceToken),
                            self.is_token(SyntaxKind::SlashToken),
                            self.is_token(SyntaxKind::DotToken),
                            self.token_pos(),
                            self.token_end(),
                        );
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;
                        result
                    };
                    let allow_malformed_jsx_after_tilde = self
                        .get_source_text()
                        .get(..self.token_pos() as usize)
                        .and_then(|prefix| prefix.chars().rev().find(|ch| !ch.is_whitespace()))
                        == Some('~')
                        && {
                            let snapshot = self.scanner.save_state();
                            let current = self.current_token;
                            self.next_token();
                            let result = self.is_token(SyntaxKind::LessThanToken);
                            self.scanner.restore_state(snapshot);
                            self.current_token = current;
                            result
                        };
                    if next_token_is_numeric_literal {
                        // In JS/JSX recovery, `<1234> x` should retain the right-hand
                        // expression `x` as part of the same malformed construct,
                        // avoiding a trailing `';' expected` cascade. Emit TS1003
                        // at the numeric head to match conformance expectations.
                        self.parse_error_at(
                            next_token_pos,
                            next_token_end.saturating_sub(next_token_pos),
                            "Identifier expected.",
                            diagnostic_codes::IDENTIFIER_EXPECTED,
                        );
                        self.parse_type_assertion()
                    } else if self.look_ahead_next_is_identifier_or_keyword_or_greater_than()
                        || next_token_is_open_brace
                        || allow_malformed_jsx_after_tilde
                    {
                        self.parse_jsx_element_or_self_closing_or_fragment(true)
                    } else {
                        if next_token_is_slash {
                            let jsx_close_tail = self
                                .get_source_text()
                                .get(self.token_pos() as usize..)
                                .and_then(|tail| {
                                    let line_len = tail.find(['\n', '\r']).unwrap_or(tail.len());
                                    tail.get(..line_len)
                                })
                                .unwrap_or("")
                                .to_string();
                            if jsx_close_tail.starts_with("</.") {
                                let start = self.token_pos();
                                self.parse_error_at(
                                    start,
                                    1,
                                    "Expression expected.",
                                    diagnostic_codes::EXPRESSION_EXPECTED,
                                );
                                self.parse_error_at(
                                    start + 2,
                                    1,
                                    "Declaration or statement expected.",
                                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                                );
                                while !self.is_token(SyntaxKind::EndOfFileToken)
                                    && !self.is_token(SyntaxKind::GreaterThanToken)
                                    && !self.is_token(SyntaxKind::SemicolonToken)
                                {
                                    self.next_token();
                                }
                                if self.is_token(SyntaxKind::GreaterThanToken) {
                                    self.next_token();
                                }
                                return NodeIndex::NONE;
                            }
                            if jsx_close_tail.starts_with("</") && jsx_close_tail.contains('[') {
                                let start = self.token_pos();
                                self.parse_error_at(
                                    start,
                                    1,
                                    "Expression expected.",
                                    diagnostic_codes::EXPRESSION_EXPECTED,
                                );
                                while !self.is_token(SyntaxKind::EndOfFileToken)
                                    && !self.is_token(SyntaxKind::GreaterThanToken)
                                    && !self.is_token(SyntaxKind::SemicolonToken)
                                {
                                    self.next_token();
                                }
                                if self.is_token(SyntaxKind::GreaterThanToken) {
                                    self.next_token();
                                }
                                return NodeIndex::NONE;
                            }
                        }
                        if next_token_is_slash
                            && self.jsx_missing_brace_semicolon_window_start
                                == Some(self.token_pos())
                        {
                            self.parse_error_at_current_token(
                                "Declaration or statement expected.",
                                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                            );
                        } else {
                            self.error_expression_expected();
                        }
                        // Match tsc's `"<:a ...>"` TSX recovery: `<` is not a JSX
                        // opener unless the lookahead token is identifier/keyword
                        // or `>`. Consume the invalid `<` and let the following
                        // namespace tail surface in declaration-list recovery.
                        let invalid_head_start = self.token_pos();
                        self.next_token();
                        if next_token_is_dot && self.is_token(SyntaxKind::DotToken) {
                            self.parse_error_at_current_token(
                                "Expression expected.",
                                diagnostic_codes::EXPRESSION_EXPECTED,
                            );
                        }
                        if self.is_token(SyntaxKind::ColonToken) {
                            self.parse_error_at_current_token(
                                "Expression expected.",
                                diagnostic_codes::EXPRESSION_EXPECTED,
                            );
                            self.recover_jsx_invalid_namespace_head_tail = true;
                            let missing_left = self.create_missing_expression();
                            let missing_right = self.create_missing_expression();
                            return self.arena.add_binary_expr(
                                syntax_kind_ext::BINARY_EXPRESSION,
                                invalid_head_start,
                                self.token_pos(),
                                BinaryExprData {
                                    left: missing_left,
                                    operator_token: SyntaxKind::LessThanToken as u16,
                                    right: missing_right,
                                },
                            );
                        }
                        NodeIndex::NONE
                    }
                } else {
                    self.parse_jsx_element_or_type_assertion()
                }
            }
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                self.parse_no_substitution_template_literal()
            }
            SyntaxKind::TemplateHead => self.parse_template_expression(),
            // Regex literal - rescan / or /= as regex
            SyntaxKind::SlashToken => {
                let slash_pos = self.token_pos();
                let malformed_jsx_closing_tail = slash_pos > 0
                    && self
                        .get_source_text()
                        .as_bytes()
                        .get(slash_pos as usize - 1)
                        == Some(&b'<')
                    && self
                        .get_source_text()
                        .get(slash_pos as usize - 1..)
                        .and_then(|tail| {
                            let line_len = tail.find(['\n', '\r']).unwrap_or(tail.len());
                            tail.get(..line_len)
                        })
                        .is_some_and(|tail| tail.starts_with("</") && tail.contains('['));
                if malformed_jsx_closing_tail {
                    let start = slash_pos - 1;
                    self.parse_error_at(
                        start,
                        1,
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                    while !self.is_token(SyntaxKind::EndOfFileToken)
                        && !self.is_token(SyntaxKind::GreaterThanToken)
                        && !self.is_token(SyntaxKind::SemicolonToken)
                    {
                        self.next_token();
                    }
                    if self.is_token(SyntaxKind::GreaterThanToken) {
                        self.next_token();
                    }
                    NodeIndex::NONE
                } else {
                    self.parse_regex_literal()
                }
            }
            SyntaxKind::SlashEqualsToken => self.parse_regex_literal(),
            // Dynamic import or import.meta
            SyntaxKind::ImportKeyword => self.parse_import_expression(),
            // `as` and `satisfies` are binary operators but also valid identifiers.
            // When they appear at expression start, they must be identifiers
            // (e.g., `var x = as as string` — first `as` is the variable).
            SyntaxKind::AsKeyword | SyntaxKind::SatisfiesKeyword => self.parse_identifier_name(),
            SyntaxKind::Unknown => {
                // TS1127: Invalid character - emit specific error for invalid characters

                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
                let start_pos = self.token_pos();
                let end_pos = self.token_end();
                self.next_token();
                self.arena
                    .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
            }
            _ => {
                // Don't consume clause boundaries or expression terminators here.
                // Let callers decide how to recover so constructs like `switch` can resynchronize
                // without losing `case`/`default` tokens.
                // ColonToken is a structural delimiter (case clauses, labels, type annotations)
                // and must not be consumed as an error token.

                // `as` and `satisfies` are contextual keywords that have binary
                // operator precedence (for type assertions / satisfies checks) but
                // can also appear as plain identifiers. In *primary* position,
                // prefer identifier parsing — matching tsc's
                // `parsePrimaryExpression -> parseIdentifier` which returns true
                // from `isIdentifier()` for contextual keywords. Without this
                // branch the subsequent `is_binary_operator` check would reject
                // them and emit a spurious TS1109.
                if matches!(
                    self.token(),
                    SyntaxKind::AsKeyword | SyntaxKind::SatisfiesKeyword
                ) {
                    return self.parse_identifier_name();
                }

                if self.is_binary_operator() {
                    // Binary operator at expression start means missing LHS.
                    // Emit TS1109 matching tsc's parsePrimaryExpression behavior.
                    self.error_expression_expected();
                    return NodeIndex::NONE;
                }
                if self.is_token(SyntaxKind::EndOfFileToken) {
                    // At EOF while expecting an expression: emit TS1109 to match tsc.
                    // Examples: `[#abc]=` or `var x =` at end of file.
                    if (self.context_flags
                        & crate::parser::state::CONTEXT_FLAG_TEMPLATE_SPAN_EXPRESSION)
                        == 0
                    {
                        self.error_expression_expected();
                    }
                    return NodeIndex::NONE;
                }
                if self.is_at_expression_end()
                    || self.is_token(SyntaxKind::CaseKeyword)
                    || self.is_token(SyntaxKind::DefaultKeyword)
                    || self.is_token(SyntaxKind::ColonToken)
                {
                    return NodeIndex::NONE;
                }

                // Statement-only keywords cannot start expressions.
                // Return NONE so callers emit TS1109 (Expression expected).
                if matches!(
                    self.token(),
                    SyntaxKind::ReturnKeyword
                        | SyntaxKind::BreakKeyword
                        | SyntaxKind::ContinueKeyword
                        | SyntaxKind::ThrowKeyword
                        | SyntaxKind::TryKeyword
                        | SyntaxKind::CatchKeyword
                        | SyntaxKind::FinallyKeyword
                        | SyntaxKind::DoKeyword
                        | SyntaxKind::WhileKeyword
                        | SyntaxKind::ForKeyword
                        | SyntaxKind::SwitchKeyword
                        | SyntaxKind::WithKeyword
                        | SyntaxKind::DebuggerKeyword
                        | SyntaxKind::IfKeyword
                        | SyntaxKind::ElseKeyword
                        | SyntaxKind::VarKeyword
                ) {
                    return NodeIndex::NONE;
                }

                if self.is_identifier_or_keyword() {
                    // In expression position, future reserved words (public, private, etc.)
                    // are valid identifiers even in strict mode. TS1212/1213/1214 only apply
                    // in binding/declaration contexts (handled by check_illegal_binding_identifier
                    // and class member name parsing), not in expression position.
                    // tsc does not emit TS1213 for `foo(public ...)` inside a class body.
                    self.parse_identifier_name()
                } else {
                    if !self.is_js_file()
                        && self.is_token(SyntaxKind::GreaterThanToken)
                        && self.get_source_text().get(
                            self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize,
                        ) == Some("<")
                    {
                        while !self.is_token(SyntaxKind::EndOfFileToken)
                            && !self.scanner.has_preceding_line_break()
                            && !self.is_token(SyntaxKind::SemicolonToken)
                        {
                            self.next_token();
                        }
                        return NodeIndex::NONE;
                    }
                    // Unknown primary expression - create an error token
                    let start_pos = self.token_pos();
                    let end_pos = self.token_end();

                    self.error_expression_expected();

                    self.next_token();
                    self.arena
                        .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
                }
            }
        }
    }

    // Parse a decorated class expression: `@dec class C { }`
    // Used when `@` is encountered in expression position.
    pub(crate) fn parse_decorated_class_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let decorators = self.parse_decorators();
        if self.is_token(SyntaxKind::ClassKeyword) || self.is_token(SyntaxKind::AbstractKeyword) {
            self.parse_class_expression_with_decorators(decorators, start_pos)
        } else {
            // Decorators not followed by class - emit error and create error token.
            // Emit TS1109 at full_start position (before trivia) to match tsc's
            // createMissingNode(getNodePos()), then TS1005 at token start (after trivia)
            // to match tsc's parseErrorAtPosition(scanner.getTokenStart()). When there's
            // leading whitespace, the positions differ and both errors are emitted.
            let full_start = self.u32_from_usize(self.scanner.get_token_full_start());
            let end = self.u32_from_usize(self.scanner.get_token_end());
            self.parse_error_at(
                full_start,
                end.saturating_sub(full_start),
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );

            // Emit TS1005 with message matching what tsc's parser recovery produces.
            // When followed by `function` keyword (e.g., `@dec function() {}`), tsc
            // emits "',' expected." because it treats the result as an expression in
            // a comma context. For other tokens (e.g., `@dec () => {}`), tsc emits
            // "';' expected." as a statement boundary.
            //
            // If recovery crossed a line break (e.g., malformed `!@$` followed by the
            // next statement), tsc does not emit the companion TS1005 at the next-line
            // token; only TS1109 at the malformed expression site is kept.
            if !self.scanner.has_preceding_line_break() {
                if self.is_token(SyntaxKind::FunctionKeyword) {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                } else {
                    self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                }
            }
            let end_pos = self.token_end();
            if self.is_token(SyntaxKind::AtToken) {
                self.next_token();
            }
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    // Parse identifier
    // Uses zero-copy accessor and only clones when storing
    pub(crate) fn parse_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();

        // Check for reserved words that cannot be used as identifiers
        // These should emit TS1359 "Identifier expected. '{0}' is a reserved word that cannot be used here."
        if self.is_reserved_word() {
            self.error_reserved_word_identifier();
            // Create a missing identifier placeholder
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // Check if current token is an identifier or keyword that can be used as identifier
        // This allows contextual keywords (type, interface, package, etc.) to be used as identifiers
        // in appropriate contexts (e.g., type aliases, interface names)
        let (atom, text, original_text) = if self.is_identifier_or_keyword() {
            // OPTIMIZATION: Capture atom for O(1) comparison
            let atom = self.scanner.get_token_atom();
            let has_unicode_escape =
                (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0;
            let text = if !has_unicode_escape {
                let src = self.scanner.source_text();
                let start = self.scanner.get_token_start();
                let end = self.scanner.get_token_end();
                if start < end && end <= src.len() {
                    src[start..end].to_string()
                } else {
                    self.scanner.get_token_value_ref().to_string()
                }
            } else {
                self.scanner.get_token_value_ref().to_string()
            };
            // tsc preserves unicode escape sequences in emitted identifiers.
            // Capture the original source text when the scanner detected escapes.
            let original_text = if has_unicode_escape {
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
            self.next_token();
            (atom, text, original_text)
        } else {
            self.error_identifier_expected();
            (Atom::NONE, String::new(), None)
        };

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

    // Parse identifier name - allows keywords to be used as identifiers
    // This is used in contexts where keywords are valid identifier names
    // (e.g., class names, property names, function names)
    pub(crate) fn parse_identifier_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let (atom, text, original_text) = if self.is_identifier_or_keyword() {
            // OPTIMIZATION: Capture atom for O(1) comparison
            let atom = self.scanner.get_token_atom();
            let has_unicode_escape =
                (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0;
            let text = if !has_unicode_escape {
                let src = self.scanner.source_text();
                let start = self.scanner.get_token_start();
                let end = self.scanner.get_token_end();
                if start < end && end <= src.len() {
                    src[start..end].to_string()
                } else {
                    self.scanner.get_token_value_ref().to_string()
                }
            } else {
                self.scanner.get_token_value_ref().to_string()
            };
            // Preserve unicode escape sequences for emission parity with tsc
            let original_text = if has_unicode_escape {
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
            self.next_token();
            (atom, text, original_text)
        } else {
            self.error_identifier_expected();
            (Atom::NONE, String::new(), None)
        };

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

    // Parse private identifier (#name)
    pub(crate) fn parse_private_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        // OPTIMIZATION: Capture atom for O(1) comparison
        let atom = self.scanner.get_token_atom();
        let text = self.scanner.get_token_value_ref().to_string();
        self.parse_expected(SyntaxKind::PrivateIdentifier);

        self.arena.add_identifier(
            SyntaxKind::PrivateIdentifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    // Binding patterns, literals, array/object literals, property names,
    // new/member expressions → state_expressions_literals.rs
}
