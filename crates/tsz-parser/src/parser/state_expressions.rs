use tsz_common::diagnostics::diagnostic_codes;

/// Parser state - expression parsing methods (comma, assignment, arrow helpers)
use super::state::{CONTEXT_FLAG_IN_CONDITIONAL_TRUE, ParserState};
use crate::parser::{NodeIndex, node::BinaryExprData, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn count_following_close_braces(&mut self) -> u32 {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let mut count = 0;
        while self.is_token(SyntaxKind::CloseBraceToken) {
            count += 1;
            self.next_token();
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        count
    }

    pub(crate) fn look_ahead_question_is_optional_parameter_marker(
        &mut self,
        previous_top_level_can_end_parameter_name: bool,
    ) -> bool {
        if !previous_top_level_can_end_parameter_name {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();

        let is_optional_parameter = matches!(
            self.token(),
            SyntaxKind::ColonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::EqualsToken
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_optional_parameter
    }

    // =========================================================================
    // Parse Methods - Expressions
    // =========================================================================

    // Parse an expression (including comma operator)
    pub fn parse_expression(&mut self) -> NodeIndex {
        // Clear the decorator context when parsing Expression, as it should be
        // unambiguous when parsing a decorator's parenthesized sub-expression.
        // This matches tsc's parseExpression() behavior.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_IN_DECORATOR;

        let start_pos = self.token_pos();
        let mut left = self.parse_assignment_expression();

        // Handle comma operator: expr, expr, expr
        // Comma expressions create a sequence, returning the last value
        while self.is_token(SyntaxKind::CommaToken) {
            self.next_token(); // consume comma
            let mut right = self.parse_assignment_expression();
            if right.is_none() {
                // Emit TS1109 for trailing comma or missing expression: expr, [missing]
                // Reset last_error_pos to bypass suppression: when both operands of
                // a comma expression are missing (e.g. `( , )`), the left-side error
                // and this right-side error are close together but both are required
                // by tsc (separate errors for each missing operand).
                let saved_error_pos = self.last_error_pos;
                self.last_error_pos = 0;
                self.error_expression_expected();
                if self.last_error_pos == 0 {
                    self.last_error_pos = saved_error_pos;
                }
                right = self.create_missing_expression();
            }
            let end_pos = self.token_end();

            left = self.arena.add_binary_expr(
                syntax_kind_ext::BINARY_EXPRESSION,
                start_pos,
                end_pos,
                BinaryExprData {
                    left,
                    operator_token: SyntaxKind::CommaToken as u16,
                    right,
                },
            );
        }

        self.context_flags = saved_flags;
        left
    }

    // Parse assignment expression
    pub(crate) fn parse_assignment_expression(&mut self) -> NodeIndex {
        let saved_pending_failed_async_arrow_colon_recovery =
            self.pending_failed_async_arrow_colon_recovery;
        let mut deferred_failed_async_arrow_colon_recovery = false;

        // Check for arrow function first (including async arrow)
        let lookahead_token = self.current_token;
        let lookahead_state = self.scanner.save_state();
        let is_arrow_start = self.is_start_of_arrow_function();
        self.scanner.restore_state(lookahead_state);
        self.current_token = lookahead_token;
        if is_arrow_start {
            // Check if it's an async arrow function
            // Note: `async => x` is a NON-async arrow where 'async' is the parameter name
            // `async x => x` or `async (x) => x` are async arrow functions
            if self.is_token(SyntaxKind::AsyncKeyword) {
                // Need to distinguish:
                // - `async => expr` (non-async, 'async' is param)
                // - `async x => expr` or `async (x) => expr` (async arrow)
                if self.look_ahead_is_simple_arrow_function() {
                    // async => expr - treat 'async' as identifier parameter
                    return self.parse_arrow_function_expression_with_async(false);
                }
                if self.look_ahead_can_commit_async_arrow_function() {
                    return self.parse_async_arrow_function_expression();
                }
                deferred_failed_async_arrow_colon_recovery = true;
                self.pending_failed_async_arrow_colon_recovery = true;
            } else {
                return self.parse_arrow_function_expression_with_async(false);
            }
        }

        // Parse the non-assignment binary expression first.
        // Start at precedence 2 to skip comma operator (precedence 1).
        // Assignment operators return precedence 0 in get_operator_precedence,
        // so they are NOT consumed by the binary expression chain. Instead,
        // we handle them here, matching tsc's parseAssignmentExpressionOrHigher.
        let start_pos = self.token_pos();
        let left = self.parse_binary_expression(2);

        // Check if the next token is an assignment operator.
        // Rescan `>` to handle compound tokens like `>>=` and `>>>=`.
        let op = if self.is_token(SyntaxKind::GreaterThanToken) {
            self.try_rescan_greater_token()
        } else {
            self.token()
        };

        if self.is_assignment_operator(op) {
            // JSX heads from malformed recovery (`<X -attr` / `<X 32attr`) are
            // never valid assignment targets. Preserve the JSX expression as-is
            // so statement-level recovery can surface tsc's `';' expected` and
            // follow-up diagnostics at the assignment token.
            let left_is_jsx_expression = self.arena.get(left).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                        | syntax_kind_ext::JSX_OPENING_ELEMENT
                        | syntax_kind_ext::JSX_ELEMENT
                        | syntax_kind_ext::JSX_FRAGMENT
                )
            });
            // Await expressions are not valid assignment targets.
            // Keep the await expression as the complete left side so statement
            // recovery can report the missing semicolon at the assignment token
            // instead of building an assignment expression.
            let left_is_await_expression = self
                .arena
                .get(left)
                .is_some_and(|node| node.kind == syntax_kind_ext::AWAIT_EXPRESSION);
            // `in` expressions also cannot be assignment targets without
            // parenthesized recovery. Preserve the parsed binary expression so
            // statement-level recovery reports `';' expected` at `=`.
            let left_is_in_expression = self.arena.get(left).is_some_and(|node| {
                node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && self.arena.get_binary_expr(node).is_some_and(|binary| {
                        SyntaxKind::try_from_u16(binary.operator_token)
                            .unwrap_or(SyntaxKind::Unknown)
                            == SyntaxKind::InKeyword
                    })
            });
            // Update expressions (`x++`, `x--`, `++x`, `--x`) are not
            // LeftHandSideExpressions and therefore cannot be targets of
            // assignment. Preserve the parsed update expression so
            // statement-level recovery reports `';' expected` at `=`, matching
            // tsc's parseAssignmentExpressionOrHigher LHS gate.
            let left_is_update_expression =
                self.arena.get(left).is_some_and(|node| match node.kind {
                    syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => true,
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                        self.arena.get_unary_expr(node).is_some_and(|data| {
                            let op = SyntaxKind::try_from_u16(data.operator)
                                .unwrap_or(SyntaxKind::Unknown);
                            matches!(op, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
                        })
                    }
                    _ => false,
                });
            if left_is_jsx_expression
                || left_is_await_expression
                || left_is_in_expression
                || left_is_update_expression
            {
                if deferred_failed_async_arrow_colon_recovery
                    && !self.is_token(SyntaxKind::ColonToken)
                {
                    self.pending_failed_async_arrow_colon_recovery =
                        saved_pending_failed_async_arrow_colon_recovery;
                }
                return left;
            }

            if self.in_if_condition_context()
                && self
                    .arena
                    .get(left)
                    .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
            {
                if deferred_failed_async_arrow_colon_recovery
                    && !self.is_token(SyntaxKind::ColonToken)
                {
                    self.pending_failed_async_arrow_colon_recovery =
                        saved_pending_failed_async_arrow_colon_recovery;
                }
                return left;
            }

            if self.in_parenthesized_expression_context()
                && self
                    .arena
                    .get(left)
                    .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
            {
                self.parse_error_at_current_token("')' expected.", diagnostic_codes::EXPECTED);
            }

            let operator_token = op as u16;
            self.next_token();
            let mut right = self.parse_assignment_expression();
            if right.is_none() {
                self.error_expression_expected();
                right = self.create_missing_expression();
            }
            let end_pos = self.token_end();
            if deferred_failed_async_arrow_colon_recovery && !self.is_token(SyntaxKind::ColonToken)
            {
                self.pending_failed_async_arrow_colon_recovery =
                    saved_pending_failed_async_arrow_colon_recovery;
            }
            return self.arena.add_binary_expr(
                syntax_kind_ext::BINARY_EXPRESSION,
                start_pos,
                end_pos,
                BinaryExprData {
                    left,
                    operator_token,
                    right,
                },
            );
        }

        if deferred_failed_async_arrow_colon_recovery && !self.is_token(SyntaxKind::ColonToken) {
            self.pending_failed_async_arrow_colon_recovery =
                saved_pending_failed_async_arrow_colon_recovery;
        }

        left
    }

    pub(crate) fn parse_assignment_expression_allowing_arrow_return_type(&mut self) -> NodeIndex {
        let saved_flags = self.context_flags;
        self.context_flags &= !CONTEXT_FLAG_IN_CONDITIONAL_TRUE;
        let expression = self.parse_assignment_expression();
        self.context_flags = saved_flags;
        expression
    }
}
