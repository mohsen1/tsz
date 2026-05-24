use tsz_common::diagnostics::diagnostic_codes;

/// Parser state - expression parsing methods
use super::state::{CONTEXT_FLAG_ARROW_PARAMETERS, CONTEXT_FLAG_IN_CONDITIONAL_TRUE, ParserState};
use crate::parser::{
    NodeIndex,
    node::{
        AccessExprData, BinaryExprData, CallExprData, ConditionalExprData, IdentifierData,
        TaggedTemplateData, UnaryExprData, UnaryExprDataEx,
    },
    node_flags, syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;
use tsz_scanner::keyword_text_len;

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

    pub(crate) fn parse_binary_expression_chain(
        &mut self,
        min_precedence: u8,
        start_pos: u32,
    ) -> NodeIndex {
        let mut left = self.parse_unary_expression();

        loop {
            let op = if self.is_token(SyntaxKind::GreaterThanToken) {
                self.try_rescan_greater_token()
            } else {
                self.token()
            };

            if !self.in_parenthesized_expression_context()
                && op == SyntaxKind::BarBarToken
                && self.is_assignment_target_with_block_bodied_arrow(left)
            {
                break;
            }

            if !self.is_js_file()
                && self.scanner.has_preceding_line_break()
                && matches!(op, SyntaxKind::LessThanToken | SyntaxKind::GreaterThanToken)
                && self.arena.get(left).is_some_and(|node| {
                    matches!(
                        node.kind,
                        syntax_kind_ext::JSX_ELEMENT
                            | syntax_kind_ext::JSX_FRAGMENT
                            | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    )
                })
            {
                break;
            }

            let precedence = self.get_operator_precedence(op);
            if precedence == 0 || precedence < min_precedence {
                break;
            }

            if op == SyntaxKind::AsKeyword || op == SyntaxKind::SatisfiesKeyword {
                // `as` and `satisfies` do not bind across line terminators.
                // `x\nas Type` is two statements via ASI, not a type assertion.
                if self.scanner.has_preceding_line_break() {
                    break;
                }
                left = self.parse_as_or_satisfies_expression(left, start_pos);
                continue;
            }

            left = self.parse_binary_expression_remainder(left, start_pos, op, precedence);
        }

        left
    }

    fn is_assignment_target_with_block_bodied_arrow(&self, node: NodeIndex) -> bool {
        let mut current = node;
        loop {
            let Some(node_data) = self.arena.get(current) else {
                return false;
            };
            if node_data.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }

            let Some(binary) = self.arena.get_binary_expr(node_data) else {
                return false;
            };
            let operator =
                SyntaxKind::try_from_u16(binary.operator_token).unwrap_or(SyntaxKind::Unknown);
            if !self.is_assignment_operator(operator) {
                return false;
            }
            if self.is_block_bodied_arrow_function(binary.right) {
                return true;
            }
            current = binary.right;
        }
    }

    fn is_block_bodied_arrow_function(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        if node_data.kind != syntax_kind_ext::ARROW_FUNCTION {
            return false;
        }
        let Some(function_data) = self.arena.get_function(node_data) else {
            return false;
        };
        let Some(body_node) = self.arena.get(function_data.body) else {
            return false;
        };

        body_node.kind == syntax_kind_ext::BLOCK
    }

    pub(crate) const fn is_assignment_operator(&self, operator: SyntaxKind) -> bool {
        matches!(
            operator,
            SyntaxKind::EqualsToken
                | SyntaxKind::PlusEqualsToken
                | SyntaxKind::MinusEqualsToken
                | SyntaxKind::AsteriskEqualsToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::PercentEqualsToken
                | SyntaxKind::AsteriskAsteriskEqualsToken
                | SyntaxKind::LessThanLessThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                | SyntaxKind::AmpersandEqualsToken
                | SyntaxKind::CaretEqualsToken
                | SyntaxKind::BarEqualsToken
                | SyntaxKind::BarBarEqualsToken
                | SyntaxKind::AmpersandAmpersandEqualsToken
                | SyntaxKind::QuestionQuestionEqualsToken
        )
    }

    fn parse_binary_expression_remainder(
        &mut self,
        left: NodeIndex,
        start_pos: u32,
        op: SyntaxKind,
        precedence: u8,
    ) -> NodeIndex {
        let operator_token = op as u16;
        self.next_token();

        if op == SyntaxKind::QuestionToken {
            return self.parse_conditional_expression(left, start_pos);
        }

        let right = self.parse_binary_expression_rhs(left, op, precedence);
        let end_pos = self.token_end();
        let final_right = if right.is_none() { left } else { right };

        self.arena.add_binary_expr(
            syntax_kind_ext::BINARY_EXPRESSION,
            start_pos,
            end_pos,
            BinaryExprData {
                left,
                operator_token,
                right: final_right,
            },
        )
    }

    fn parse_conditional_expression(&mut self, condition: NodeIndex, start_pos: u32) -> NodeIndex {
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CONDITIONAL_TRUE;

        let mut when_true = self.parse_assignment_expression();
        self.context_flags = saved_flags;

        if when_true.is_none() {
            self.error_expression_expected();
            when_true = self.create_missing_expression();
        }

        self.parse_expected(SyntaxKind::ColonToken);
        let mut when_false = self.parse_assignment_expression();
        self.context_flags = saved_flags;
        if when_false.is_none() {
            self.error_expression_expected();
            when_false = self.create_missing_expression();
        }
        let end_pos = self.token_end();

        self.arena.add_conditional_expr(
            syntax_kind_ext::CONDITIONAL_EXPRESSION,
            start_pos,
            end_pos,
            ConditionalExprData {
                condition,
                when_true,
                when_false,
            },
        )
    }

    fn parse_binary_expression_rhs(
        &mut self,
        _left: NodeIndex,
        op: SyntaxKind,
        precedence: u8,
    ) -> NodeIndex {
        let is_assignment = matches!(
            op,
            SyntaxKind::EqualsToken
                | SyntaxKind::PlusEqualsToken
                | SyntaxKind::MinusEqualsToken
                | SyntaxKind::AsteriskEqualsToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::PercentEqualsToken
                | SyntaxKind::AsteriskAsteriskEqualsToken
                | SyntaxKind::LessThanLessThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                | SyntaxKind::AmpersandEqualsToken
                | SyntaxKind::CaretEqualsToken
                | SyntaxKind::BarEqualsToken
                | SyntaxKind::BarBarEqualsToken
                | SyntaxKind::AmpersandAmpersandEqualsToken
                | SyntaxKind::QuestionQuestionEqualsToken
        );
        let next_min = if op == SyntaxKind::AsteriskAsteriskToken {
            precedence
        } else {
            precedence + 1
        };
        let right = if is_assignment {
            self.parse_assignment_expression()
        } else {
            self.parse_binary_expression(next_min)
        };

        if right.is_none() {
            // Emit TS1109 directly, bypassing distance-based suppression.
            // tsc only suppresses at the exact same position, so a missing RHS
            // after a binary operator always emits TS1109 even if a prior error
            // (e.g., TS1003 from JSX) is nearby.
            if !(!self.is_js_file()
                && self.is_token(SyntaxKind::GreaterThanToken)
                && self
                    .get_source_text()
                    .get(self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize)
                    == Some("<"))
            {
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            let recovered = self.try_recover_binary_rhs();
            if recovered.is_none() {
                // Create a missing expression placeholder instead of returning
                // `left`. Returning `left` would duplicate the left operand in
                // the parent binary expression (e.g., `1 > > 2` would become
                // `1 > 1 > 2` instead of `1 >  > 2`). A missing expression
                // keeps the AST structurally correct and the emitter will
                // output nothing for it.
                return self.create_missing_expression();
            }
            return recovered;
        }

        right
    }

    // Parse as/satisfies expression: expr as Type, expr satisfies Type
    // Also handles const assertion: expr as const
    pub(crate) fn parse_as_or_satisfies_expression(
        &mut self,
        expression: NodeIndex,
        start_pos: u32,
    ) -> NodeIndex {
        let is_satisfies = self.is_token(SyntaxKind::SatisfiesKeyword);
        let keyword_pos = self.token_pos();
        self.next_token(); // consume 'as' or 'satisfies'

        // Handle 'as const' - const assertion
        let type_node = if !is_satisfies && self.is_token(SyntaxKind::ConstKeyword) {
            // Create a token node for 'const' keyword
            let const_start = self.token_pos();
            let const_end = self.token_end();
            self.next_token(); // consume 'const'
            self.arena
                .add_token(SyntaxKind::ConstKeyword as u16, const_start, const_end)
        } else {
            self.parse_non_predicate_type()
        };
        let end_pos = self.token_end();

        let result = self.arena.add_type_assertion(
            if is_satisfies {
                syntax_kind_ext::SATISFIES_EXPRESSION
            } else {
                syntax_kind_ext::AS_EXPRESSION
            },
            start_pos,
            end_pos,
            crate::parser::node::TypeAssertionData {
                expression,
                type_node,
                keyword_pos,
            },
        );

        // Allow chaining: x as T as U
        if self.is_token(SyntaxKind::AsKeyword) || self.is_token(SyntaxKind::SatisfiesKeyword) {
            return self.parse_as_or_satisfies_expression(result, start_pos);
        }

        result
    }

    // Parse unary expression
    pub(crate) fn parse_unary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                let is_update_operator = operator == SyntaxKind::PlusPlusToken as u16
                    || operator == SyntaxKind::MinusMinusToken as u16;
                self.next_token();
                if is_update_operator {
                    match self.token() {
                        // TSC recovers `++delete foo.bar`, `++++y`, `++\n++y`
                        // by treating the outer `++`/`--` as a unary with a
                        // missing operand and leaving the inner unary
                        // (`delete …`, `++y`, …) for the next statement, so
                        // the JS emitter prints the bare `++;` followed by
                        // the inner expression statement. tsc reaches the
                        // same shape via `parsePrimaryExpression`'s default
                        // `parseIdentifier(Expression_expected)` branch,
                        // which emits TS1109 at the offender without
                        // consuming it.
                        SyntaxKind::DeleteKeyword
                        | SyntaxKind::PlusPlusToken
                        | SyntaxKind::MinusMinusToken => {
                            self.parse_error_at(
                                self.token_pos(),
                                self.token_end().saturating_sub(self.token_pos()),
                                "Expression expected.",
                                diagnostic_codes::EXPRESSION_EXPECTED,
                            );
                            // End the unary expression at the offender's
                            // start so the next statement begins at the
                            // unconsumed token.
                            let end_pos = self.token_pos();
                            return self.arena.add_unary_expr(
                                syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                                start_pos,
                                end_pos,
                                UnaryExprData {
                                    operator,
                                    operand: NodeIndex::NONE,
                                },
                            );
                        }
                        // TS1109: ++await and --await are invalid because await
                        // expressions are not valid left-hand-side expressions
                        // for increment/decrement.
                        SyntaxKind::AwaitKeyword => {
                            self.error_expression_expected();
                            // In async context, parse the full await expression
                            // (including operand like `42`) so tokens are consumed
                            // and no spurious TS1005 follows.
                            if self.in_async_context() {
                                let operand = self.parse_unary_expression();
                                let end_pos = self.token_end();
                                return self.arena.add_unary_expr(
                                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                                    start_pos,
                                    end_pos,
                                    UnaryExprData { operator, operand },
                                );
                            }
                        }
                        _ => {}
                    }
                }
                // For prefix ++/-- (update operators), parse only a
                // LeftHandSideExpression as the operand — matching tsc's
                // parseUpdateExpression which calls
                // parseLeftHandSideExpressionOrHigher, NOT
                // parseUnaryExpressionOrHigher.  This prevents `--x--`
                // from being parsed as `--(x--)`.  Instead, `--x` is one
                // expression statement, and the trailing `--;` triggers
                // TS1005 (';' expected) + TS1109 (Expression expected).
                //
                // For other prefix unary operators (+, -, ~, !, typeof,
                // void, delete), the operand is still a full
                // UnaryExpression.
                let operand = if is_update_operator {
                    self.parse_left_hand_side_expression()
                } else {
                    self.parse_unary_expression()
                };
                if operand.is_none() {
                    // When a prefix unary operator has no operand, emit TS1109 at
                    // the current position unconditionally. tsc emits this via
                    // parsePrimaryExpression's default case -> createMissingNode,
                    // which uses only exact-position dedup (no distance-based
                    // suppression). Bypass should_report_error() so a prior
                    // TS1005 at the operator itself (e.g. `,` expected at `~` in
                    // `var a = q~;`) does not swallow the distinct missing-operand
                    // error. parse_error_at already dedupes at the same position,
                    // so this won't double up when the recursive call already
                    // reported at the same token.
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            // `*` is only a binary operator (multiplication, etc.). Fall through to
            // the default path so `parse_primary_expression`'s `is_binary_operator`
            // branch reports TS1109 and returns a missing LHS without advancing,
            // matching tsc's `parsePrimaryExpression -> createMissingNode` flow.
            // The outer `parse_binary_expression_chain` then consumes `*` as a
            // binary operator, which is the correct tree shape for recovery
            // (e.g. `import type defer * as ns1 from "./a";` parses `* as`
            // as a binary expression and produces `;' expected` on `ns1`,
            // matching tsc).
            SyntaxKind::TypeOfKeyword | SyntaxKind::VoidKeyword | SyntaxKind::DeleteKeyword => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                self.next_token();
                let operand = self.parse_unary_expression();
                if operand.is_none() {
                    // Emit TS1109 for incomplete unary expression: typeof[missing], void[missing], delete[missing]
                    self.error_expression_expected();
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            SyntaxKind::AwaitKeyword => {
                // Check if 'await' is followed by an expression
                let snapshot = self.scanner.save_state();
                let current_token = self.current_token;
                self.next_token(); // consume 'await'
                let next_token = self.token();
                self.scanner.restore_state(snapshot);
                self.current_token = current_token;

                let has_following_expression = !matches!(
                    next_token,
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CloseBracketToken
                        | SyntaxKind::CommaToken
                        | SyntaxKind::ColonToken
                        | SyntaxKind::EqualsGreaterThanToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::EndOfFileToken
                        | SyntaxKind::CloseBraceToken
                );

                // In static block context with a following expression, but NOT in an async context
                // (i.e., directly in the static block, not in a nested async function),
                // emit TS18037 and parse as await expression for correct AST structure
                if self.in_static_block_context()
                    && !self.in_async_context()
                    && has_following_expression
                {
                    self.parse_error_at_current_token(
                        "'await' expression cannot be used inside a class static block.",
                        diagnostic_codes::AWAIT_EXPRESSION_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
                    );
                    // Fall through to parse as await expression
                } else if !self.in_async_context()
                    && has_following_expression
                    && !self.in_parameter_default_context()
                    && (next_token != SyntaxKind::OpenParenToken
                        || !self.in_function_body_context())
                {
                    // Parse as await expression - the checker will emit TS1308
                    // (not TS1359 from the parser) to match TSC behavior
                } else if self.in_async_context()
                    && self.in_parameter_default_context()
                    && has_following_expression
                {
                    // Note: TS2524 ('await' expressions cannot be used in a parameter initializer)
                    // is emitted by the checker, not the parser, matching TSC behavior.
                    // Fall through to parse as await expression for error recovery
                } else if !self.in_async_context() {
                    // NOT in async context - 'await' should be treated as identifier
                    // In parameter default context of non-async functions, 'await' is a valid identifier
                    if self.in_parameter_default_context() && !has_following_expression {
                        // Parse 'await' as regular identifier in parameter defaults of non-async functions
                        let start_pos = self.token_pos();
                        let end_pos = self.token_end(); // capture end before consuming
                        let atom = self.scanner.get_token_atom();
                        self.next_token(); // consume the await token
                        return self.arena.add_identifier(
                            SyntaxKind::Identifier as u16,
                            start_pos,
                            end_pos,
                            crate::parser::node::IdentifierData {
                                atom,
                                escaped_text: String::from("await"),
                                original_text: None,
                                type_arguments: None,
                            },
                        );
                    }

                    // Outside async context or in other contexts, check if await is used as a bare expression
                    // If followed by tokens that can't start an expression, report "Expression expected"
                    // Examples where await is a reserved identifier but invalid as expression:
                    //   await;  // Error: TS1359 in static blocks (reserved word)
                    //   await (1);  // Error: Expression expected (in static blocks)
                    //   async (a = await => x) => {}  // Error: Expression expected (before arrow)

                    // Special case: Don't emit TS1109 for 'await' in computed property names like { [await]: foo }
                    // In this context, 'await' is used as an identifier and CloseBracketToken is expected
                    let is_computed_property_context = next_token == SyntaxKind::CloseBracketToken;
                    // Special case: Don't emit TS1109 for 'await' when followed by colon (labeled statement)
                    // The labeled statement parser will emit TS1109 (Expression expected) in static blocks
                    let is_label_context = next_token == SyntaxKind::ColonToken;

                    if !has_following_expression
                        && !is_computed_property_context
                        && !is_label_context
                        && self.in_static_block_context()
                    {
                        // In static blocks, tsc treats `await` as a keyword and
                        // emits TS1109 at the token AFTER `await` (the missing
                        // operand position), matching await-expression parsing.
                        let start_pos = self.token_pos();
                        self.next_token(); // consume `await`
                        self.error_expression_expected();
                        let end_pos = self.token_end();
                        return self.arena.add_unary_expr_ex(
                            syntax_kind_ext::AWAIT_EXPRESSION,
                            start_pos,
                            end_pos,
                            UnaryExprDataEx {
                                expression: NodeIndex::NONE,
                                asterisk_token: false,
                            },
                        );
                    }
                    // Outside static blocks and async contexts, 'await' without a following
                    // expression is a valid identifier (e.g., inside nested function bodies
                    // within static blocks, or in non-module script code). Don't emit TS1109;
                    // fall through to parse as identifier via parse_postfix_expression().

                    // Fall through to parse as identifier/postfix expression
                    return self.parse_postfix_expression();
                }

                // In async context, parse as await expression
                let start_pos = self.token_pos();
                self.consume_keyword(); // TS1260 check for await keyword with escapes

                // In parameter-default context, `await =>` reports a missing operand.
                //
                // In arrow function parameters (`CONTEXT_FLAG_ARROW_PARAMETERS`):
                //   Emit TS1109 at the `await` keyword and do NOT consume `=>`.
                //   The parameter-list recovery will then emit TS1005 "',' expected"
                //   at `=>`, giving the code set {TS1005, TS1109} matching tsc.
                //   Example: `async (a = await => await) => {}` → TS1109 + TS1005.
                //
                // In regular function parameters (no arrow context):
                //   Emit TS1109 at `=>` and consume `=>` + following token for recovery.
                //   Example: `async function foo(a = await => await) {}` → only TS1109.
                if self.in_parameter_default_context()
                    && self.is_token(SyntaxKind::EqualsGreaterThanToken)
                {
                    let in_arrow_params = (self.context_flags & CONTEXT_FLAG_ARROW_PARAMETERS) != 0;
                    if in_arrow_params {
                        // Emit TS1109 at await position (different from =>) to avoid
                        // position-based dedup with the TS1005 from parameter list.
                        self.parse_error_at(
                            start_pos,
                            keyword_text_len(SyntaxKind::AwaitKeyword),
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                    } else {
                        // Regular function: emit at => and consume for recovery
                        self.error_expression_expected();
                        self.next_token(); // consume `=>`
                        if !self.is_token(SyntaxKind::CloseParenToken)
                            && !self.is_token(SyntaxKind::EndOfFileToken)
                        {
                            self.next_token(); // skip arrow body token
                        }
                    }
                    let end_pos = self.token_end();
                    return self.arena.add_unary_expr_ex(
                        syntax_kind_ext::AWAIT_EXPRESSION,
                        start_pos,
                        end_pos,
                        UnaryExprDataEx {
                            expression: NodeIndex::NONE,
                            asterisk_token: false,
                        },
                    );
                }

                // Unlike return/throw, `await` does NOT participate in ASI
                // for its operand. `await\n1` parses as `await 1`, not `await; 1;`.
                // Only emit TS1109 when the next token truly can't start an expression
                // (`;`, `)`, `}`, EOF, etc.), not when there's a line break before a valid expr.
                if !self.is_expression_start() {
                    self.error_expression_expected();
                }

                let expression = self.parse_unary_expression();
                let end_pos = self.token_end();

                self.arena.add_unary_expr_ex(
                    syntax_kind_ext::AWAIT_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprDataEx {
                        expression,
                        asterisk_token: false,
                    },
                )
            }
            SyntaxKind::YieldKeyword => {
                if self.in_class_member_name()
                    && !self.in_generator_context()
                    && !self.is_computed_class_member_yield_expression()
                {
                    return self.parse_identifier_name();
                }

                // Check if 'yield' is followed by a token that disambiguates
                // between yield-expression and yield-as-identifier.
                let snapshot = self.scanner.save_state();
                let current_token = self.current_token;
                self.next_token(); // consume 'yield'

                // For non-generator context: tsc only parses yield as a yield expression
                // when the next token on the same line is an identifier, keyword, or literal.
                // This matches tsc's `nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`.
                // e.g., `yield foo;` → yield expression (TS1163)
                // e.g., `yield(foo);` → identifier + call (checker emits TS1212)
                // e.g., `yield * x;` → identifier * x (checker emits TS1212)
                let next_is_ident_keyword_or_literal_on_same_line =
                    !self.scanner.has_preceding_line_break()
                        && (crate::parser::parse_rules::is_identifier_or_keyword(self.token())
                            || matches!(
                                self.token(),
                                SyntaxKind::NumericLiteral
                                    | SyntaxKind::BigIntLiteral
                                    | SyntaxKind::StringLiteral
                            ));

                self.scanner.restore_state(snapshot);
                self.current_token = current_token;

                // Outside a generator context: use tsc's disambiguation rule.
                // Only parse as yield expression (for TS1163 error recovery) when the
                // next token on the same line is an identifier, keyword, or literal.
                // Otherwise parse as an identifier (the checker will emit TS1212 in
                // strict mode for `yield` as a reserved word).
                if !self.in_generator_context() && next_is_ident_keyword_or_literal_on_same_line {
                    self.parse_error_at_current_token(
                        "A 'yield' expression is only allowed in a generator body.",
                        diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY,
                    );
                    // Fall through to parse as yield expression
                } else if !self.in_generator_context() {
                    // Outside a generator context and next token is not identifier/keyword/
                    // literal on same line — 'yield' is a regular identifier.
                    // e.g., `yield(foo)` → call expression, `yield * x` → multiplication,
                    //        `function f(yield = yield) {}` → identifier
                    let start_pos = self.token_pos();
                    let end_pos = self.token_end();
                    let atom = self.scanner.get_token_atom();
                    self.next_token();
                    return self.arena.add_identifier(
                        SyntaxKind::Identifier as u16,
                        start_pos,
                        end_pos,
                        IdentifierData {
                            atom,
                            escaped_text: String::from("yield"),
                            original_text: None,
                            type_arguments: None,
                        },
                    );
                }

                let start_pos = self.token_pos();

                // Note: TS2523 ('yield' expressions cannot be used in a parameter initializer)
                // is emitted by the checker, not the parser, matching TSC behavior.

                self.consume_keyword(); // TS1260 check for yield keyword with escapes

                // Check for yield* (delegate yield)
                let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

                // Parse the expression (may be empty for bare yield)
                let expression = if !self.scanner.has_preceding_line_break()
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::ColonToken)
                    && !self.is_token(SyntaxKind::CommaToken)
                    && !self.is_token(SyntaxKind::EqualsGreaterThanToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.parse_assignment_expression()
                } else {
                    NodeIndex::NONE
                };

                // yield * requires an expression (TS1109: Expression expected)
                if asterisk_token && expression.is_none() {
                    self.error_expression_expected();
                }

                let end_pos = self.token_end();

                self.arena.add_unary_expr_ex(
                    syntax_kind_ext::YIELD_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprDataEx {
                        expression,
                        asterisk_token,
                    },
                )
            }
            _ => self.parse_postfix_expression(),
        }
    }

    // Parse postfix expression
    pub(crate) fn parse_postfix_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_left_hand_side_expression();

        // Handle postfix operators
        if !self.scanner.has_preceding_line_break()
            && (self.is_token(SyntaxKind::PlusPlusToken)
                || self.is_token(SyntaxKind::MinusMinusToken))
        {
            let operator = self.token() as u16;
            let end_pos = self.token_end();
            self.next_token();

            expr = self.arena.add_unary_expr(
                syntax_kind_ext::POSTFIX_UNARY_EXPRESSION,
                start_pos,
                end_pos,
                UnaryExprData {
                    operator,
                    operand: expr,
                },
            );
        }

        expr
    }

    // Parse left-hand side expression (member access, call, etc.)
    pub(crate) fn parse_left_hand_side_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    let missing_name_pos = self.token_end();
                    if let Some(node) = self.arena.get(expr)
                        && node.kind
                            == crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                        && let Some(eta) = self.arena.get_expr_type_args(node)
                    {
                        // TSC emits TS1477 at the `<…>` type-argument span (from `<` to
                        // past `>`), not at the whole expression start. This avoids
                        // setting THIS_NODE_HAS_ERROR on the expression identifier itself,
                        // which would suppress TS2304 for unresolved names like `List`.
                        //
                        // tsc's formula: `pos = typeArguments.pos - 1` (the `<`),
                        // `end = skipTrivia(typeArguments.end) + 1` (past `>`). Prefer
                        // the first type argument's start - 1 so the column points to
                        // the `<` itself even when whitespace separates `b` from `<`.
                        // Fall back to the expression's end when no args are available.
                        let first_arg_pos = eta
                            .type_arguments
                            .as_ref()
                            .and_then(|list| list.nodes.first())
                            .and_then(|&idx| self.arena.get(idx))
                            .map(|n| n.pos);
                        let err_pos =
                            first_arg_pos
                                .map(|p| p.saturating_sub(1))
                                .unwrap_or_else(|| {
                                    self.arena
                                        .get(eta.expression)
                                        .map_or(node.pos, |expr_node| expr_node.end)
                                });
                        let err_len = node.end.saturating_sub(err_pos);
                        self.parse_error_at(
                            err_pos,
                            err_len,
                            tsz_common::diagnostics::diagnostic_messages::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                            tsz_common::diagnostics::diagnostic_codes::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                        );
                    }
                    self.next_token();
                    // Handle both regular identifiers and private identifiers (#name)
                    // Also try rescanning HashToken as PrivateIdentifier.
                    if self.is_token(SyntaxKind::HashToken) {
                        let rescanned = self.scanner.re_scan_hash_token();
                        self.current_token = rescanned;
                    }
                    let is_private_identifier = self.is_token(SyntaxKind::PrivateIdentifier);
                    let is_optional_chain_continuation =
                        is_private_identifier && self.is_optional_chain_expression(expr);
                    let name = if is_private_identifier {
                        self.parse_private_identifier()
                    } else if self.is_token(SyntaxKind::HashToken) {
                        // Bare `#` after `.` — emit TS1127 like tsc's scanner does.
                        self.parse_error_at_current_token(
                            tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                            tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                        );
                        self.next_token();
                        NodeIndex::NONE
                    } else if self.is_identifier_or_keyword() {
                        // When there's a line break after the dot and the current token
                        // starts a declaration (e.g. `foo.\nvar y = 1;`), don't consume
                        // the token as a property name. Instead, emit TS1003 and create
                        // a missing identifier. This matches tsc's parseRightSideOfDot.
                        if self.scanner.has_preceding_line_break()
                            && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
                        {
                            self.parse_error_at(
                                missing_name_pos,
                                0,
                                "Identifier expected.",
                                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                            NodeIndex::NONE
                        } else {
                            self.parse_identifier_name()
                        }
                    } else {
                        // Emit at the current token position (reportAtCurrentPosition: true),
                        // matching tsc's parseRightSideOfDot/createMissingNode behavior.
                        // This ensures the TS1003 error is at the same position as where
                        // parseExpected(CloseParenToken) would emit TS1005, allowing the
                        // duplicate-position suppression to prevent cascading errors.
                        let missing_pos = if self.is_token(SyntaxKind::EndOfFileToken) {
                            missing_name_pos
                        } else {
                            self.token_pos()
                        };
                        if self.is_token(SyntaxKind::Unknown) {
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                                tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                            );
                        } else {
                            self.parse_error_at(
                                missing_pos,
                                0,
                                "Identifier expected.",
                                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                        }
                        NodeIndex::NONE
                    };
                    if is_optional_chain_continuation && let Some(name_node) = self.arena.get(name)
                    {
                        self.parse_error_at(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            tsz_common::diagnostics::diagnostic_messages::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                            tsz_common::diagnostics::diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                        );
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
                    // In decorator context, `[` starts a computed property name, not element access
                    if (self.context_flags & crate::parser::state::CONTEXT_FLAG_IN_DECORATOR) != 0 {
                        break;
                    }
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
                SyntaxKind::OpenParenToken => {
                    let callee_expr = expr;
                    self.next_token();
                    let arguments = self.parse_argument_list();
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseParenToken);

                    let is_optional_chain = self
                        .arena
                        .get(callee_expr)
                        .and_then(|callee_node| self.arena.get_access_expr(callee_node))
                        .is_some_and(|access| access.question_dot_token);
                    let call_expr = self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        CallExprData {
                            expression: expr,
                            type_arguments: None,
                            arguments: Some(arguments),
                        },
                    );
                    let optional_chain_flag = self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                    if is_optional_chain && let Some(call_node) = self.arena.get_mut(call_expr) {
                        call_node.flags |= optional_chain_flag;
                    }
                    expr = call_expr;
                }
                // Tagged template literals: tag`template` or tag`head${expr}tail`
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    self.in_tagged_template = true;
                    let template = self.parse_template_literal();
                    self.in_tagged_template = false;
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
                // Optional chaining: expr?.prop, expr?.[index], expr?.()
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if !self.is_js_file()
                        && self.is_less_than_or_compound()
                        && let Some(type_args) = self.try_parse_type_arguments_for_call()
                    {
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            // expr?.<T>()
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            let call_expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                            let optional_chain_flag =
                                self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                            if let Some(call_node) = self.arena.get_mut(call_expr) {
                                call_node.flags |= optional_chain_flag;
                            }
                            expr = call_expr;
                            continue;
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            self.in_tagged_template = true;
                            let template = self.parse_template_literal();
                            self.in_tagged_template = false;
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                            continue;
                        }
                        // expr?.<T> not followed by `(` or a template literal.
                        // tsc emits TS1005 ('(' expected) here. Do NOT fall
                        // through to the property-access path, which would call
                        // parse_identifier_name() and emit the spurious TS1003.
                        self.parse_expected(SyntaxKind::OpenParenToken);
                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            self.token_pos(),
                            CallExprData {
                                expression: expr,
                                type_arguments: Some(type_args),
                                arguments: Some(self.make_node_list(Vec::new())),
                            },
                        );
                        let optional_chain_flag =
                            self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= optional_chain_flag;
                        }
                        expr = call_expr;
                        continue;
                    }
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // expr?.[index]
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
                    } else if self.is_token(SyntaxKind::OpenParenToken) {
                        // expr?.()
                        self.next_token();
                        let arguments = self.parse_argument_list();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseParenToken);

                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            end_pos,
                            CallExprData {
                                expression: expr,
                                type_arguments: None,
                                arguments: Some(arguments),
                            },
                        );
                        let optional_chain_flag =
                            self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= optional_chain_flag;
                        }
                        expr = call_expr;
                    } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                        || self.is_token(SyntaxKind::TemplateHead)
                    {
                        // expr?.`template` — tagged template in optional chain is not allowed.
                        // tsc emits TS1358 and still parses the tagged template expression.
                        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.parse_error_at_current_token(
                            diagnostic_messages::TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN,
                            diagnostic_codes::TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN,
                        );
                        self.in_tagged_template = true;
                        let template = self.parse_template_literal();
                        self.in_tagged_template = false;
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
                        continue;
                    } else {
                        // expr?.prop
                        let is_private_identifier = self.is_token(SyntaxKind::PrivateIdentifier);
                        let name = if is_private_identifier {
                            self.parse_private_identifier()
                        } else {
                            self.parse_identifier_name()
                        };

                        // TS18030: Optional chain cannot contain private identifiers
                        if is_private_identifier && let Some(name_node) = self.arena.get(name) {
                            self.parse_error_at(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    "An optional chain cannot contain private identifiers.",
                                    diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                                );
                        }

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
                // Non-null assertion: expr!
                SyntaxKind::ExclamationToken => {
                    // Non-null assertion only if no line break before
                    if self.scanner.has_preceding_line_break() {
                        break;
                    }
                    self.next_token();
                    let end_pos = self.token_end();

                    expr = self.arena.add_unary_expr_ex(
                        syntax_kind_ext::NON_NULL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::UnaryExprDataEx {
                            expression: expr,
                            asterisk_token: false,
                        },
                    );
                }
                // Type arguments followed by call: expr<T>() or expr<T, U>()
                // Also handles `<<` for nested generics: foo<<T>(x: T) => number>(fn)
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
                    if self.is_js_file() {
                        break;
                    }
                    if self
                        .arena
                        .get(expr)
                        .is_some_and(|node| node.kind == SyntaxKind::SuperKeyword as u16)
                    {
                        let type_arg_start = self.token_pos();
                        let type_args = self.parse_type_arguments();
                        let type_arg_end = self.token_full_start();
                        self.parse_error_at(
                            type_arg_start,
                            (type_arg_end.saturating_sub(type_arg_start)).max(1),
                            tsz_common::diagnostics::diagnostic_messages::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                            tsz_common::diagnostics::diagnostic_codes::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                        );
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);
                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                                tsz_common::diagnostics::diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                            );
                            self.in_tagged_template = true;
                            let template = self.parse_template_literal();
                            self.in_tagged_template = false;
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else if !self.is_token(SyntaxKind::DotToken)
                            && !self.is_token(SyntaxKind::OpenBracketToken)
                        {
                            // TS1034: super<T> followed by something other than
                            // call/member access (e.g., tagged template literal)
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                                tsz_common::diagnostics::diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                            );
                        }
                        continue;
                    }

                    // Try to parse as type arguments for a call expression
                    // This is tricky because < could be comparison operator
                    if let Some(type_args) = self.try_parse_type_arguments_for_call() {
                        // After type arguments, we expect ( for a call or ` for tagged template
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            // Tagged template with type arguments: tag<T>`template`
                            self.in_tagged_template = true;
                            let template = self.parse_template_literal();
                            self.in_tagged_template = false;
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else {
                            // Not a call or tagged template - this is an instantiation expression
                            // (e.g., f<string>, new Foo<number>, a<b>?.())
                            let end_pos = self.token_end();
                            expr = self.arena.add_expr_with_type_args(
                                crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                                start_pos,
                                end_pos,
                                crate::parser::node::ExprWithTypeArgsData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                },
                            );
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        expr
    }
}
