use tsz_common::diagnostics::diagnostic_codes;

/// Parser state - binary/conditional/as-satisfies expression parsing
use super::state::{CONTEXT_FLAG_IN_CONDITIONAL_TRUE, ParserState};
use crate::parser::{
    NodeIndex,
    node::{BinaryExprData, ConditionalExprData},
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn parse_binary_expression_chain(
        &mut self,
        min_precedence: u8,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_binary_expression_chain_seeded(min_precedence, start_pos, None)
    }

    /// Binary-expression precedence climbing with an optional pre-parsed left
    /// operand. When `seed` is `Some`, that node is used as the initial left
    /// operand instead of calling `parse_unary_expression`. This mirrors tsc's
    /// `parseBinaryExpressionRest(precedence, leftOperand)`, which is reached
    /// during error recovery when `parsePrimaryExpression` synthesizes a missing
    /// identifier (via `createMissingNode`) for a statement that begins with a
    /// binary operator. The operator is then consumed by this loop and the
    /// missing node becomes the binary expression's left operand, so the
    /// emitted tree is `<missing> <op> <rhs>` rather than dropping the operator.
    pub(crate) fn parse_binary_expression_chain_seeded(
        &mut self,
        min_precedence: u8,
        start_pos: u32,
        seed: Option<NodeIndex>,
    ) -> NodeIndex {
        let mut left = match seed {
            Some(node) => node,
            None => self.parse_unary_expression(),
        };

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

    pub(crate) fn is_assignment_target_with_block_bodied_arrow(&self, node: NodeIndex) -> bool {
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

    pub(crate) fn is_block_bodied_arrow_function(&self, node: NodeIndex) -> bool {
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

    pub(crate) fn parse_binary_expression_remainder(
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

        let right = self.parse_binary_expression_rhs(op, precedence);
        // Use token_full_start() (start of next lookahead token's trivia) rather than
        // token_end() (end of that token). After parse_binary_expression_rhs returns, the
        // scanner sits on the first token not part of this expression. token_full_start()
        // matches tsc's finishNode(scanner.getTokenFullStart()) convention.
        let end_pos = self.token_full_start();
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

    pub(crate) fn parse_conditional_expression(
        &mut self,
        condition: NodeIndex,
        start_pos: u32,
    ) -> NodeIndex {
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
        // Use token_full_start() rather than token_end(); same convention as
        // parse_binary_expression_remainder and state_types.rs union/intersection
        // types: after the last branch is parsed, the scanner sits on the first
        // token not part of this expression, so token_full_start() gives the
        // correct node end, matching tsc's finishNode(scanner.getTokenFullStart()).
        let end_pos = self.token_full_start();

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

    pub(crate) fn parse_binary_expression_rhs(
        &mut self,
        op: SyntaxKind,
        precedence: u8,
    ) -> NodeIndex {
        let right = self.parse_binary_rhs_operand(op, precedence);
        if right.is_none() {
            return self.recover_missing_binary_rhs();
        }

        right
    }

    pub(crate) fn parse_binary_rhs_operand(&mut self, op: SyntaxKind, precedence: u8) -> NodeIndex {
        if self.is_assignment_operator(op) {
            self.parse_assignment_expression()
        } else {
            self.parse_binary_expression(Self::binary_rhs_precedence(op, precedence))
        }
    }

    pub(crate) fn recover_missing_binary_rhs(&mut self) -> NodeIndex {
        self.report_missing_binary_rhs();

        let recovered = self.try_recover_binary_rhs();
        if !recovered.is_none() {
            return recovered;
        }

        // Create a missing expression placeholder instead of returning the
        // left operand. Returning the left operand would duplicate it in the
        // parent binary expression (for example, `1 > > 2` would become
        // `1 > 1 > 2` instead of `1 >  > 2`). A missing expression keeps the
        // AST structurally correct and the emitter will output nothing for it.
        self.create_missing_expression()
    }

    pub(crate) fn report_missing_binary_rhs(&mut self) {
        // Emit TS1109 directly, bypassing distance-based suppression. tsc only
        // suppresses at the exact same position, so a missing RHS after a binary
        // operator always emits TS1109 even if a prior error (for example,
        // TS1003 from JSX) is nearby.
        if !self.should_suppress_missing_binary_rhs_error() {
            if self.is_token(SyntaxKind::EndOfFileToken) {
                // At EOF, tsc's `createMissingNode` anchors the diagnostic at
                // the position right after the last real token (before any
                // trailing trivia, e.g. a final newline, is skipped) rather
                // than at the EOF token's own post-trivia position — compare
                // `a &&;`/`a && ;` (mid-line, anchored at the next real
                // token) with `a &&\n` (anchored one column after `&&`, not
                // at the start of the following line).
                let pos = self.token_full_start();
                self.parse_error_at(
                    pos,
                    0,
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            } else {
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
        }
    }

    pub(crate) fn should_suppress_missing_binary_rhs_error(&self) -> bool {
        !self.is_js_file()
            && self.is_token(SyntaxKind::GreaterThanToken)
            && self
                .get_source_text()
                .get(self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize)
                == Some("<")
    }

    pub(crate) const fn binary_rhs_precedence(op: SyntaxKind, precedence: u8) -> u8 {
        if matches!(op, SyntaxKind::AsteriskAsteriskToken) {
            precedence
        } else {
            precedence + 1
        }
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
        // token_full_start() matches tsc's finishNode default (scanner.getTokenFullStart()).
        // token_end() overshoots: it returns the end of the *next* token, not the type.
        let end_pos = self.token_full_start();

        self.arena.add_type_assertion(
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
        )
    }
}
