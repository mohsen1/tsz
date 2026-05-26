use super::state::*;
use crate::parser::node::*;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    pub(crate) fn parse_switch_case_clauses(&mut self) -> Vec<NodeIndex> {
        let mut clauses = Vec::new();
        let mut seen_default = false;
        let mut reported_duplicate_default = false;
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CaseKeyword) {
                clauses.push(self.parse_switch_case_clause());
            } else if self.is_token(SyntaxKind::DefaultKeyword) {
                clauses.push(self.parse_switch_default_clause(
                    &mut seen_default,
                    &mut reported_duplicate_default,
                ));
            } else {
                // Unexpected token in switch body.
                // Emit TS1130 once (guarded by last_error_pos), then try to parse the
                // unexpected tokens as a complete statement so that compound constructs
                // like `class D {}` are consumed in one shot (emitting only ONE TS1130),
                // matching TSC's parseList / abortParsingListOrMoveToNextToken behavior.
                if self.token_pos() != self.last_error_pos {
                    self.parse_error_at_current_token(
                        diagnostic_messages::CASE_OR_DEFAULT_EXPECTED,
                        diagnostic_codes::CASE_OR_DEFAULT_EXPECTED,
                    );
                }
                let pos_before = self.token_pos();
                let _ = self.parse_statement();
                // Failsafe: if parse_statement didn't advance, advance one token to avoid
                // an infinite loop.
                if self.token_pos() == pos_before {
                    self.next_token();
                }
            }
        }
        clauses
    }

    pub(crate) fn parse_switch_case_clause(&mut self) -> NodeIndex {
        let clause_start = self.token_pos();
        self.next_token();
        let clause_expr = self.parse_expression();
        if clause_expr == NodeIndex::NONE {
            self.error_expression_expected();
        }
        self.parse_expected(SyntaxKind::ColonToken);

        let statements = self.parse_switch_clause_statements();
        let clause_end = self.token_end();
        self.arena.add_case_clause(
            syntax_kind_ext::CASE_CLAUSE,
            clause_start,
            clause_end,
            CaseClauseData {
                expression: clause_expr,
                statements: self.make_node_list(statements),
            },
        )
    }

    pub(crate) fn parse_switch_default_clause(
        &mut self,
        seen_default: &mut bool,
        reported_duplicate_default: &mut bool,
    ) -> NodeIndex {
        let clause_start = self.token_pos();

        // TS1260: Keywords cannot contain escape characters.
        // tsc emits this when `default` is written with unicode escapes like `def\u0061ult`.
        // The scanner resolves it to DefaultKeyword but sets UnicodeEscape flag.
        if (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0 {
            self.parse_error_at_current_token(
                diagnostic_messages::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS,
                diagnostic_codes::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS,
            );
        } else if *seen_default && !*reported_duplicate_default {
            self.parse_error_at_current_token(
                "A 'default' clause cannot appear more than once in a 'switch' statement.",
                diagnostic_codes::A_DEFAULT_CLAUSE_CANNOT_APPEAR_MORE_THAN_ONCE_IN_A_SWITCH_STATEMENT,
            );
            *reported_duplicate_default = true;
        }
        *seen_default = true;

        self.next_token();
        self.parse_expected(SyntaxKind::ColonToken);
        let statements = self.parse_switch_clause_statements();
        let clause_end = self.token_end();

        self.arena.add_case_clause(
            syntax_kind_ext::DEFAULT_CLAUSE,
            clause_start,
            clause_end,
            CaseClauseData {
                expression: NodeIndex::NONE,
                statements: self.make_node_list(statements),
            },
        )
    }

    pub(crate) fn parse_switch_clause_statements(&mut self) -> Vec<NodeIndex> {
        let mut statements = Vec::new();
        while !self.is_token(SyntaxKind::CaseKeyword)
            && !self.is_token(SyntaxKind::DefaultKeyword)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let pos_before = self.token_pos();
            let statement = self.parse_statement();
            if statement.is_some() {
                statements.push(statement);
            }
            if self.token_pos() == pos_before {
                self.next_token();
            }
        }
        statements
    }

    // Parse try statement
    // Parse orphan catch/finally block (missing try)
    // Emits TS1005: 'try' expected
    pub(crate) fn parse_try_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TryKeyword);

        let diag_len_before_try_block = self.parse_diagnostics.len();
        let try_block = self.parse_block();

        // Parse catch clause
        let catch_clause = if self.is_token(SyntaxKind::CatchKeyword) {
            let catch_start = self.token_pos();
            self.next_token();

            // Parse optional catch binding
            let variable_declaration = if self.is_token(SyntaxKind::OpenParenToken) {
                self.next_token();
                let decl = if self.is_token(SyntaxKind::CloseParenToken) {
                    // TS1003: `catch ()` — parens present but no binding identifier.
                    // `catch { }` (no parens) is valid optional catch binding,
                    // but `catch () { }` requires an identifier between the parens.
                    self.parse_error_at_current_token(
                        "Identifier expected.",
                        diagnostic_codes::IDENTIFIER_EXPECTED,
                    );
                    NodeIndex::NONE
                } else {
                    // Pass flag 0x8 (CATCH_CLAUSE_BINDING) to suppress TS1182
                    // since catch bindings are destructuring without initializers
                    self.parse_variable_declaration_with_flags(0x8)
                };
                self.parse_expected(SyntaxKind::CloseParenToken);
                decl
            } else {
                NodeIndex::NONE
            };

            let catch_block = self.parse_block();
            let catch_end = self.token_end();

            self.arena.add_catch_clause(
                syntax_kind_ext::CATCH_CLAUSE,
                catch_start,
                catch_end,
                CatchClauseData {
                    variable_declaration,
                    block: catch_block,
                },
            )
        } else {
            NodeIndex::NONE
        };

        // Parse finally clause
        let finally_block = if self.is_token(SyntaxKind::FinallyKeyword) {
            self.next_token();
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // Error recovery: try without catch or finally is invalid
        let saw_orphan_catch_or_finally_recovery = self.parse_diagnostics
            [diag_len_before_try_block..]
            .iter()
            .any(|diag| {
                diag.code == diagnostic_codes::EXPECTED && diag.message == "'try' expected."
            });
        if catch_clause.is_none()
            && finally_block.is_none()
            && self.token_pos() != self.last_error_pos
            && !saw_orphan_catch_or_finally_recovery
        {
            self.parse_error_at_current_token(
                "'catch' or 'finally' expected.",
                diagnostic_codes::CATCH_OR_FINALLY_EXPECTED,
            );
        }

        let end_pos = self.token_end();
        self.arena.add_try(
            syntax_kind_ext::TRY_STATEMENT,
            start_pos,
            end_pos,
            TryData {
                try_block,
                catch_clause,
                finally_block,
            },
        )
    }

    // Parse with statement
    pub(crate) fn parse_with_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let with_end = self.token_end();
        self.parse_expected(SyntaxKind::WithKeyword);

        // TS1101: 'with' statements are not allowed in strict mode.
        // Class bodies and module top-level are auto-strict per the ECMA spec,
        // so a `with` syntactically nested inside either is an error. tsc
        // emits the diagnostic at the `with` keyword's span. Parsing continues
        // unchanged so the rest of the construct still reaches downstream.
        if self.in_strict_mode_context() {
            self.parse_error_at(
                start_pos,
                with_end.saturating_sub(start_pos),
                "'with' statements are not allowed in strict mode.",
                diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
            );
        }

        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing with expression: with () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();

        // Use if statement structure for with (expression + statement)
        self.arena.add_if_statement(
            syntax_kind_ext::WITH_STATEMENT,
            start_pos,
            end_pos,
            IfStatementData {
                expression,
                then_statement: statement,
                else_statement: NodeIndex::NONE,
            },
        )
    }

    // Parse debugger statement
    pub(crate) fn parse_debugger_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DebuggerKeyword);
        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena
            .add_token(syntax_kind_ext::DEBUGGER_STATEMENT, start_pos, end_pos)
    }

    // Parse expression statement
    pub(crate) fn parse_expression_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.pending_jsx_missing_close_brace_in_expression_statement = 0;
        self.jsx_missing_brace_semicolon_window_start = Some(start_pos);

        let started_with_binary_operator = !self.is_expression_start() && self.is_binary_operator();
        let expression = if started_with_binary_operator {
            self.error_expression_expected();
            self.next_token();
            let right = if self.is_expression_start() {
                self.parse_binary_expression(2)
            } else {
                NodeIndex::NONE
            };
            if right.is_none() {
                self.create_missing_expression()
            } else {
                right
            }
        } else {
            // Early rejection: If the current token cannot start an expression, fail immediately
            // This prevents TS1109 from being emitted for tokens that are obviously not expressions
            // (e.g., }, ], ), etc.) when we fall through to parse_expression_statement() from
            // parse_statement()'s wildcard match.
            if !self.is_expression_start() {
                // Don't emit error here - let the statement-level error handling deal with it
                // Just return NONE to indicate failure
                self.jsx_missing_brace_semicolon_window_start = None;
                return NodeIndex::NONE;
            }

            self.parse_expression()
        };

        // If expression parsing failed completely, resync to recover
        if expression.is_none() {
            if !self.is_js_file()
                && self.is_token(SyntaxKind::GreaterThanToken)
                && self
                    .get_source_text()
                    .get(self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize)
                    == Some("<")
            {
                while !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.scanner.has_preceding_line_break()
                    && !self.is_token(SyntaxKind::SemicolonToken)
                {
                    self.next_token();
                }
                self.pending_jsx_missing_close_brace_in_expression_statement = 0;
                self.jsx_missing_brace_semicolon_window_start = None;
                return NodeIndex::NONE;
            }
            // Emit error for unexpected token if we haven't already
            if self.token_pos() != self.last_error_pos && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            // Try to parse semicolon for partial recovery, then resync
            let _ = self.can_parse_semicolon();
            if self.is_token(SyntaxKind::SemicolonToken) {
                let semicolon_pos = self.token_pos();
                if self.should_emit_jsx_missing_close_brace_at_semicolon(start_pos, semicolon_pos) {
                    self.parse_error_at(
                        semicolon_pos,
                        0,
                        "'}' expected.",
                        diagnostic_codes::EXPECTED,
                    );
                }
                self.jsx_missing_brace_semicolon_window_start =
                    Some(semicolon_pos.saturating_add(1));
                self.next_token();
            } else {
                self.resync_after_error();
            }
            self.pending_jsx_missing_close_brace_in_expression_statement = 0;
            self.jsx_missing_brace_semicolon_window_start = None;
            return NodeIndex::NONE;
        }

        if !self.suppress_next_jsx_head_missing_semicolon {
            self.recover_adjacent_jsx_siblings(expression);
        }

        // Use smart error reporting for missing semicolons (matches TypeScript's
        // parseExpressionOrLabeledStatement behavior). Instead of generic TS1005 "';' expected",
        // this checks if the expression is a misspelled keyword and emits TS1435/TS1434.
        if self.is_token(SyntaxKind::SemicolonToken) {
            let semicolon_pos = self.token_pos();
            let needs_jsx_semicolon_missing_brace =
                self.arena.get(expression).is_some_and(|node| {
                    self.should_emit_jsx_missing_close_brace_at_semicolon(node.pos, semicolon_pos)
                });
            let has_empty_jsx_attribute_expression = self
                .get_source_text()
                .get(start_pos as usize..semicolon_pos as usize)
                .is_some_and(|segment| segment.contains("={}"));
            if self.recover_jsx_missing_attr_initializer_head {
                self.parse_error_at(
                    semicolon_pos,
                    0,
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            } else if !self.suppress_next_jsx_missing_brace_at_semicolon
                && !has_empty_jsx_attribute_expression
                && (self.pending_jsx_missing_close_brace_in_expression_statement > 0
                    || needs_jsx_semicolon_missing_brace)
            {
                self.parse_error_at(
                    semicolon_pos,
                    0,
                    "'}' expected.",
                    diagnostic_codes::EXPECTED,
                );
            }
            self.jsx_missing_brace_semicolon_window_start = Some(semicolon_pos.saturating_add(1));
            self.next_token();
        } else if self.is_token(SyntaxKind::Unknown) {
            // Invalid character (e.g., standalone `\`). Emit TS1127 and skip it,
            // then check for semicolon again. This matches tsc's scanError behavior
            // where the scanner reports TS1127 and advances past the invalid char.
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
            );
            self.next_token();
            if self.is_token(SyntaxKind::SemicolonToken) {
                let semicolon_pos = self.token_pos();
                self.jsx_missing_brace_semicolon_window_start =
                    Some(semicolon_pos.saturating_add(1));
                self.next_token();
            }
        } else if self.suppress_next_jsx_head_missing_semicolon
            && (self.is_token(SyntaxKind::LessThanToken)
                || self.is_token(SyntaxKind::LessThanSlashToken))
            && self
                .get_source_text()
                .get(self.token_pos() as usize..)
                .is_some_and(|tail| {
                    tail.starts_with("</") || self.is_token(SyntaxKind::LessThanSlashToken)
                })
        {
            self.parse_error_at(
                self.token_pos(),
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
                self.parse_error_at(
                    self.token_pos(),
                    1,
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
                self.next_token();
            }
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.next_token();
            }
            self.suppress_next_jsx_head_missing_semicolon = false;
        } else if self.recover_jsx_closing_tag_trailing_tail {
            let tail_error_pos = self.last_error_pos.saturating_add(1);
            self.parse_error_at(
                tail_error_pos,
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
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
                self.next_token();
            }
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
                self.next_token();
            }
        } else if !self.can_parse_semicolon() {
            let jsx_head_needs_semicolon = self.arena.get(expression).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                        | syntax_kind_ext::JSX_OPENING_ELEMENT
                        | syntax_kind_ext::JSX_ELEMENT
                )
            });
            // When the expression statement holds an arrow or function expression with a
            // block body and the next token is `=`, defer to the parent statement-list
            // loop. tsc emits TS2809 ("Declaration or statement expected. This '=' follows
            // a block of statements...") at the `=` and a separate TS1005 at the start
            // of the recovered expression that follows. Emitting TS1005 here at the `=`
            // would dedupe-suppress TS2809 and lose the second TS1005.
            let arrow_or_func_block_followed_by_equals = self.is_token(SyntaxKind::EqualsToken)
                && self
                    .arena
                    .get(expression)
                    .is_some_and(|node| node.is_function_expression_or_arrow());
            let has_numeric_follow_error = self.current_token_has_numeric_literal_follow_error();
            if jsx_head_needs_semicolon && has_numeric_follow_error {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            } else if started_with_binary_operator {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                if self.is_assignment_operator(self.token()) {
                    self.next_token();
                }
            } else if self.suppress_next_jsx_head_missing_semicolon {
                self.suppress_next_jsx_head_missing_semicolon = false;
            } else if !has_numeric_follow_error && !arrow_or_func_block_followed_by_equals {
                self.parse_error_for_missing_semicolon_after(expression);
                if self.expression_statement_block_function_recovers_conditional_tail(expression) {
                    self.recover_invalid_conditional_tail_after_expression_statement();
                }
            }
            // For malformed JSX heads like `<X -attr={...} />`, tsc reports `';' expected`
            // at `=` and then continues from the `{...}` tail, which can surface
            // downstream slash-regex diagnostics. Consume the standalone `=` token to
            // align that recovery shape without affecting numeric-literal follow cases.
            if jsx_head_needs_semicolon && self.is_token(SyntaxKind::EqualsToken) {
                self.next_token();
            }
            // Recovery for malformed fragments like `this.x: any;`.
            // Consume stray `:` so the following token can still be parsed as
            // a standalone expression statement on the next iteration.
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token();
            }
        }
        // token_full_start() (not token_end()) matches tsc's finishNode/getTokenFullStart() for ASI.
        let end_pos = self.token_full_start();
        self.pending_jsx_missing_close_brace_in_expression_statement = 0;
        self.jsx_missing_brace_semicolon_window_start = None;
        self.suppress_next_jsx_missing_brace_at_semicolon = false;
        self.recover_jsx_missing_attr_initializer_head = false;
        self.recover_jsx_closing_tag_trailing_tail = false;
        self.recover_jsx_closing_tag_extra_namespace_tail = false;
        self.recover_jsx_invalid_namespace_head_tail = false;
        self.suppress_next_jsx_head_missing_semicolon = false;

        self.arena.add_expr_statement(
            syntax_kind_ext::EXPRESSION_STATEMENT,
            start_pos,
            end_pos,
            ExprStatementData { expression },
        )
    }

    pub(crate) fn expression_statement_block_function_recovers_conditional_tail(
        &self,
        expression: NodeIndex,
    ) -> bool {
        if !self.is_token(SyntaxKind::QuestionToken) {
            return false;
        }
        let Some(node) = self.arena.get(expression) else {
            return false;
        };
        if !node.is_function_expression_or_arrow() {
            return false;
        }
        let Some(function) = self.arena.get_function(node) else {
            return false;
        };
        self.arena
            .get(function.body)
            .is_some_and(|body| body.kind == syntax_kind_ext::BLOCK)
    }

    pub(crate) fn recover_invalid_conditional_tail_after_expression_statement(&mut self) {
        if !self.is_token(SyntaxKind::QuestionToken) {
            return;
        }

        self.next_token(); // consume the stray `?`
        self.parse_recovered_invalid_conditional_branch_expression_statement();
        if self.is_token(SyntaxKind::ColonToken) {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token(); // consume the stray `:`
        }
        self.parse_recovered_invalid_conditional_branch_expression_statement();
    }

    pub(crate) fn parse_recovered_invalid_conditional_branch_expression_statement(&mut self) {
        if !self.is_expression_start() {
            return;
        }

        let pending_start = self.pending_recovered_expression_statements.len();
        let stmt = self.parse_expression_statement();
        let nested_recovered = self
            .pending_recovered_expression_statements
            .split_off(pending_start);
        if stmt.is_some() {
            self.pending_recovered_expression_statements.push(stmt);
        }
        self.pending_recovered_expression_statements
            .extend(nested_recovered);
    }
}
